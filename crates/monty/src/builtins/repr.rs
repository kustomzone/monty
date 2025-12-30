//! Implementation of the repr() builtin function.

use crate::args::ArgValues;
use crate::heap::{Heap, HeapData};
use crate::intern::Interns;
use crate::resource::ResourceTracker;
use crate::run_frame::RunResult;
use crate::types::PyTrait;
use crate::value::Value;

/// Implementation of the repr() builtin function.
///
/// Returns a string containing a printable representation of an object.
pub fn builtin_repr(heap: &mut Heap<impl ResourceTracker>, args: ArgValues, interns: &Interns) -> RunResult<Value> {
    let value = args.get_one_arg("repr")?;
    let heap_id = heap.allocate(HeapData::Str(value.py_repr(heap, interns).into_owned().into()))?;
    value.drop_with_heap(heap);
    Ok(Value::Ref(heap_id))
}
