use std::fmt;
use std::str::FromStr;

use crate::builtins::Builtins;
use crate::exceptions::{ExcType, ExceptionRaise};
use crate::heap::{Heap, HeapData};
use crate::object::{Attr, Object};
use crate::operators::{CmpOperator, Operator};
use crate::parse::CodeRange;

#[derive(Debug, Clone)]
pub(crate) struct Identifier<'c> {
    pub position: CodeRange<'c>,
    pub name: String, // TODO could this a `&'c str` or cow?
    pub id: usize,
}

impl<'c> Identifier<'c> {
    pub fn new(name: String, position: CodeRange<'c>) -> Self {
        Self { name, position, id: 0 }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Kwarg<'c> {
    pub key: Identifier<'c>,
    pub value: ExprLoc<'c>,
}

/// Represents a callable entity in the Python runtime.
///
/// A callable can be a builtin function, an exception type (which acts as a constructor),
/// or an identifier that will be resolved during preparation.
#[derive(Debug, Clone)]
pub(crate) enum Callable<'c> {
    Builtin(Builtins),
    Exception(ExcType),
    // TODO can we remove Ident here and thereby simplify Callable?
    Ident(Identifier<'c>),
}

impl fmt::Display for Callable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(b) => write!(f, "{b}"),
            Self::Exception(exc) => write!(f, "{exc}"),
            Self::Ident(i) => write!(f, "{}", i.name),
        }
    }
}

/// Parses a callable from its string representation.
///
/// Attempts to resolve the name as a builtin function first, then as an exception type.
/// Returns an error if the name doesn't match any known builtin or exception.
///
/// This is used during the preparation phase to resolve identifier names into their
/// corresponding builtin or exception type callables.
///
/// # Examples
/// - `"print".parse::<Callable>()` returns `Ok(Callable::Builtin(Builtins::Print))`
/// - `"ValueError".parse::<Callable>()` returns `Ok(Callable::Exception(ExcType::ValueError))`
/// - `"unknown".parse::<Callable>()` returns `Err(())`
impl FromStr for Callable<'static> {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        if let Ok(builtin) = name.parse::<Builtins>() {
            Ok(Self::Builtin(builtin))
        } else if let Ok(exc_type) = name.parse::<ExcType>() {
            Ok(Self::Exception(exc_type))
        } else {
            Err(())
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Expr<'c> {
    Constant(Const),
    Name(Identifier<'c>),
    Call {
        callable: Callable<'c>,
        args: Vec<ExprLoc<'c>>,
        kwargs: Vec<Kwarg<'c>>,
    },
    AttrCall {
        object: Identifier<'c>,
        attr: Attr,
        args: Vec<ExprLoc<'c>>,
        kwargs: Vec<Kwarg<'c>>,
    },
    Op {
        left: Box<ExprLoc<'c>>,
        op: Operator,
        right: Box<ExprLoc<'c>>,
    },
    CmpOp {
        left: Box<ExprLoc<'c>>,
        op: CmpOperator,
        right: Box<ExprLoc<'c>>,
    },
    List(Vec<ExprLoc<'c>>),
    Tuple(Vec<ExprLoc<'c>>),
}

impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constant(object) => write!(f, "{}", object.repr()),
            Self::Name(identifier) => write!(f, "{}", identifier.name),
            Self::Call { callable, args, kwargs } => self.print_args(f, callable, args, kwargs),
            Self::AttrCall {
                object,
                attr,
                args,
                kwargs,
            } => {
                write!(f, "{}.", object.name)?;
                self.print_args(f, attr, args, kwargs)
            }
            Self::Op { left, op, right } => write!(f, "{left} {op} {right}"),
            Self::CmpOp { left, op, right } => write!(f, "{left} {op} {right}"),
            Self::List(itms) => {
                write!(
                    f,
                    "[{}]",
                    itms.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
                )
            }
            Self::Tuple(itms) => {
                write!(
                    f,
                    "({})",
                    itms.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
                )
            }
        }
    }
}

impl<'c> Expr<'c> {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::Constant(Const::None))
    }

    fn print_args(
        &self,
        f: &mut fmt::Formatter<'_>,
        func: impl fmt::Display,
        args: &[ExprLoc<'c>],
        kwargs: &[Kwarg<'c>],
    ) -> fmt::Result {
        write!(f, "{func}(")?;
        let mut pos_args = false;
        for (index, arg) in args.iter().enumerate() {
            if index == 0 {
                write!(f, "{arg}")?;
            } else {
                write!(f, ", {arg}")?;
            }
            pos_args = true;
        }
        if pos_args {
            for kwarg in kwargs {
                write!(f, ", {}={}", kwarg.key.name, kwarg.value)?;
            }
        } else {
            for (index, kwarg) in kwargs.iter().enumerate() {
                if index == 0 {
                    write!(f, "{}={}", kwarg.key.name, kwarg.value)?;
                } else {
                    write!(f, ", {}={}", kwarg.key.name, kwarg.value)?;
                }
            }
        }
        write!(f, ")")
    }
}

/// Represents values that can be produced purely from the parser/prepare pipeline.
///
/// Const values are intentionally detached from the runtime heap so we can keep
/// parse-time transformations (constant folding, namespace seeding, etc.) free from
/// reference-count semantics. Only once execution begins are these literals turned
/// into real `Object`s that participate in the interpreter's runtime rules.
///
/// Note: unlike the AST `Constant` type, we store tuples only as expressions since they
/// can't always be recorded as constants.
#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Undefined,
    Ellipsis,
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
}

impl Const {
    /// Converts the literal into its runtime `Object` counterpart.
    ///
    /// This is the only place parse-time data crosses the boundary into runtime
    /// semantics, ensuring every literal follows the same conversion path (helpful
    /// for keeping later heap/refcount logic centralized).
    ///
    /// Heap-allocated types (Str, Bytes, Tuple) will be allocated on the heap and
    /// returned as `Object::Ref` variants. Immediate values are returned inline.
    pub fn to_object(&self, heap: &mut Heap) -> Object {
        match self {
            Self::Undefined => Object::Undefined,
            Self::Ellipsis => Object::Ellipsis,
            Self::None => Object::None,
            Self::Bool(b) => Object::Bool(*b),
            Self::Int(v) => Object::Int(*v),
            Self::Float(v) => Object::Float(*v),
            Self::Str(s) => Object::Ref(heap.allocate(HeapData::Str(s.clone().into()))),
            Self::Bytes(b) => Object::Ref(heap.allocate(HeapData::Bytes(b.clone().into()))),
        }
    }

    /// Returns a Python-esque string representation for logging/debugging.
    ///
    /// This avoids the need to import runtime formatting helpers into parser code
    /// while still giving enough fidelity to display constants in errors/traces.
    pub fn repr(&self) -> String {
        match self {
            Self::Undefined => "Undefined".to_string(),
            Self::Ellipsis => "...".to_string(),
            Self::None => "None".to_string(),
            Self::Bool(true) => "True".to_string(),
            Self::Bool(false) => "False".to_string(),
            Self::Int(v) => v.to_string(),
            Self::Float(v) => v.to_string(),
            Self::Str(v) => format!("'{v}'"),
            Self::Bytes(v) => format!("b'{v:?}'"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExprLoc<'c> {
    pub position: CodeRange<'c>,
    pub expr: Expr<'c>,
}

impl fmt::Display for ExprLoc<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // don't show position as that should be displayed separately
        write!(f, "{}", self.expr)
    }
}

impl<'c> ExprLoc<'c> {
    pub fn new(position: CodeRange<'c>, expr: Expr<'c>) -> Self {
        Self { position, expr }
    }
}

// TODO need a new AssignTo (enum of identifier, tuple) type used for "Assign" and "For"

#[derive(Debug, Clone)]
pub(crate) enum Node<'c> {
    Pass,
    Expr(ExprLoc<'c>),
    Return(ExprLoc<'c>),
    ReturnNone,
    Raise(Option<ExprLoc<'c>>),
    Assign {
        target: Identifier<'c>,
        object: ExprLoc<'c>,
    },
    OpAssign {
        target: Identifier<'c>,
        op: Operator,
        object: ExprLoc<'c>,
    },
    For {
        target: Identifier<'c>,
        iter: ExprLoc<'c>,
        body: Vec<Node<'c>>,
        or_else: Vec<Node<'c>>,
    },
    If {
        test: ExprLoc<'c>,
        body: Vec<Node<'c>>,
        or_else: Vec<Node<'c>>,
    },
}

#[derive(Debug)]
pub enum FrameExit<'c> {
    Return(Object),
    // Yield(Object),
    #[allow(dead_code)]
    Raise(ExceptionRaise<'c>),
}
