use crate::object::Object;
use crate::parse::ParseResult;
use crate::prepare::PrepareResult;

use rustpython_parser::ast::{Expr as AstExpr, ExprKind, TextRange};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Operator {
    Add,
    Sub,
    Mult,
    MatMult,
    Div,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    FloorDiv,
    // bool operators
    And,
    Or,
}

/// Defined separately since these operators always return a bool
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CmpOperator {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

#[derive(Debug, Clone)]
pub(crate) struct Position {
    start: u32,
    end: u32,
}

impl Position {
    pub fn from_range(range: &TextRange) -> Self {
        Self {
            start: range.start().into(),
            end: range.end().into(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExprLoc {
    pub position: Position,
    pub expr: Expr,
}

impl ExprLoc {
    pub fn from_expr(range: &TextRange, expr: Expr) -> Self {
        Self {
            position: Position::from_range(range),
            expr,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Identifier {
    pub name: String,
    pub id: usize,
}

impl Identifier {
    pub fn from_ast(ast: AstExpr) -> ParseResult<Self> {
        match ast.node {
            ExprKind::Name { id, .. } => Ok(Self::from_name(id)),
            _ => Err(format!("Expected name, got {:?}", ast.node).into()),
        }
    }

    pub fn from_name(name: String) -> Self {
        Self { name, id: 0 }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Kwarg {
    pub key: Identifier,
    pub value: ExprLoc,
}

#[derive(Debug, Clone)]
pub(crate) enum Function {
    Builtin(Builtins),
    Ident(Identifier),
}

impl Function {
    /// whether the function has side effects
    pub fn side_effects(&self) -> bool {
        match self {
            Self::Builtin(b) => b.side_effects(),
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Expr {
    Constant(Object),
    Name(Identifier),
    Call {
        func: Function,
        args: Vec<ExprLoc>,
        kwargs: Vec<Kwarg>,
    },
    Op {
        left: Box<ExprLoc>,
        op: Operator,
        right: Box<ExprLoc>,
    },
    CmpOp {
        left: Box<ExprLoc>,
        op: CmpOperator,
        right: Box<ExprLoc>,
    },
    #[allow(dead_code)]
    List(Vec<ExprLoc>),
}

impl Expr {
    pub fn is_const(&self) -> bool {
        matches!(self, Self::Constant(_))
    }

    pub fn into_object(self) -> Object {
        match self {
            Self::Constant(object) => object,
            _ => panic!("into_const can only be called on Constant expression."),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Node {
    Pass,
    Expr(ExprLoc),
    Assign {
        target: Identifier,
        object: ExprLoc,
    },
    OpAssign {
        target: Identifier,
        op: Operator,
        object: ExprLoc,
    },
    For {
        target: ExprLoc,
        iter: ExprLoc,
        body: Vec<Node>,
        or_else: Vec<Node>,
    },
    If {
        test: ExprLoc,
        body: Vec<Node>,
        or_else: Vec<Node>,
    },
}

// this is a temporary hack
#[derive(Debug, Clone)]
pub(crate) enum Builtins {
    Print,
    Range,
    Len,
}

impl Builtins {
    pub fn find(name: &str) -> PrepareResult<Self> {
        match name {
            "print" => Ok(Self::Print),
            "range" => Ok(Self::Range),
            "len" => Ok(Self::Len),
            _ => Err(format!("unknown builtin: {name}").into()),
        }
    }

    /// whether the function has side effects
    pub fn side_effects(&self) -> bool {
        match self {
            Self::Print => true,
            _ => false,
        }
    }
}
