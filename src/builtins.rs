use std::fmt;
use std::str::FromStr;

use crate::exceptions::{check_arg_count, exc_err_fmt, internal_err, ExcType, InternalRunError};
use crate::heap::{Heap, HeapData};
use crate::object::Object;
use crate::run::RunResult;
use crate::values::PyValue;

/// Builtins enumerates every interpreter-native Python builtin Monty currently supports.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Builtins {
    Print,
    Len,
    Str,
    Repr,
    Id,
    Range,
    Hash,
}

/// Parses a builtin function from its string representation.
///
/// Returns `Ok(Builtins)` if the name matches a known builtin function,
/// or `Err(())` if the name is not recognized.
///
/// # Examples
/// - `"print".parse::<Builtins>()` returns `Ok(Builtins::Print)`
/// - `"unknown".parse::<Builtins>()` returns `Err(())`
impl FromStr for Builtins {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "print" => Ok(Self::Print),
            "len" => Ok(Self::Len),
            "str" => Ok(Self::Str),
            "repr" => Ok(Self::Repr),
            "id" => Ok(Self::Id),
            "range" => Ok(Self::Range),
            "hash" => Ok(Self::Hash),
            _ => Err(()),
        }
    }
}

impl Builtins {
    /// Executes the builtin with the provided positional arguments.
    pub(crate) fn call<'c>(self, heap: &mut Heap, args: Vec<Object>) -> RunResult<'c, Object> {
        match self {
            Self::Print => {
                for (i, object) in args.iter().enumerate() {
                    if i == 0 {
                        print!("{}", object.py_str(heap));
                    } else {
                        print!(" {}", object.py_str(heap));
                    }
                }
                println!();
                Ok(Object::None)
            }
            Self::Len => {
                let [object] = check_arg_count::<1>("len", args)?;
                match object.py_len(heap) {
                    Some(len) => Ok(Object::Int(len as i64)),
                    None => exc_err_fmt!(ExcType::TypeError; "Object of type {} has no len()", object.py_repr(heap)),
                }
            }
            Self::Str => {
                let [object] = check_arg_count::<1>("str", args)?;
                let object_id = heap.allocate(HeapData::Str(object.py_str(heap).into_owned().into()));
                Ok(Object::Ref(object_id))
            }
            Self::Repr => {
                let [object] = check_arg_count::<1>("repr", args)?;
                let object_id = heap.allocate(HeapData::Str(object.py_repr(heap).into_owned().into()));
                Ok(Object::Ref(object_id))
            }
            Self::Id => {
                let [mut object] = check_arg_count::<1>("id", args)?;
                let id = object.id(heap);
                // TODO might need to use bigint here
                Ok(Object::Int(id as i64))
            }
            Self::Range => {
                if args.len() == 1 {
                    let [object] = check_arg_count::<1>("range", args)?;
                    let size = object.as_int()?;
                    Ok(Object::Range(size))
                } else {
                    internal_err!(InternalRunError::TodoError; "range() takes exactly one argument")
                }
            }
            Self::Hash => {
                let [object] = check_arg_count::<1>("hash", args)?;
                match object.py_hash_u64(heap) {
                    Some(hash) => Ok(Object::Int(hash as i64)),
                    None => Err(ExcType::type_error_unhashable(object.py_type(heap))),
                }
            }
        }
    }

    /// Returns the canonical Python spelling of the builtin.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Len => "len",
            Self::Str => "str",
            Self::Repr => "repr",
            Self::Id => "id",
            Self::Range => "range",
            Self::Hash => "hash",
        }
    }
}

impl fmt::Display for Builtins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}
