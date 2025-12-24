use crate::evaluate::ExternalCall;
use crate::exception::{ExcType, RunError};
use crate::expressions::Node;
use crate::heap::Heap;
use crate::intern::{ExtFunctionId, Interns};
use crate::io::{PrintWriter, StdPrint};
use crate::namespace::Namespaces;
use crate::object::PyObject;
use crate::parse::{parse, ParseError};
use crate::position::{FrameExit, NoPositionTracker, Position, PositionTracker};
use crate::prepare::prepare;
use crate::resource::NoLimitTracker;
use crate::resource::{LimitedTracker, ResourceLimits, ResourceTracker};
use crate::run_frame::{RunFrame, RunResult};
use crate::value::Value;
use crate::PythonException;

/// Main executor that parses and runs Python code.
///
/// The executor stores the compiled AST and source code for error reporting.
#[derive(Debug, Clone)]
pub struct Executor {
    namespace_size: usize,
    /// Maps variable names to their indices in the namespace. Used for ref-count testing.
    #[cfg(feature = "ref-counting")]
    name_map: ahash::AHashMap<String, crate::namespace::NamespaceId>,
    nodes: Vec<Node>,
    /// Interned strings used for looking up names and filenames during execution.
    interns: Interns,
    /// ids to create values to inject into the the namespace to represent external functions.
    external_function_ids: Vec<ExtFunctionId>,
    /// Source code for error reporting (extracting preview lines for tracebacks).
    source: String,
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
    ///
    /// # Errors
    /// Returns `PythonException` if the code cannot be parsed.
    pub fn new(code: &str, filename: &str, input_names: &[&str]) -> Result<Self, PythonException> {
        Self::new_internal(code, filename, input_names, vec![]).map_err(|e| e.into_python_exc(filename, code))
    }

    fn new_internal(
        code: &str,
        filename: &str,
        input_names: &[&str],
        external_functions: Vec<String>,
    ) -> Result<Self, ParseError> {
        let parse_result = parse(code, filename)?;
        let prepared = prepare(parse_result, input_names, &external_functions)?;

        // incrementing order matches the indexes used in intern::Interns::get_external_function_name
        let external_function_ids = (0..external_functions.len()).map(ExtFunctionId::new).collect();

        Ok(Self {
            namespace_size: prepared.namespace_size,
            #[cfg(feature = "ref-counting")]
            name_map: prepared.name_map,
            nodes: prepared.nodes,
            interns: Interns::new(prepared.interner, prepared.functions, external_functions),
            external_function_ids,
            source: code.to_string(),
        })
    }

    /// Executes the code with the given input values.
    ///
    /// Uses `StdPrint` for print output.
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
    pub fn run_no_limits(&self, inputs: Vec<PyObject>) -> Result<PyObject, PythonException> {
        self.run_with_tracker(inputs, NoLimitTracker::default(), &mut StdPrint)
            .map_err(|e| e.into_python_exception(&self.interns, &self.source))
    }

    /// Executes the code with configurable resource limits.
    ///
    /// Uses `StdPrint` for print output.
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
    pub fn run_with_limits(&self, inputs: Vec<PyObject>, limits: ResourceLimits) -> Result<PyObject, PythonException> {
        let resource_tracker = LimitedTracker::new(limits);
        self.run_with_tracker(inputs, resource_tracker, &mut StdPrint)
            .map_err(|e| e.into_python_exception(&self.interns, &self.source))
    }

    /// Executes the code with a custom print writer.
    ///
    /// This allows capturing or redirecting print output from the executed code.
    ///
    /// # Arguments
    /// * `inputs` - Values to fill the first N slots of the namespace
    /// * `writer` - Custom print writer implementation
    pub fn run_with_writer(
        &self,
        inputs: Vec<PyObject>,
        writer: &mut impl PrintWriter,
    ) -> Result<PyObject, PythonException> {
        self.run_with_tracker(inputs, NoLimitTracker::default(), writer)
            .map_err(|e| e.into_python_exception(&self.interns, &self.source))
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
    /// * `writer` - print writer implementation
    ///
    fn run_with_tracker(
        &self,
        inputs: Vec<PyObject>,
        resource_tracker: impl ResourceTracker,
        writer: &mut impl PrintWriter,
    ) -> Result<PyObject, RunError> {
        let mut heap = Heap::new(self.namespace_size, resource_tracker);
        let mut namespaces = self.prepare_namespaces(inputs, &mut heap)?;

        let mut position_tracker = NoPositionTracker;
        let mut frame = RunFrame::module_frame(&self.interns, &mut position_tracker, writer);
        let frame_exit = frame.execute(&mut namespaces, &mut heap, &self.nodes);

        // Clean up the global namespace before returning (only needed with dec-ref-check)
        #[cfg(feature = "dec-ref-check")]
        namespaces.drop_global_with_heap(&mut heap);

        frame_exit_to_object(frame_exit?, &mut heap, &self.interns)
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
    pub fn run_ref_counts(&self, inputs: Vec<PyObject>) -> Result<RefCountOutput, PythonException> {
        use crate::value::Value;
        use std::collections::HashSet;

        let run = || -> RunResult<RefCountOutput> {
            let mut heap = Heap::new(self.namespace_size, NoLimitTracker::default());
            let mut namespaces = self.prepare_namespaces(inputs, &mut heap)?;

            let mut position_tracker = NoPositionTracker;
            let mut print_writer = StdPrint;
            let mut frame = RunFrame::module_frame(&self.interns, &mut position_tracker, &mut print_writer);
            // Use execute() instead of execute_py_object() so the return value stays alive
            // while we compute refcounts
            let frame_exit = frame.execute(&mut namespaces, &mut heap, &self.nodes)?;

            // Compute ref counts before consuming the heap - return value is still alive in frame_exit
            let final_namespace = namespaces.into_global();
            let mut counts = ahash::AHashMap::new();
            let mut unique_ids = HashSet::new();

            for (name, &namespace_id) in &self.name_map {
                if let Some(Value::Ref(id)) = final_namespace.get_opt(namespace_id) {
                    counts.insert(name.clone(), heap.get_refcount(*id));
                    unique_ids.insert(*id);
                }
            }
            let unique_refs = unique_ids.len();
            let heap_count = heap.entry_count();

            // Clean up the namespace after reading ref counts but before moving the heap
            for obj in final_namespace {
                obj.drop_with_heap(&mut heap);
            }

            // Now convert the return value to PyObject (this drops the Value, decrementing refcount)
            let py_object = frame_exit_to_object(frame_exit, &mut heap, &self.interns)?;

            Ok(RefCountOutput {
                py_object,
                counts,
                unique_refs,
                heap_count,
            })
        };

        run().map_err(|e| e.into_python_exception(&self.interns, &self.source))
    }

    /// Prepares the namespace namespaces for execution.
    ///
    /// Converts each `PyObject` input to a `Value`, allocating on the heap if needed.
    /// Returns the prepared Namespaces or an error if there are too many inputs or invalid input types.
    fn prepare_namespaces(
        &self,
        inputs: Vec<PyObject>,
        heap: &mut Heap<impl ResourceTracker>,
    ) -> Result<Namespaces, RunError> {
        let Some(extra) = self
            .namespace_size
            .checked_sub(self.external_function_ids.len() + inputs.len())
        else {
            return Err(RunError::internal("too many inputs for namespace"));
        };
        // register external functions in the namespace first, matching the logic in prepare
        let mut namespace: Vec<Value> = Vec::with_capacity(self.namespace_size);
        for f_id in &self.external_function_ids {
            namespace.push(Value::ExtFunction(*f_id));
        }
        // Convert each PyObject to a Value, propagating any invalid input errors
        for input in inputs {
            namespace.push(
                input
                    .to_value(heap, &self.interns)
                    .map_err(|_| RunError::internal("invalid input type"))?,
            );
        }
        if extra > 0 {
            namespace.extend((0..extra).map(|_| Value::Undefined));
        }
        Ok(Namespaces::new(namespace))
    }

    /// Internal helper to run execution from a position stack.
    ///
    /// Shared by both `ExecutorIter::run` logic below.
    fn run_from_position<T: ResourceTracker>(
        self,
        mut heap: Heap<T>,
        mut namespaces: Namespaces,
        mut position_tracker: PositionTracker,
        writer: &mut impl PrintWriter,
    ) -> Result<ExecProgress<T>, RunError> {
        let mut frame = RunFrame::module_frame(&self.interns, &mut position_tracker, writer);
        let exit = match frame.execute(&mut namespaces, &mut heap, &self.nodes) {
            Ok(exit) => exit,
            Err(e) => {
                // Clean up before propagating error (only needed with dec-ref-check)
                #[cfg(feature = "dec-ref-check")]
                namespaces.drop_global_with_heap(&mut heap);
                return Err(e);
            }
        };

        match exit {
            None => {
                // Clean up the global namespace before returning (only needed with dec-ref-check)
                #[cfg(feature = "dec-ref-check")]
                namespaces.drop_global_with_heap(&mut heap);

                Ok(ExecProgress::Complete(PyObject::None))
            }
            Some(FrameExit::Return(return_value)) => {
                // Clean up the global namespace before returning (only needed with dec-ref-check)
                #[cfg(feature = "dec-ref-check")]
                namespaces.drop_global_with_heap(&mut heap);

                let py_object = PyObject::new(return_value, &mut heap, &self.interns);
                Ok(ExecProgress::Complete(py_object))
            }
            Some(FrameExit::ExternalCall(ExternalCall { function_id, args })) => {
                let (args, kwargs) = args.into_py_objects(&mut heap, &self.interns);
                Ok(ExecProgress::FunctionCall {
                    function_name: self.interns.get_external_function_name(function_id),
                    args,
                    kwargs,
                    state: FunctionCallExecutorState {
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

fn frame_exit_to_object(
    opt_frame_exit: Option<FrameExit>,
    heap: &mut Heap<impl ResourceTracker>,
    interns: &Interns,
) -> RunResult<PyObject> {
    match opt_frame_exit {
        Some(FrameExit::Return(return_value)) => Ok(PyObject::new(return_value, heap, interns)),
        Some(FrameExit::ExternalCall(_)) => {
            Err(ExcType::not_implemented("external function calls not supported by standard execution.").into())
        }
        None => Ok(PyObject::None),
    }
}

#[cfg(feature = "ref-counting")]
#[derive(Debug)]
pub struct RefCountOutput {
    pub py_object: PyObject,
    pub counts: ahash::AHashMap<String, usize>,
    pub unique_refs: usize,
    pub heap_count: usize,
}

/// Result of a single step of iterative execution.
///
/// This enum owns the execution state, ensuring type-safe state transitions.
/// - `FunctionCall` contains info about an external function call and state to resume
/// - `Complete` contains just the final value (execution is done)
///
/// # Type Parameters
/// * `T` - Resource tracker implementation (e.g., `NoLimitTracker` or `LimitedTracker`)
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ExecProgress<T: ResourceTracker> {
    /// Execution paused at an external function call. Call `state.run(return_value)` to resume.
    FunctionCall {
        /// The name of the function being called.
        function_name: String,
        /// The positional arguments passed to the function.
        args: Vec<PyObject>,
        /// The keyword arguments passed to the function (key, value pairs).
        kwargs: Vec<(PyObject, PyObject)>,
        /// The execution state that can be resumed with a return value.
        state: FunctionCallExecutorState<T>,
    },
    /// Execution completed with a final result.
    Complete(PyObject),
}

impl<T: ResourceTracker> ExecProgress<T> {
    /// Consumes the `ExecProgress` and returns external function call info and state.
    ///
    /// Returns (function_name, positional_args, keyword_args, state).
    #[allow(clippy::type_complexity)]
    pub fn into_function_call(
        self,
    ) -> Option<(
        String,
        Vec<PyObject>,
        Vec<(PyObject, PyObject)>,
        FunctionCallExecutorState<T>,
    )> {
        match self {
            ExecProgress::FunctionCall {
                function_name,
                args,
                kwargs,
                state,
            } => Some((function_name, args, kwargs, state)),
            ExecProgress::Complete(_) => None,
        }
    }

    /// Consumes the `ExecProgress` and returns the final value.
    pub fn into_complete(self) -> Option<PyObject> {
        match self {
            ExecProgress::Complete(value) => Some(value),
            ExecProgress::FunctionCall { .. } => None,
        }
    }
}

/// Execution state that can be resumed after an external function call.
///
/// This struct owns all runtime state and provides a `run()` method to continue
/// execution with the return value from the external function. When `run()` is
/// called, it consumes self and returns the next `ExecProgress`.
///
/// External function calls occur when calling a function that is not a builtin,
/// exception, or user-defined function.
///
/// # Type Parameters
/// * `T` - Resource tracker implementation
#[derive(Debug)]
pub struct FunctionCallExecutorState<T: ResourceTracker> {
    /// The underlying executor containing parsed AST and interns.
    executor: Executor,
    /// The heap for allocating runtime values.
    heap: Heap<T>,
    /// The namespace stack for variable storage.
    namespaces: Namespaces,
    /// Stack of execution positions for resuming inside nested control flow.
    position_stack: Vec<Position>,
}

impl<T: ResourceTracker> FunctionCallExecutorState<T> {
    /// Continues execution with the return value from the external function.
    ///
    /// Consumes self and returns the next execution progress.
    ///
    /// # Arguments
    /// * `return_value` - The value returned by the external function
    pub fn run(
        mut self,
        return_value: PyObject,
        writer: &mut impl PrintWriter,
    ) -> Result<ExecProgress<T>, PythonException> {
        // Clone data needed for error handling before consuming self.executor
        // This is necessary because run_from_position consumes the executor,
        // but we need interns and source to convert errors to PythonException
        let interns = self.executor.interns.clone();
        let source = self.executor.source.clone();

        // Convert PyObject to Value
        let value = return_value
            .to_value(&mut self.heap, &self.executor.interns)
            .map_err(|_| RunError::internal("invalid return value type").into_python_exception(&interns, &source))?;

        self.namespaces.push_return_value(value);

        // Continue execution from saved position
        // Note: run_from_position consumes self.executor, but may return it in ExecProgress::FunctionCall
        self.executor
            .run_from_position(self.heap, self.namespaces, self.position_stack.into(), writer)
            .map_err(|e| e.into_python_exception(&interns, &source))
    }
}

/// Iterative executor that supports pausing and resuming execution.
///
/// Unlike `Executor` which runs code to completion, `ExecutorIter` allows
/// execution to be paused at functions calls and resumed later. Call `run()`
/// to start execution - it consumes self and returns an `ExecProgress`:
/// - `ExecProgress::FunctionCall { ..., state }` - external function call, call `state.run(return_value)` to resume
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
/// use monty::{ExecutorIter, ExecProgress, NoLimitTracker, PyObject, StdPrint};
///
/// let exec = ExecutorIter::new("x + 1", "test.py", &["x"], vec![]).unwrap();
/// match exec.run_no_limits(vec![PyObject::Int(41)], &mut StdPrint).unwrap() {
///     ExecProgress::Complete(result) => assert_eq!(result, PyObject::Int(42)),
///     _ => panic!("unexpected function call"),
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
    /// Returns `PythonException` if the code cannot be parsed.
    pub fn new(
        code: &str,
        filename: &str,
        input_names: &[&str],
        external_functions: Vec<String>,
    ) -> Result<Self, PythonException> {
        let executor = Executor::new_internal(code, filename, input_names, external_functions)
            .map_err(|e| e.into_python_exc(filename, code))?;
        Ok(Self { executor })
    }

    /// Starts execution with the given inputs and no resource tracker, consuming self.
    ///
    /// Creates the heap and namespaces, then begins execution.
    ///
    /// # Arguments
    /// * `inputs` - Initial input values (must match length of `input_names` from `new()`)
    ///
    /// # Errors
    /// Returns `PythonException` if:
    /// - The number of inputs doesn't match the expected count
    /// - An input value is invalid (e.g., `PyObject::Repr`)
    /// - A runtime error occurs during execution
    pub fn run_no_limits(
        self,
        inputs: Vec<PyObject>,
        writer: &mut impl PrintWriter,
    ) -> Result<ExecProgress<NoLimitTracker>, PythonException> {
        self.run_with_tracker(inputs, NoLimitTracker::default(), writer)
    }

    /// Starts execution with the given inputs and resource limits, consuming self.
    ///
    /// Creates the heap and namespaces, then begins execution.
    ///
    /// # Arguments
    /// * `inputs` - Initial input values (must match length of `input_names` from `new()`)
    /// * `limits` - Resource limits for the execution
    ///
    /// # Errors
    /// Returns `PythonException` if:
    /// - The number of inputs doesn't match the expected count
    /// - An input value is invalid (e.g., `PyObject::Repr`)
    /// - A runtime error occurs during execution
    pub fn run_with_limits(
        self,
        inputs: Vec<PyObject>,
        limits: ResourceLimits,
        writer: &mut impl PrintWriter,
    ) -> Result<ExecProgress<LimitedTracker>, PythonException> {
        let resource_tracker = LimitedTracker::new(limits);
        self.run_with_tracker(inputs, resource_tracker, writer)
    }

    /// Starts execution with the given inputs and resource tracker, consuming self.
    ///
    /// Creates the heap and namespaces, then begins execution.
    ///
    /// # Arguments
    /// * `inputs` - Initial input values (must match length of `input_names` from `new()`)
    /// * `resource_tracker` - Resource tracker for the execution
    /// * `writer` - Writer for print output
    ///
    /// # Errors
    /// Returns `PythonException` if:
    /// - The number of inputs doesn't match the expected count
    /// - An input value is invalid (e.g., `PyObject::Repr`)
    /// - A runtime error occurs during execution
    pub fn run_with_tracker<T: ResourceTracker>(
        self,
        inputs: Vec<PyObject>,
        resource_tracker: T,
        writer: &mut impl PrintWriter,
    ) -> Result<ExecProgress<T>, PythonException> {
        // Clone data needed for error handling before consuming self.executor
        let interns = self.executor.interns.clone();
        let source = self.executor.source.clone();

        let mut heap = Heap::new(self.executor.namespace_size, resource_tracker);

        let namespaces = self
            .executor
            .prepare_namespaces(inputs, &mut heap)
            .map_err(|e| e.into_python_exception(&interns, &source))?;

        // Start execution from index 0 (beginning of code)
        let position_tracker = PositionTracker::default();
        self.executor
            .run_from_position(heap, namespaces, position_tracker, writer)
            .map_err(|e| e.into_python_exception(&interns, &source))
    }
}
