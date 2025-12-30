//! Implementation of the len() builtin function.

use crate::args::ArgValues;
use crate::exception::{exc_err_fmt, ExcType};
use crate::heap::Heap;
use crate::intern::Interns;
use crate::resource::ResourceTracker;
use crate::run_frame::RunResult;
use crate::types::PyTrait;
use crate::value::Value;

/// Implementation of the len() builtin function.
///
/// Returns the length of an object (number of items in a container).
pub fn builtin_len(heap: &mut Heap<impl ResourceTracker>, args: ArgValues, interns: &Interns) -> RunResult<Value> {
    let value = args.get_one_arg("len")?;
    let result = match value.py_len(heap, interns) {
        Some(len) => Ok(Value::Int(len as i64)),
        None => {
            exc_err_fmt!(ExcType::TypeError; "object of type {} has no len()", value.py_repr(heap, interns))
        }
    };
    value.drop_with_heap(heap);
    result
}
