//! Implementation of the filter() builtin function.

use crate::{
    args::ArgValues,
    builtins::Builtins,
    exception_private::{ExcType, RunResult, SimpleException},
    heap::{Heap, HeapData},
    intern::Interns,
    io::NoPrint,
    resource::ResourceTracker,
    types::{List, MontyIter, PyTrait},
    value::Value,
};

/// Mode for filtering: either truthiness check (None) or calling a builtin function.
#[derive(Debug, Clone, Copy)]
enum FilterMode {
    Truthiness,
    BuiltinFunction(Builtins),
}

/// Implementation of the filter() builtin function.
///
/// Filters elements from an iterable based on a predicate function.
/// If the predicate is None, filters out falsy values.
///
/// Note: In Python this returns an iterator, but we return a list for simplicity.
///
/// Examples:
/// ```python
/// filter(lambda x: x > 0, [-1, 0, 1, 2])  # [1, 2]
/// filter(None, [0, 1, False, True, ''])   # [1, True]
/// ```
pub fn builtin_filter(heap: &mut Heap<impl ResourceTracker>, args: ArgValues, interns: &Interns) -> RunResult<Value> {
    let (function, iterable) = args.get_two_args("filter", heap)?;

    let mut iter = match MontyIter::new(iterable, heap, interns) {
        Ok(it) => it,
        Err(e) => {
            function.drop_with_heap(heap);
            return Err(e);
        }
    };

    let filter_mode = match function {
        Value::None => FilterMode::Truthiness,
        Value::Builtin(builtin) => FilterMode::BuiltinFunction(builtin),
        not_supported => {
            let func_type = not_supported.py_type(heap);
            not_supported.drop_with_heap(heap);
            iter.drop_with_heap(heap);

            return Err(
                SimpleException::new_msg(ExcType::TypeError, format!("'{func_type}' object is not callable")).into(),
            );
        }
    };

    function.drop_with_heap(heap);

    let mut out = Vec::new();

    while let Some(item) = iter.for_next(heap, interns)? {
        let should_include = match filter_mode {
            FilterMode::Truthiness => item.py_bool(heap, interns),
            FilterMode::BuiltinFunction(builtin) => {
                let args = ArgValues::One(item.clone_with_heap(heap));
                let result_value = builtin.call(heap, args, interns, &mut NoPrint)?;
                let is_truthy = result_value.py_bool(heap, interns);
                result_value.drop_with_heap(heap);

                is_truthy
            }
        };

        if should_include {
            out.push(item);
        } else {
            item.drop_with_heap(heap);
        }
    }

    iter.drop_with_heap(heap);

    let heap_id = heap.allocate(HeapData::List(List::new(out)))?;

    Ok(Value::Ref(heap_id))
}
