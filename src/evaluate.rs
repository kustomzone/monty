use crate::exceptions::{internal_err, ExcType, InternalRunError, SimpleException};
use crate::expressions::{Callable, Expr, ExprLoc, Identifier, Kwarg};
use crate::heap::Heap;
use crate::object::{Attr, Object};
use crate::operators::{CmpOperator, Operator};
use crate::run::RunResult;
use crate::values::{List, PyValue};
use crate::HeapData;

/// Evaluates an expression node and returns a value.
///
/// `namespace` provides the current frame bindings, while `heap` is threaded so any
/// future heap-backed objects can be created/cloned without re-threading plumbing later.
pub(crate) fn evaluate_use<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    expr_loc: &'d ExprLoc<'c>,
) -> RunResult<'c, Object> {
    match &expr_loc.expr {
        Expr::Constant(literal) => Ok(literal.to_object(heap)),
        Expr::Name(ident) => {
            if let Some(object) = namespace.get(ident.id) {
                match object {
                    Object::Undefined => Err(InternalRunError::Undefined(ident.name.clone().into()).into()),
                    _ => Ok(object.clone_with_heap(heap)),
                }
            } else {
                let name = ident.name.clone();

                Err(SimpleException::new(ExcType::NameError, Some(name.into()))
                    .with_position(expr_loc.position)
                    .into())
            }
        }
        Expr::Call { callable, args, kwargs } => Ok(call_function(namespace, heap, callable, args, kwargs)?),
        Expr::AttrCall {
            object,
            attr,
            args,
            kwargs,
        } => Ok(attr_call(namespace, heap, expr_loc, object, attr, args, kwargs)?),
        Expr::Op { left, op, right } => eval_op(namespace, heap, left, op, right),
        Expr::CmpOp { left, op, right } => Ok(cmp_op(namespace, heap, left, op, right)?.into()),
        Expr::List(elements) => {
            let objects = elements
                .iter()
                .map(|e| evaluate_use(namespace, heap, e))
                .collect::<RunResult<_>>()?;
            let object_id = heap.allocate(HeapData::List(List::from_vec(objects)));
            Ok(Object::Ref(object_id))
        }
        Expr::Tuple(elements) => {
            let objects = elements
                .iter()
                .map(|e| evaluate_use(namespace, heap, e))
                .collect::<RunResult<_>>()?;
            let object_id = heap.allocate(HeapData::Tuple(objects));
            Ok(Object::Ref(object_id))
        }
    }
}

/// Evaluates an expression node and discard the returned value
///
/// `namespace` provides the current frame bindings, while `heap` is threaded so any
/// future heap-backed objects can be created/cloned without re-threading plumbing later.
pub(crate) fn evaluate_discard<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    expr_loc: &'d ExprLoc<'c>,
) -> RunResult<'c, ()> {
    match &expr_loc.expr {
        Expr::Constant(_) => Ok(()),
        Expr::Name(ident) => {
            if let Some(object) = namespace.get(ident.id) {
                match object {
                    Object::Undefined => Err(InternalRunError::Undefined(ident.name.clone().into()).into()),
                    _ => Ok(()),
                }
            } else {
                let name = ident.name.clone();

                Err(SimpleException::new(ExcType::NameError, Some(name.into()))
                    .with_position(expr_loc.position)
                    .into())
            }
        }
        Expr::Call { callable, args, kwargs } => call_function(namespace, heap, callable, args, kwargs).map(|_| ()),
        Expr::AttrCall {
            object,
            attr,
            args,
            kwargs,
        } => attr_call(namespace, heap, expr_loc, object, attr, args, kwargs).map(|_| ()),
        Expr::Op { left, op, right } => eval_op(namespace, heap, left, op, right).map(|_| ()),
        Expr::CmpOp { left, op, right } => cmp_op(namespace, heap, left, op, right).map(|_| ()),
        Expr::List(elements) => {
            for el in elements {
                evaluate_discard(namespace, heap, el)?;
            }
            Ok(())
        }
        Expr::Tuple(elements) => {
            for el in elements {
                evaluate_discard(namespace, heap, el)?;
            }
            Ok(())
        }
    }
}

/// Specialized helper for truthiness checks; shares implementation with `evaluate`.
pub(crate) fn evaluate_bool<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    expr_loc: &'d ExprLoc<'c>,
) -> RunResult<'c, bool> {
    if let Expr::CmpOp { left, op, right } = &expr_loc.expr {
        cmp_op(namespace, heap, left, op, right)
    } else {
        let obj = evaluate_use(namespace, heap, expr_loc)?;
        Ok(obj.py_bool(heap))
    }
}

/// Evaluates a binary operator expression (`+, -, %`, etc.).
fn eval_op<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    left: &'d ExprLoc<'c>,
    op: &'d Operator,
    right: &'d ExprLoc<'c>,
) -> RunResult<'c, Object> {
    let left_object = evaluate_use(namespace, heap, left)?;
    let right_object = evaluate_use(namespace, heap, right)?;
    let op_object: Option<Object> = match op {
        Operator::Add => left_object.py_add(&right_object, heap),
        Operator::Sub => left_object.py_sub(&right_object, heap),
        Operator::Mod => left_object.py_mod(&right_object),
        _ => return internal_err!(InternalRunError::TodoError; "Operator {op:?} not yet implemented"),
    };
    match op_object {
        Some(object) => Ok(object),
        None => SimpleException::operand_type_error(left, op, right, left_object, right_object, heap),
    }
}

/// Evaluates comparison operators, reusing `evaluate` so heap semantics remain consistent.
fn cmp_op<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    left: &'d ExprLoc<'c>,
    op: &'d CmpOperator,
    right: &'d ExprLoc<'c>,
) -> RunResult<'c, bool> {
    let mut left_object = evaluate_use(namespace, heap, left)?;
    let mut right_object = evaluate_use(namespace, heap, right)?;
    match op {
        CmpOperator::Eq => Ok(left_object.py_eq(&right_object, heap)),
        CmpOperator::NotEq => Ok(!left_object.py_eq(&right_object, heap)),
        CmpOperator::Gt => Ok(left_object.gt(&right_object)),
        CmpOperator::GtE => Ok(left_object.ge(&right_object)),
        CmpOperator::Lt => Ok(left_object.lt(&right_object)),
        CmpOperator::LtE => Ok(left_object.le(&right_object)),
        CmpOperator::Is => Ok(left_object.is(heap, &mut right_object)),
        CmpOperator::IsNot => Ok(!left_object.is(heap, &mut right_object)),
        CmpOperator::ModEq(v) => match left_object.py_mod_eq(&right_object, *v) {
            Some(b) => Ok(b),
            None => SimpleException::operand_type_error(left, Operator::Mod, right, left_object, right_object, heap),
        },
        _ => internal_err!(InternalRunError::TodoError; "Operator {op:?} not yet implemented"),
    }
}

/// Evaluates callable function calls, collecting argument values via the shared heap.
///
/// Handles builtin functions, exception constructors, and (eventually) user-defined functions.
fn call_function<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    callable: &'d Callable,
    args: &'d [ExprLoc<'c>],
    _kwargs: &'d [Kwarg],
) -> RunResult<'c, Object> {
    let args = args
        .iter()
        .map(|a| evaluate_use(namespace, heap, a))
        .collect::<RunResult<_>>()?;
    match callable {
        Callable::Builtin(builtin) => builtin.call(heap, args),
        Callable::Exception(exc_type) => call_exception(heap, args, *exc_type),
        Callable::Ident(_) => {
            internal_err!(InternalRunError::TodoError; "User defined functions not yet implemented")
        }
    }
}

/// Handles attribute method calls like `list.append`, again threading the heap for safety.
fn attr_call<'c, 'd>(
    namespace: &'d mut [Object],
    heap: &'d mut Heap,
    expr_loc: &'d ExprLoc<'c>,
    object_ident: &Identifier<'c>,
    attr: &Attr,
    args: &'d [ExprLoc<'c>],
    _kwargs: &'d [Kwarg],
) -> RunResult<'c, Object> {
    // Evaluate arguments first to avoid borrow conflicts
    let args: Vec<Object> = args
        .iter()
        .map(|a| evaluate_use(namespace, heap, a))
        .collect::<RunResult<_>>()?;

    let object = if let Some(object) = namespace.get_mut(object_ident.id) {
        match object {
            Object::Undefined => return Err(InternalRunError::Undefined(object_ident.name.clone().into()).into()),
            _ => object,
        }
    } else {
        let name = object_ident.name.clone();

        return Err(SimpleException::new(ExcType::NameError, Some(name.into()))
            .with_position(expr_loc.position)
            .into());
    };
    object.call_attr(heap, attr, args)
}

fn call_exception<'c>(heap: &mut Heap, args: Vec<Object>, exc_type: ExcType) -> RunResult<'c, Object> {
    if let Some(first) = args.first() {
        if args.len() == 1 {
            if let Object::Ref(object_id) = first {
                if let HeapData::Str(s) = heap.get(*object_id) {
                    return Ok(Object::Exc(SimpleException::new(
                        exc_type,
                        Some(s.as_str().to_owned().into()),
                    )));
                }
            }
        }
        internal_err!(InternalRunError::TodoError; "Exceptions can only be called with zero or one string argument")
    } else {
        Ok(Object::Exc(SimpleException::new(exc_type, None)))
    }
}
