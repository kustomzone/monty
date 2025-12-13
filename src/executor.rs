use crate::exceptions::{InternalRunError, RunError};
use crate::expressions::Node;
use crate::heap::Heap;
use crate::intern::Interns;
use crate::namespace::Namespaces;
use crate::object::PyObject;
use crate::parse::parse;
use crate::parse_error::ParseError;
use crate::position::{FrameExit, NoPositionTracker, Position, PositionTracker};
use crate::prepare::prepare;
use crate::resource::NoLimitTracker;
use crate::resource::{LimitedTracker, ResourceLimits, ResourceTracker};
use crate::run_frame::RunFrame;
use crate::value::Value;

/// Main executor that parses and runs Python code.
///
/// The executor stores the compiled AST.
#[derive(Debug, Clone)]
pub struct Executor {
    namespace_size: usize,
    /// Maps variable names to their indices in the namespace. Used for ref-count testing.
    #[cfg(feature = "ref-counting")]
    name_map: ahash::AHashMap<String, crate::namespace::NamespaceId>,
    nodes: Vec<Node>,
    /// Interned strings used for looking up names and filenames during execution.
    interns: Interns,
}

impl Executor {
    /// Creates a new executor with the given code, filename, and input names.
    ///
    /// # Arguments
    /// * `code` - The Python code to execute.
    /// * `filename` - The filename of the Python code.
    /// * `input_names` - The names of the input variables.
    ///
    /// # Returns
    /// A new `Executor` instance which can be used to execute the code.
    pub fn new(code: &str, filename: &str, input_names: &[&str]) -> Result<Self, ParseError> {
        let parse_result = parse(code, filename)?;
        let prepared = prepare(parse_result, input_names)?;
        Ok(Self {
            namespace_size: prepared.namespace_size,
            #[cfg(feature = "ref-counting")]
            name_map: prepared.name_map,
            nodes: prepared.nodes,
            interns: Interns::new(prepared.interner, prepared.functions),
        })
    }

    /// Executes the code with the given input values.
    ///
    /// # Arguments
    /// * `inputs` - Values to fill the first N slots of the namespace (e.g., function parameters)
    ///
    /// # Example
    /// ```
    /// use std::time::Duration;
    /// use monty::Executor;
    ///
    /// let ex = Executor::new("1 + 2", "test.py", &[]).unwrap();
    /// let py_object = ex.run_no_limits(vec![]).unwrap();
    /// assert_eq!(py_object, monty::PyObject::Int(3));
    /// ```
    pub fn run_no_limits(&self, inputs: Vec<PyObject>) -> Result<PyObject, RunError> {
        self.run_with_tracker(inputs, NoLimitTracker::default())
    }

    /// Executes the code with configurable resource limits.
    ///
    /// # Arguments
    /// * `inputs` - Values to fill the first N slots of the namespace
    /// * `limits` - Resource limits to enforce during execution
    ///
    /// # Example
    /// ```
    /// use std::time::Duration;
    /// use monty::{Executor, ResourceLimits, PyObject};
    ///
    /// let limits = ResourceLimits::new()
    ///     .max_allocations(1000)
    ///     .max_duration(Duration::from_secs(5));
    /// let ex = Executor::new("1 + 2", "test.py", &[]).unwrap();
    /// let py_object = ex.run_with_limits(vec![], limits).unwrap();
    /// assert_eq!(py_object, PyObject::Int(3));
    /// ```
    pub fn run_with_limits(&self, inputs: Vec<PyObject>, limits: ResourceLimits) -> Result<PyObject, RunError> {
        let resource_tracker = LimitedTracker::new(limits);
        self.run_with_tracker(inputs, resource_tracker)
    }

    /// Executes the code with a custom resource tracker.
    ///
    /// This provides full control over resource tracking and garbage collection
    /// scheduling. The tracker is called on each allocation and periodically
    /// during execution to check time limits and trigger GC.
    ///
    /// # Arguments
    /// * `inputs` - Values to fill the first N slots of the namespace
    /// * `resource_tracker` - Custom resource tracker implementation
    ///
    /// # Type Parameters
    /// * `T` - A type implementing `ResourceTracker`
    fn run_with_tracker<T: ResourceTracker>(
        &self,
        inputs: Vec<PyObject>,
        resource_tracker: T,
    ) -> Result<PyObject, RunError> {
        let mut heap = Heap::new(self.namespace_size, resource_tracker);
        let mut namespaces = self.prepare_namespaces(inputs, &mut heap)?;

        let mut position_tracker = NoPositionTracker;
        let mut frame = RunFrame::module_frame(&self.interns, &mut position_tracker);
        let frame_result = frame.execute(&mut namespaces, &mut heap, &self.nodes);

        // Clean up the global namespace before returning (only needed with dec-ref-check)
        #[cfg(feature = "dec-ref-check")]
        namespaces.drop_global_with_heap(&mut heap);

        frame_result.map(|frame_exit| match frame_exit {
            Some(exit) => PyObject::new(exit.into(), &mut heap, &self.interns),
            None => PyObject::None,
        })
    }

    /// Executes the code and returns both the result and reference count data.
    ///
    /// This is used for testing reference counting behavior. Returns:
    /// - The execution result (`Exit`)
    /// - Reference count data as a tuple of:
    ///   - A map from variable names to their reference counts (only for heap-allocated values)
    ///   - The number of unique heap value IDs referenced by variables
    ///   - The total number of live heap values
    ///
    /// For strict matching validation, compare unique_refs_count with heap_entry_count.
    /// If they're equal, all heap values are accounted for by named variables.
    ///
    /// Only available when the `ref-counting` feature is enabled.
    #[cfg(feature = "ref-counting")]
    pub fn run_ref_counts(&self, inputs: Vec<PyObject>) -> RunRefCountsResult {
        use crate::value::Value;
        use std::collections::HashSet;

        let mut heap = Heap::new(self.namespace_size, NoLimitTracker::default());
        let mut namespaces = self.prepare_namespaces(inputs, &mut heap)?;

        let mut position_tracker = NoPositionTracker;
        let mut frame = RunFrame::module_frame(&self.interns, &mut position_tracker);
        let result = frame.execute(&mut namespaces, &mut heap, &self.nodes);

        // Compute ref counts before consuming the heap
        let final_namespace = namespaces.into_global();
        let mut counts = ahash::AHashMap::new();
        let mut unique_ids = HashSet::new();

        for (name, &namespace_id) in &self.name_map {
            if let Some(Value::Ref(id)) = final_namespace.get_opt(namespace_id) {
                counts.insert(name.clone(), heap.get_refcount(*id));
                unique_ids.insert(*id);
            }
        }
        let ref_count_data: RefCountSnapshot = (counts, unique_ids.len(), heap.entry_count());

        // Clean up the namespace after reading ref counts but before moving the heap
        for obj in final_namespace {
            obj.drop_with_heap(&mut heap);
        }

        let python_value = result.map(|frame_exit| match frame_exit {
            Some(exit) => PyObject::new(exit.into(), &mut heap, &self.interns),
            None => PyObject::None,
        })?;

        Ok((python_value, ref_count_data))
    }

    /// Prepares the namespace namespaces for execution.
    ///
    /// Converts each `PyObject` input to a `Value`, allocating on the heap if needed.
    /// Returns the prepared Namespaces or an error if there are too many inputs or invalid input types.
    fn prepare_namespaces<T: ResourceTracker>(
        &self,
        inputs: Vec<PyObject>,
        heap: &mut Heap<T>,
    ) -> Result<Namespaces, InternalRunError> {
        let Some(extra) = self.namespace_size.checked_sub(inputs.len()) else {
            return Err(InternalRunError::Error(
                format!("input length should be <= {}", self.namespace_size).into(),
            ));
        };
        // Convert each PyObject to a Value, propagating any invalid input errors
        let mut namespace: Vec<Value> = inputs
            .into_iter()
            .map(|pv| pv.to_value(heap, &self.interns))
            .collect::<Result<_, _>>()
            .map_err(|e| InternalRunError::Error(e.to_string().into()))?;
        if extra > 0 {
            namespace.extend((0..extra).map(|_| Value::Undefined));
        }
        Ok(Namespaces::new(namespace))
    }

    /// Returns the namespace size for this executor.
    fn namespace_size(&self) -> usize {
        self.namespace_size
    }

    /// Returns a reference to the interned strings.
    fn interns(&self) -> &Interns {
        &self.interns
    }

    /// Returns a reference to the AST nodes.
    fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Internal helper to run execution from a position stack.
    ///
    /// Shared by both `ExecutorIter::run` logic below.
    fn run_from_position<T: ResourceTracker>(
        self,
        mut heap: Heap<T>,
        mut namespaces: Namespaces,
        mut position_tracker: PositionTracker,
    ) -> Result<ExecProgress<T>, RunError> {
        let mut frame = RunFrame::module_frame(self.interns(), &mut position_tracker);
        let exit = frame.execute(&mut namespaces, &mut heap, self.nodes())?;

        match exit {
            None => {
                // Clean up the global namespace before returning (only needed with dec-ref-check)
                #[cfg(feature = "dec-ref-check")]
                namespaces.drop_global_with_heap(&mut heap);

                Ok(ExecProgress::Complete(PyObject::None))
            }
            Some(FrameExit::Return(value)) => {
                // Clean up the global namespace before returning (only needed with dec-ref-check)
                #[cfg(feature = "dec-ref-check")]
                namespaces.drop_global_with_heap(&mut heap);

                let py_object = PyObject::new(value, &mut heap, self.interns());
                Ok(ExecProgress::Complete(py_object))
            }
            Some(FrameExit::Yield(value)) => {
                let py_object = PyObject::new(value, &mut heap, self.interns());
                Ok(ExecProgress::Yield {
                    value: py_object,
                    state: YieldExecutorState {
                        executor: self,
                        heap,
                        namespaces,
                        position_stack: position_tracker.stack,
                    },
                })
            }
        }
    }
}

#[cfg(feature = "ref-counting")]
/// Aggregated reference counting statistics returned by `Executor::run_ref_counts`.
type RefCountSnapshot = (ahash::AHashMap<String, usize>, usize, usize);

#[cfg(feature = "ref-counting")]
/// Result type used by `Executor::run_ref_counts`.
type RunRefCountsResult = Result<(PyObject, RefCountSnapshot), RunError>;

/// Result of a single step of iterative execution.
///
/// This enum owns the execution state, ensuring type-safe state transitions.
/// - `Yield` contains the yielded value AND the state needed to resume
/// - `Complete` contains just the final value (execution is done)
///
/// # Type Parameters
/// * `T` - Resource tracker implementation (e.g., `NoLimitTracker` or `LimitedTracker`)
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ExecProgress<T: ResourceTracker> {
    /// Execution yielded with a value. Call `state.run()` to resume.
    Yield {
        /// The value that was yielded.
        value: PyObject,
        /// The execution state that can be resumed. Boxed to reduce enum size.
        state: YieldExecutorState<T>,
    },
    /// Execution completed with a final result.
    Complete(PyObject),
}

impl<T: ResourceTracker> ExecProgress<T> {
    /// Consumes the `ExecProgress` and returns the yielded value and the state needed to resume.
    pub fn into_yield(self) -> Option<(PyObject, YieldExecutorState<T>)> {
        match self {
            ExecProgress::Yield { value, state } => Some((value, state)),
            ExecProgress::Complete(_) => None,
        }
    }

    /// Consumes the `ExecProgress` and returns the final value.
    pub fn into_complete(self) -> Option<PyObject> {
        match self {
            ExecProgress::Complete(value) => Some(value),
            ExecProgress::Yield { .. } => None,
        }
    }
}

/// Execution state that can be resumed after a yield.
///
/// This struct owns all runtime state (executor, heap, namespaces) and provides
/// a `run()` method to continue execution. When `run()` is called, it consumes
/// self and returns the next `ExecProgress`.
///
/// This design ensures type-safe state transitions - you can't accidentally
/// call `run()` on an already-completed execution.
///
/// # Type Parameters
/// * `T` - Resource tracker implementation
#[derive(Debug)]
pub struct YieldExecutorState<T: ResourceTracker> {
    /// The underlying executor containing parsed AST and interns.
    executor: Executor,
    /// The heap for allocating runtime values.
    heap: Heap<T>,
    /// The namespace stack for variable storage.
    namespaces: Namespaces,
    /// Stack of execution positions for resuming inside nested control flow.
    position_stack: Vec<Position>,
}

impl<T: ResourceTracker> YieldExecutorState<T> {
    /// Continues execution from where it yielded.
    ///
    /// Consumes self and returns the next execution progress. This can be
    /// either another `Yield` (with new state to resume) or `Complete`.
    pub fn run(self) -> Result<ExecProgress<T>, RunError> {
        // Convert to internal representation and run from saved position stack
        self.executor
            .run_from_position(self.heap, self.namespaces, self.position_stack.into())
    }
}

/// Iterative executor that supports pausing and resuming execution.
///
/// Unlike `Executor` which runs code to completion, `ExecutorIter` allows
/// execution to be paused at yield points and resumed later. Call `run()`
/// to start execution - it consumes self and returns an `ExecProgress`:
/// - `ExecProgress::Yield { value, state }` - yielded, call `state.run()` to resume
/// - `ExecProgress::Complete(value)` - execution finished
///
/// This enables snapshotting execution state and returning control to the host
/// application during long-running computations.
///
/// The executor is created with `new()` which parses the code, then `run()` is
/// called with inputs and a resource tracker to start execution. The heap and
/// namespaces are created lazily when `run()` is called.
///
/// # Example
/// ```
/// use monty::{ExecutorIter, ExecProgress, NoLimitTracker, PyObject};
///
/// let exec = ExecutorIter::new("x + 1", "test.py", &["x"]).unwrap();
/// match exec.run_no_limits(vec![PyObject::Int(41)]).unwrap() {
///     ExecProgress::Complete(result) => assert_eq!(result, PyObject::Int(42)),
///     ExecProgress::Yield { .. } => panic!("unexpected yield"),
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ExecutorIter {
    /// The underlying executor containing parsed AST and interns.
    executor: Executor,
}

impl ExecutorIter {
    /// Creates a new iterative executor by parsing the given code.
    ///
    /// This only parses and prepares the code - no heap or namespaces are created yet.
    /// Call `run()` with inputs and a resource tracker to start execution.
    ///
    /// # Arguments
    /// * `code` - The Python code to execute
    /// * `filename` - The filename for error messages
    /// * `input_names` - Names of input variables
    ///
    /// # Errors
    /// Returns `ParseError` if the code cannot be parsed.
    pub fn new(code: &str, filename: &str, input_names: &[&str]) -> Result<Self, ParseError> {
        let executor = Executor::new(code, filename, input_names)?;
        Ok(Self { executor })
    }

    /// Starts execution with the given inputs and no resource tracker, consuming self.
    ///
    /// Creates the heap and namespaces, then begins execution. Returns `Yield` with
    /// state to resume, or `Complete` when done.
    ///
    /// # Arguments
    /// * `inputs` - Initial input values (must match length of `input_names` from `new()`)
    ///
    /// # Errors
    /// Returns `RunError` if:
    /// - The number of inputs doesn't match the expected count
    /// - An input value is invalid (e.g., `PyObject::Repr`)
    /// - A runtime error occurs during execution
    pub fn run_no_limits(self, inputs: Vec<PyObject>) -> Result<ExecProgress<NoLimitTracker>, RunError> {
        self.run_with_tracker(inputs, NoLimitTracker::default())
    }

    /// Starts execution with the given inputs and resource limits, consuming self.
    ///
    /// Creates the heap and namespaces, then begins execution. Returns `Yield` with
    /// state to resume, or `Complete` when done.
    ///
    /// # Arguments
    /// * `inputs` - Initial input values (must match length of `input_names` from `new()`)
    /// * `limits` - Resource limits for the execution
    ///
    /// # Errors
    /// Returns `RunError` if:
    /// - The number of inputs doesn't match the expected count
    /// - An input value is invalid (e.g., `PyObject::Repr`)
    /// - A runtime error occurs during execution
    pub fn run_with_limits(
        self,
        inputs: Vec<PyObject>,
        limits: ResourceLimits,
    ) -> Result<ExecProgress<LimitedTracker>, RunError> {
        let resource_tracker = LimitedTracker::new(limits);
        self.run_with_tracker(inputs, resource_tracker)
    }

    fn run_with_tracker<T: ResourceTracker>(
        self,
        inputs: Vec<PyObject>,
        resource_tracker: T,
    ) -> Result<ExecProgress<T>, RunError> {
        let mut heap = Heap::new(self.executor.namespace_size(), resource_tracker);

        let namespaces = self.executor.prepare_namespaces(inputs, &mut heap)?;

        // Start execution from index 0 (beginning of code)
        let position_tracker = PositionTracker::default();
        self.executor.run_from_position(heap, namespaces, position_tracker)
    }
}
