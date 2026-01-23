//! Implementation of the hasattr() builtin function.

use crate::{
    ExcType,
    args::ArgValues,
    exception_private::{RunResult, SimpleException},
    heap::Heap,
    intern::Interns,
    resource::ResourceTracker,
    value::Value,
};

/// Implementation of the hasattr() builtin function.
///
/// Returns True if the object has the named attribute, False otherwise.
/// This function always succeeds and never raises AttributeError.
///
/// Signature: `hasattr(object, name)`
///
/// Note: This is implemented by calling getattr(object, name) and returning
/// True if it succeeds, False if it raises an exception.
///
/// Examples:
/// ```python
/// hasattr(obj, 'x')             # Check if obj.x exists
/// hasattr(slice(1, 10), 'start') # True - slice has start attribute
/// hasattr(42, 'nonexistent')    # False - int has no such attribute
/// ```
pub fn builtin_hasattr(heap: &mut Heap<impl ResourceTracker>, args: ArgValues, interns: &Interns) -> RunResult<Value> {
    let (mut positional, kwargs) = args.into_parts();

    let pos_count = positional.len();
    let kw_count = kwargs.len();

    if !kwargs.is_empty() {
        for (k, v) in kwargs {
            k.drop_with_heap(heap);
            v.drop_with_heap(heap);
        }
        for v in positional {
            v.drop_with_heap(heap);
        }
        return Err(ExcType::type_error_arg_count("hasattr", 2, pos_count + kw_count));
    }

    if pos_count != 2 {
        for v in positional {
            v.drop_with_heap(heap);
        }
        return Err(ExcType::type_error_arg_count("hasattr", 2, pos_count));
    }

    let object = positional.next().unwrap();
    let name = positional.next().unwrap();

    let Value::InternString(name_id) = name else {
        object.drop_with_heap(heap);
        name.drop_with_heap(heap);
        return Err(SimpleException::new_msg(ExcType::TypeError, "hasattr(): attribute name must be string").into());
    };

    name.drop_with_heap(heap);

    // important: we must own the returned value if py_get_attr succeeds to drop it
    let has_attr = match object.py_get_attr(name_id, heap, interns) {
        Ok(value) => {
            value.drop_with_heap(heap);
            true
        }
        Err(_) => false,
    };

    object.drop_with_heap(heap);

    Ok(Value::Bool(has_attr))
}
