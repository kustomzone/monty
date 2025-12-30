//! Implementation of the abs() builtin function.

use crate::args::ArgValues;
use crate::exception::{exc_err_fmt, ExcType};
use crate::heap::Heap;
use crate::resource::ResourceTracker;
use crate::run_frame::RunResult;
use crate::types::PyTrait;
use crate::value::Value;

/// Implementation of the abs() builtin function.
///
/// Returns the absolute value of a number. Works with integers and floats.
pub fn builtin_abs(heap: &mut Heap<impl ResourceTracker>, args: ArgValues) -> RunResult<Value> {
    let value = args.get_one_arg("abs")?;

    let result = match &value {
        Value::Int(n) => {
            // Handle potential overflow for i64::MIN
            Ok(Value::Int(n.checked_abs().unwrap_or(i64::MIN)))
        }
        Value::Float(f) => Ok(Value::Float(f.abs())),
        Value::Bool(b) => Ok(Value::Int(i64::from(*b))),
        _ => {
            exc_err_fmt!(ExcType::TypeError; "bad operand type for abs(): '{}'", value.py_type(Some(heap)))
        }
    };

    value.drop_with_heap(heap);
    result
}
