//! Implementation of the filter() builtin function.
//!
//! This module provides the filter() builtin which filters elements from an iterable
//! based on a predicate function. The implementation supports:
//! - `None` as predicate (filters falsy values)
//! - Builtin functions (len, abs, etc.)
//! - Type constructors (int, str, float, etc.)
//! - User-defined functions (requires VM-level support via `do_filter`)

use crate::{
    args::ArgValues,
    builtins::Builtins,
    exception_private::{ExcType, RunError, RunResult},
    heap::{Heap, HeapData},
    intern::Interns,
    io::PrintWriter,
    resource::ResourceTracker,
    types::{List, MontyIter, PyTrait},
    value::Value,
};

/// Implementation of the filter() builtin function.
///
/// Filters elements from an iterable based on a predicate function.
/// If the predicate is None, filters out falsy values.
///
/// Note: In Python this returns an iterator, but we return a list for simplicity.
///
/// For user-defined functions, this is called via `do_filter` from the VM level
/// which provides access to the VM's function calling machinery.
///
/// Examples:
/// ```python
/// filter(lambda x: x > 0, [-1, 0, 1, 2])  # [1, 2]
/// filter(None, [0, 1, False, True, ''])   # [1, True]
/// ```
pub fn builtin_filter(
    heap: &mut Heap<impl ResourceTracker>,
    args: ArgValues,
    interns: &Interns,
    print_writer: &mut impl PrintWriter,
) -> RunResult<Value> {
    let (function, iterable) = args.get_two_args("filter", heap)?;
    do_filter(function, iterable, heap, interns, print_writer)
}

/// Performs the filter operation with full callable support.
///
/// This is the main implementation of filter() that handles all callable types.
/// It's separated from `builtin_filter` to allow direct calls from the VM level
/// when user-defined function support is needed.
///
/// # Arguments
/// * `function` - The predicate function (None, builtin, type constructor, or user-defined)
/// * `iterable` - The iterable to filter
/// * `heap` - The heap for memory management
/// * `interns` - Interned strings for comparisons
/// * `print_writer` - Output writer (needed for builtin function calls)
pub fn do_filter(
    function: Value,
    iterable: Value,
    heap: &mut Heap<impl ResourceTracker>,
    interns: &Interns,
    print_writer: &mut impl PrintWriter,
) -> RunResult<Value> {
    let mut iter = match MontyIter::new(iterable, heap, interns) {
        Ok(it) => it,
        Err(e) => {
            function.drop_with_heap(heap);
            return Err(e);
        }
    };

    // Check if function is a user-defined function that we can't call from here
    if needs_vm_support(&function, heap) {
        function.drop_with_heap(heap);
        iter.drop_with_heap(heap);
        return Err(ExcType::type_error(
            "filter() predicate must be None or a builtin function (user-defined functions not yet supported)",
        ));
    }

    let mut out: Vec<Value> = Vec::new();

    loop {
        let item = match iter.for_next(heap, interns) {
            Ok(Some(item)) => item,
            Ok(None) => break, // Iterator exhausted
            Err(e) => {
                // Clean up on iterator error
                function.drop_with_heap(heap);
                iter.drop_with_heap(heap);
                for v in out {
                    v.drop_with_heap(heap);
                }
                return Err(e);
            }
        };

        // Clone for predicate call - the clone is consumed by call_predicate_function
        let item_for_predicate = item.clone_with_heap(heap);

        let should_include = match call_predicate_function(&function, item_for_predicate, heap, interns, print_writer) {
            Ok(result) => result,
            Err(e) => {
                // Clean up on predicate error
                // Note: item_for_predicate is already dropped inside call_predicate_function
                item.drop_with_heap(heap);
                function.drop_with_heap(heap);
                iter.drop_with_heap(heap);
                for v in out {
                    v.drop_with_heap(heap);
                }
                return Err(e);
            }
        };

        if should_include {
            out.push(item);
        } else {
            item.drop_with_heap(heap);
        }
    }

    function.drop_with_heap(heap);
    iter.drop_with_heap(heap);

    let heap_id = heap.allocate(HeapData::List(List::new(out)))?;
    Ok(Value::Ref(heap_id))
}

/// Checks if a function value requires VM-level support to call.
///
/// Returns true for user-defined functions, closures, and external functions
/// that need VM frame management for proper execution.
fn needs_vm_support(function: &Value, heap: &Heap<impl ResourceTracker>) -> bool {
    match function {
        Value::None | Value::Builtin(_) => false,
        Value::DefFunction(_) | Value::ExtFunction(_) => true,
        Value::Ref(heap_id) => {
            // Check if this is a closure or function with defaults
            matches!(
                heap.get(*heap_id),
                HeapData::Closure(_, _, _) | HeapData::FunctionDefaults(_, _)
            )
        }
        _ => false, // Other values will be caught as "not callable" in call_predicate_function
    }
}

/// Calls a predicate function on a single element and returns whether the result is truthy.
///
/// Handles different callable types:
/// - `None` - returns the truthiness of the element itself
/// - Builtin functions - calls directly via `builtin.call()`
/// - Type constructors - calls via `type.call()`
/// - Other values - returns an error
///
/// This is similar to `call_key_function` in list.rs but returns a bool for the truthiness
/// of the result rather than the result value itself.
fn call_predicate_function(
    predicate: &Value,
    elem: Value,
    heap: &mut Heap<impl ResourceTracker>,
    interns: &Interns,
    print_writer: &mut impl PrintWriter,
) -> Result<bool, RunError> {
    match predicate {
        Value::None => {
            // No predicate - use truthiness of element
            let is_truthy = elem.py_bool(heap, interns);
            elem.drop_with_heap(heap);
            Ok(is_truthy)
        }
        Value::Builtin(Builtins::Function(builtin)) => {
            let args = ArgValues::One(elem);
            let result = builtin.call(heap, args, interns, print_writer)?;
            let is_truthy = result.py_bool(heap, interns);
            result.drop_with_heap(heap);
            Ok(is_truthy)
        }
        Value::Builtin(Builtins::Type(t)) => {
            // Type constructors (int, str, float, etc.) are callable predicates
            let args = ArgValues::One(elem);
            let result = t.call(heap, args, interns)?;
            let is_truthy = result.py_bool(heap, interns);
            result.drop_with_heap(heap);
            Ok(is_truthy)
        }
        Value::Builtin(Builtins::ExcType(_)) => {
            // Exception types are technically callable but not useful as predicates
            elem.drop_with_heap(heap);
            Err(ExcType::type_error("filter() predicate cannot be an exception type"))
        }
        _ => {
            // This shouldn't be reached if needs_vm_support is called first
            let type_name = predicate.py_type(heap);
            elem.drop_with_heap(heap);
            Err(ExcType::type_error(format!("'{type_name}' object is not callable")))
        }
    }
}
