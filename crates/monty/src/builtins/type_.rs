//! Implementation of the type() builtin function.

use crate::args::ArgValues;
use crate::heap::Heap;
use crate::resource::ResourceTracker;
use crate::run_frame::RunResult;
use crate::types::PyTrait;
use crate::value::Value;

use super::Builtins;

/// Implementation of the type() builtin function.
///
/// Returns the type of an object.
pub fn builtin_type(heap: &mut Heap<impl ResourceTracker>, args: ArgValues) -> RunResult<Value> {
    let value = args.get_one_arg("type")?;
    let type_obj = value.py_type(Some(heap));
    value.drop_with_heap(heap);
    Ok(Value::Builtin(Builtins::Type(type_obj)))
}
