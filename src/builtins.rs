use std::fmt;
use std::str::FromStr;

use crate::exceptions::{exc_err_fmt, internal_err, ExcType, InternalRunError};
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
            _ => Err(()),
        }
    }
}

impl Builtins {
    /// Executes the builtin with the provided positional arguments.
    pub(crate) fn call<'c>(self, heap: &mut Heap, mut args: Vec<Object>) -> RunResult<'c, Object> {
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
                if args.len() != 1 {
                    return exc_err_fmt!(ExcType::TypeError; "len() takes exactly exactly one argument ({} given)", args.len());
                }
                let object = &args[0];
                match object.py_len(heap) {
                    Some(len) => Ok(Object::Int(len as i64)),
                    None => exc_err_fmt!(ExcType::TypeError; "Object of type {} has no len()", object.py_repr(heap)),
                }
            }
            Self::Str => {
                if args.len() != 1 {
                    return exc_err_fmt!(ExcType::TypeError; "str() takes exactly exactly one argument ({} given)", args.len());
                }
                let object = &args[0];
                let object_id = heap.allocate(HeapData::Str(object.py_str(heap).into_owned().into()));
                Ok(Object::Ref(object_id))
            }
            Self::Repr => {
                if args.len() != 1 {
                    return exc_err_fmt!(ExcType::TypeError; "repr() takes exactly exactly one argument ({} given)", args.len());
                }
                let object = &args[0];
                let object_id = heap.allocate(HeapData::Str(object.py_repr(heap).into_owned().into()));
                Ok(Object::Ref(object_id))
            }
            Self::Id => {
                if args.len() != 1 {
                    return exc_err_fmt!(ExcType::TypeError; "id() takes exactly exactly one argument ({} given)", args.len());
                }
                let object = &mut args[0];
                let id = object.id(heap);
                // TODO might need to use bigint here
                Ok(Object::Int(id as i64))
            }
            Self::Range => {
                if args.len() == 1 {
                    let object = &args[0];
                    let size = object.as_int()?;
                    Ok(Object::Range(size))
                } else {
                    internal_err!(InternalRunError::TodoError; "range() takes exactly one argument")
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
        }
    }
}

impl fmt::Display for Builtins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}
