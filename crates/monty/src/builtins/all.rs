//! Implementation of the all() builtin function.

use crate::args::ArgValues;
use crate::for_iterator::ForIterator;
use crate::heap::Heap;
use crate::intern::Interns;
use crate::resource::ResourceTracker;
use crate::run_frame::RunResult;
use crate::types::PyTrait;
use crate::value::Value;

/// Implementation of the all() builtin function.
///
/// Returns True if all elements of the iterable are true (or if the iterable is empty).
/// Short-circuits on the first falsy value.
pub fn builtin_all(heap: &mut Heap<impl ResourceTracker>, args: ArgValues, interns: &Interns) -> RunResult<Value> {
    let iterable = args.get_one_arg("all")?;
    let mut iter = ForIterator::new(iterable, heap, interns)?;

    while let Some(item) = iter.for_next(heap, interns)? {
        let is_truthy = item.py_bool(heap, interns);
        item.drop_with_heap(heap);
        if !is_truthy {
            iter.drop_with_heap(heap);
            return Ok(Value::Bool(false));
        }
    }

    iter.drop_with_heap(heap);
    Ok(Value::Bool(true))
}
