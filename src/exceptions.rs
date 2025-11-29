use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use crate::expressions::ExprLoc;
use crate::heap::HeapData;
use crate::object::{Attr, Object};
use crate::parse::CodeRange;
use crate::run::RunResult;
use crate::values::str::string_repr;
use crate::values::PyValue;
use crate::Heap;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum ExcType {
    ValueError,
    TypeError,
    NameError,
    AttributeError,
    KeyError,
}

impl fmt::Display for ExcType {
    // TODO replace with a strum
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.str())
    }
}

impl ExcType {
    // TODO replace with a strum
    fn str(self) -> &'static str {
        match self {
            Self::ValueError => "ValueError",
            Self::TypeError => "TypeError",
            Self::NameError => "NameError",
            Self::AttributeError => "AttributeError",
            Self::KeyError => "KeyError",
        }
    }

    #[must_use]
    pub fn attribute_error<'c>(type_str: &str, attr: &Attr) -> RunError<'c> {
        exc_fmt!(Self::AttributeError; "'{type_str}' object has no attribute '{attr}'").into()
    }

    #[must_use]
    pub fn type_error_not_sub<'c>(type_str: &str) -> RunError<'c> {
        exc_fmt!(Self::TypeError; "'{type_str}' object is not subscriptable").into()
    }

    /// Creates a TypeError for unhashable types (e.g., list, dict used as dict keys).
    ///
    /// This matches Python's error message: `TypeError: unhashable type: 'list'`
    #[must_use]
    pub fn type_error_unhashable<'c>(type_str: &str) -> RunError<'c> {
        exc_fmt!(Self::TypeError; "unhashable type: '{type_str}'").into()
    }

    /// Creates a KeyError for a missing dict key.
    ///
    /// For string keys, uses the raw string value without extra quoting.
    /// For other types, uses repr.
    #[must_use]
    pub fn key_error<'c>(key: &Object, heap: &Heap) -> RunError<'c> {
        let key_str = match key {
            Object::Ref(id) => {
                if let HeapData::Str(s) = heap.get(*id) {
                    s.as_str().to_owned()
                } else {
                    key.py_repr(heap).into_owned()
                }
            }
            _ => key.py_repr(heap).into_owned(),
        };
        SimpleException::new(Self::KeyError, Some(key_str.into())).into()
    }

    /// Creates a TypeError for when a function receives the wrong number of arguments.
    ///
    /// Matches CPython's error format exactly:
    /// - For 1 expected arg: `{name}() takes exactly one argument ({actual} given)`
    /// - For N expected args: `{name} expected {expected} arguments, got {actual}`
    ///
    /// # Arguments
    /// * `name` - The function name (e.g., "len" for builtins, "list.append" for methods)
    /// * `expected` - Number of expected arguments
    /// * `actual` - Number of arguments actually provided
    #[must_use]
    pub fn type_error_arg_count<'c>(name: &str, expected: usize, actual: usize) -> RunError<'c> {
        if expected == 1 {
            // CPython: "len() takes exactly one argument (2 given)"
            exc_fmt!(Self::TypeError; "{}() takes exactly one argument ({} given)", name, actual).into()
        } else {
            // CPython: "insert expected 2 arguments, got 1"
            exc_fmt!(Self::TypeError; "{} expected {} arguments, got {}", name, expected, actual).into()
        }
    }

    /// Creates a TypeError for when a method that takes no arguments receives some.
    ///
    /// Matches CPython's format: `{name}() takes no arguments ({actual} given)`
    ///
    /// # Arguments
    /// * `name` - The method name (e.g., "dict.keys")
    /// * `actual` - Number of arguments actually provided
    #[must_use]
    pub fn type_error_no_args<'c>(name: &str, actual: usize) -> RunError<'c> {
        // CPython: "dict.keys() takes no arguments (1 given)"
        exc_fmt!(Self::TypeError; "{}() takes no arguments ({} given)", name, actual).into()
    }

    /// Creates a TypeError for when a function receives fewer arguments than required.
    ///
    /// Matches CPython's format: `{name} expected at least {min} argument, got {actual}`
    ///
    /// # Arguments
    /// * `name` - The function name (e.g., "get", "pop")
    /// * `min` - Minimum number of required arguments
    /// * `actual` - Number of arguments actually provided
    #[must_use]
    pub fn type_error_at_least<'c>(name: &str, min: usize, actual: usize) -> RunError<'c> {
        // CPython: "get expected at least 1 argument, got 0"
        exc_fmt!(Self::TypeError; "{} expected at least {} argument, got {}", name, min, actual).into()
    }

    /// Creates a TypeError for when a function receives more arguments than allowed.
    ///
    /// Matches CPython's format: `{name} expected at most {max} arguments, got {actual}`
    ///
    /// # Arguments
    /// * `name` - The function name (e.g., "get", "pop")
    /// * `max` - Maximum number of allowed arguments
    /// * `actual` - Number of arguments actually provided
    #[must_use]
    pub fn type_error_at_most<'c>(name: &str, max: usize, actual: usize) -> RunError<'c> {
        // CPython: "get expected at most 2 arguments, got 3"
        exc_fmt!(Self::TypeError; "{} expected at most {} arguments, got {}", name, max, actual).into()
    }
}

/// Validates argument count and returns args as a fixed-size array.
///
/// Uses const generics so the compiler knows the exact array size at compile time,
/// enabling safe pattern matching without runtime indexing or `.unwrap()`.
///
/// # Example
/// ```ignore
/// let [a, b] = check_arg_count::<2>("list.insert", args)?;
/// ```
pub fn check_arg_count<'c, const N: usize>(name: &str, args: Vec<Object>) -> Result<[Object; N], RunError<'c>> {
    let actual = args.len();
    if actual == N {
        // Safe: we just verified length equals N
        Ok(args
            .try_into()
            .unwrap_or_else(|_| unreachable!("length was just verified")))
    } else {
        Err(ExcType::type_error_arg_count(name, N, actual))
    }
}

/// Parses an exception type from its string representation.
///
/// Returns `Ok(ExcType)` if the name matches a known exception type,
/// or `Err(())` if the name is not recognized.
///
/// # Examples
/// - `"ValueError".parse::<ExcType>()` returns `Ok(ExcType::ValueError)`
/// - `"UnknownError".parse::<ExcType>()` returns `Err(())`
impl FromStr for ExcType {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "ValueError" => Ok(Self::ValueError),
            "TypeError" => Ok(Self::TypeError),
            "NameError" => Ok(Self::NameError),
            "AttributeError" => Ok(Self::AttributeError),
            "KeyError" => Ok(Self::KeyError),
            _ => Err(()),
        }
    }
}

/// Simple lightweight representation of an exception.
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleException {
    exc_type: ExcType,
    arg: Option<Cow<'static, str>>,
}

impl fmt::Display for SimpleException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.exc_type.str())?;

        if let Some(arg) = &self.arg {
            write!(f, "{}", string_repr(arg))?;
        }

        write!(f, ")")
    }
}

impl SimpleException {
    /// Creates a new exception with the given type and optional argument message.
    #[must_use]
    pub fn new(exc_type: ExcType, arg: Option<Cow<'static, str>>) -> Self {
        SimpleException { exc_type, arg }
    }

    pub(crate) fn type_str(&self) -> &'static str {
        self.exc_type.str()
    }

    /// Computes a hash for this exception based on its type and argument.
    ///
    /// Used when exceptions are used as dict keys (rare but supported).
    #[must_use]
    pub fn py_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.exc_type.hash(&mut hasher);
        self.arg.hash(&mut hasher);
        hasher.finish()
    }

    pub(crate) fn with_frame(self, frame: StackFrame) -> ExceptionRaise {
        ExceptionRaise {
            exc: self,
            frame: Some(frame),
        }
    }

    pub(crate) fn with_position(self, position: CodeRange) -> ExceptionRaise {
        ExceptionRaise {
            exc: self,
            frame: Some(StackFrame::from_position(position)),
        }
    }

    /// TODO move to `ExcType`
    pub(crate) fn operand_type_error<'c, 'd, T>(
        left: &'d ExprLoc<'c>,
        op: impl fmt::Display,
        right: &'d ExprLoc<'c>,
        left_object: Object,
        right_object: Object,
        heap: &Heap,
    ) -> RunResult<'c, T> {
        let left_type = left_object.py_type(heap);
        let right_type = right_object.py_type(heap);
        let new_position = left.position.extend(&right.position);
        Err(
            exc_fmt!(ExcType::TypeError; "unsupported operand type(s) for {op}: '{left_type}' and '{right_type}'")
                .with_position(new_position)
                .into(),
        )
    }
}

macro_rules! exc_static {
    ($error_type:expr; $msg:expr) => {
        crate::exceptions::SimpleException::new($error_type, Some($msg.into()))
    };
}
pub(crate) use exc_static;

macro_rules! exc_fmt {
    ($error_type:expr; $($fmt_args:tt)*) => {
        crate::exceptions::SimpleException::new($error_type, Some(format!($($fmt_args)*).into()))
    };
}
pub(crate) use exc_fmt;

macro_rules! exc_err_static {
    ($error_type:expr; $msg:expr) => {
        Err(crate::exceptions::exc_static!($error_type; $msg).into())
    };
}
pub(crate) use exc_err_static;

macro_rules! exc_err_fmt {
    ($error_type:expr; $($fmt_args:tt)*) => {
        Err(crate::exceptions::exc_fmt!($error_type; $($fmt_args)*).into())
    };
}
pub(crate) use exc_err_fmt;

#[derive(Debug, Clone)]
pub struct ExceptionRaise<'c> {
    pub exc: SimpleException,
    // first in vec is closes "bottom" frame
    pub(crate) frame: Option<StackFrame<'c>>,
}

impl fmt::Display for ExceptionRaise<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref frame) = self.frame {
            writeln!(f, "Traceback (most recent call last):")?;
            write!(f, "{frame}")?;
        }
        write!(f, "{}", self.exc)
    }
}

impl From<SimpleException> for ExceptionRaise<'_> {
    fn from(exc: SimpleException) -> Self {
        ExceptionRaise { exc, frame: None }
    }
}

impl ExceptionRaise<'_> {
    pub(crate) fn summary(&self) -> String {
        if let Some(ref frame) = self.frame {
            format!("({}) {}", frame.position, self.exc)
        } else {
            format!("(<no-tb>) {}", self.exc)
        }
    }
}

#[derive(Debug, Clone)]
pub struct StackFrame<'c> {
    pub(crate) position: CodeRange<'c>,
    pub(crate) frame_name: Option<&'c str>,
    pub(crate) parent: Option<Box<StackFrame<'c>>>,
}

impl fmt::Display for StackFrame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref parent) = self.parent {
            write!(f, "{parent}")?;
        }

        self.position.traceback(f, self.frame_name)
    }
}

impl<'c> StackFrame<'c> {
    pub(crate) fn new(position: &CodeRange<'c>, frame_name: &'c str, parent: Option<&StackFrame<'c>>) -> Self {
        Self {
            position: *position,
            frame_name: Some(frame_name),
            parent: parent.map(|parent| Box::new(parent.clone())),
        }
    }

    fn from_position(position: CodeRange<'c>) -> Self {
        Self {
            position,
            frame_name: None,
            parent: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum InternalRunError {
    Error(Cow<'static, str>),
    TodoError(Cow<'static, str>),
    // could be NameError, but we don't always have the name
    Undefined(Cow<'static, str>),
}

macro_rules! internal_error {
    ($error_type:expr; $msg:tt) => {
        $error_type(format!($msg).into())
    };
    ($error_type:expr; $msg:tt, $( $msg_args:expr ),+ ) => {
        $error_type(format!($msg, $( $msg_args ),+).into())
    };
}
pub(crate) use internal_error;

macro_rules! internal_err {
    ($error_type:expr; $msg:tt) => {
        Err(crate::exceptions::internal_error!($error_type; $msg).into())
    };
    ($error_type:expr; $msg:tt, $( $msg_args:expr ),+ ) => {
        Err(crate::exceptions::internal_error!($error_type; $msg, $( $msg_args ),+).into())
    };
}
pub(crate) use internal_err;

impl fmt::Display for InternalRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error(s) => write!(f, "Internal Error: {s}"),
            Self::TodoError(s) => write!(f, "Internal Error TODO: {s}"),
            Self::Undefined(s) => {
                if s.is_empty() {
                    write!(f, "Internal Error: accessing undefined object")
                } else {
                    write!(f, "Internal Error: accessing undefined object `{s}`")
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum RunError<'c> {
    Internal(InternalRunError),
    Exc(ExceptionRaise<'c>),
}

impl fmt::Display for RunError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(s) => write!(f, "{s}"),
            Self::Exc(s) => write!(f, "{s}"),
        }
    }
}

impl From<InternalRunError> for RunError<'_> {
    fn from(internal_error: InternalRunError) -> Self {
        Self::Internal(internal_error)
    }
}

impl<'c> From<ExceptionRaise<'c>> for RunError<'c> {
    fn from(exc: ExceptionRaise<'c>) -> Self {
        Self::Exc(exc)
    }
}

impl From<SimpleException> for RunError<'_> {
    fn from(exc: SimpleException) -> Self {
        Self::Exc(exc.into())
    }
}
