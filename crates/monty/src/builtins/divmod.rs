//! Implementation of the divmod() builtin function.

use num_bigint::BigInt;
use num_integer::Integer;

use crate::{
    args::ArgValues,
    exception_private::{ExcType, RunResult, SimpleException},
    heap::{Heap, HeapData},
    resource::ResourceTracker,
    types::{LongInt, PyTrait, Tuple},
    value::Value,
};

/// Implementation of the divmod() builtin function.
///
/// Returns a tuple (quotient, remainder) from integer division.
/// Equivalent to (a // b, a % b).
pub fn builtin_divmod(heap: &mut Heap<impl ResourceTracker>, args: ArgValues) -> RunResult<Value> {
    let (a, b) = args.get_two_args("divmod", heap)?;
    let a = super::round::normalize_bool_to_int(a);
    let b = super::round::normalize_bool_to_int(b);

    let result = match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y == 0 {
                Err(ExcType::divmod_by_zero())
            } else {
                // Python uses floor division (toward negative infinity), not Euclidean
                let (quot, rem) = floor_divmod(i64::from(*x), i64::from(*y));
                let quot_val = crate::value::int_value(quot, heap)?;
                let rem_val = crate::value::int_value(rem, heap)?;
                let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                Ok(Value::Ref(tuple_id))
            }
        }
        // Int / LongInt or Int / Float
        (Value::Int(x), Value::Ref(id)) => match heap.get(*id) {
            HeapData::LongInt(li) => {
                if li.is_zero() {
                    Err(ExcType::divmod_by_zero())
                } else {
                    let x_bi = BigInt::from(*x);
                    let (quot, rem) = bigint_floor_divmod(&x_bi, li.inner());
                    let quot_val = LongInt::new(quot).into_value(heap)?;
                    let rem_val = LongInt::new(rem).into_value(heap)?;
                    let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                    Ok(Value::Ref(tuple_id))
                }
            }
            HeapData::Float(y) => {
                if *y == 0.0 {
                    Err(ExcType::divmod_by_zero())
                } else {
                    let xf = f64::from(*x);
                    let quot = (xf / y).floor();
                    let rem = xf - quot * y;
                    let quot_val = Value::Ref(heap.allocate(HeapData::Float(quot))?);
                    let rem_val = Value::Ref(heap.allocate(HeapData::Float(rem))?);
                    let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                    Ok(Value::Ref(tuple_id))
                }
            }
            _ => {
                let a_type = a.py_type(heap);
                let b_type = b.py_type(heap);
                Err(SimpleException::new_msg(
                    ExcType::TypeError,
                    format!("unsupported operand type(s) for divmod(): '{a_type}' and '{b_type}'"),
                )
                .into())
            }
        },
        // LongInt / Int or Float / Int
        (Value::Ref(id), Value::Int(y)) => match heap.get(*id) {
            HeapData::LongInt(li) => {
                if *y == 0 {
                    Err(ExcType::divmod_by_zero())
                } else {
                    let y_bi = BigInt::from(*y);
                    let (quot, rem) = bigint_floor_divmod(li.inner(), &y_bi);
                    let quot_val = LongInt::new(quot).into_value(heap)?;
                    let rem_val = LongInt::new(rem).into_value(heap)?;
                    let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                    Ok(Value::Ref(tuple_id))
                }
            }
            HeapData::Float(x) => {
                if *y == 0 {
                    Err(ExcType::divmod_by_zero())
                } else {
                    let yf = f64::from(*y);
                    let quot = (x / yf).floor();
                    let rem = x - quot * yf;
                    let quot_val = Value::Ref(heap.allocate(HeapData::Float(quot))?);
                    let rem_val = Value::Ref(heap.allocate(HeapData::Float(rem))?);
                    let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                    Ok(Value::Ref(tuple_id))
                }
            }
            _ => {
                let a_type = a.py_type(heap);
                let b_type = b.py_type(heap);
                Err(SimpleException::new_msg(
                    ExcType::TypeError,
                    format!("unsupported operand type(s) for divmod(): '{a_type}' and '{b_type}'"),
                )
                .into())
            }
        },
        // Ref / Ref: LongInt/LongInt, Float/Float, etc.
        (Value::Ref(id1), Value::Ref(id2)) => match (heap.get(*id1), heap.get(*id2)) {
            (HeapData::LongInt(li1), HeapData::LongInt(li2)) => {
                if li2.is_zero() {
                    Err(ExcType::divmod_by_zero())
                } else {
                    let (quot, rem) = bigint_floor_divmod(li1.inner(), li2.inner());
                    let quot_val = LongInt::new(quot).into_value(heap)?;
                    let rem_val = LongInt::new(rem).into_value(heap)?;
                    let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                    Ok(Value::Ref(tuple_id))
                }
            }
            (HeapData::Float(x), HeapData::Float(y)) => {
                if *y == 0.0 {
                    Err(ExcType::divmod_by_zero())
                } else {
                    let quot = (x / y).floor();
                    let rem = x - quot * y;
                    let quot_val = Value::Ref(heap.allocate(HeapData::Float(quot))?);
                    let rem_val = Value::Ref(heap.allocate(HeapData::Float(rem))?);
                    let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                    Ok(Value::Ref(tuple_id))
                }
            }
            (HeapData::Float(x), HeapData::LongInt(li)) => {
                if li.is_zero() {
                    Err(ExcType::divmod_by_zero())
                } else {
                    let y = li.to_f64().unwrap_or(f64::INFINITY);
                    let quot = (x / y).floor();
                    let rem = x - quot * y;
                    let quot_val = Value::Ref(heap.allocate(HeapData::Float(quot))?);
                    let rem_val = Value::Ref(heap.allocate(HeapData::Float(rem))?);
                    let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                    Ok(Value::Ref(tuple_id))
                }
            }
            (HeapData::LongInt(li), HeapData::Float(y)) => {
                if *y == 0.0 {
                    Err(ExcType::divmod_by_zero())
                } else {
                    let x = li.to_f64().unwrap_or(f64::INFINITY);
                    let quot = (x / y).floor();
                    let rem = x - quot * y;
                    let quot_val = Value::Ref(heap.allocate(HeapData::Float(quot))?);
                    let rem_val = Value::Ref(heap.allocate(HeapData::Float(rem))?);
                    let tuple_id = heap.allocate(HeapData::Tuple(Tuple::new(vec![quot_val, rem_val])))?;
                    Ok(Value::Ref(tuple_id))
                }
            }
            _ => {
                let a_type = a.py_type(heap);
                let b_type = b.py_type(heap);
                Err(SimpleException::new_msg(
                    ExcType::TypeError,
                    format!("unsupported operand type(s) for divmod(): '{a_type}' and '{b_type}'"),
                )
                .into())
            }
        },
        _ => {
            let a_type = a.py_type(heap);
            let b_type = b.py_type(heap);
            Err(SimpleException::new_msg(
                ExcType::TypeError,
                format!("unsupported operand type(s) for divmod(): '{a_type}' and '{b_type}'"),
            )
            .into())
        }
    };

    a.drop_with_heap(heap);
    b.drop_with_heap(heap);
    result
}

/// Computes Python-style floor division and modulo.
///
/// Python's division rounds toward negative infinity (floor division),
/// and the remainder has the same sign as the divisor.
/// This differs from Rust's truncating division and Euclidean division.
fn floor_divmod(a: i64, b: i64) -> (i64, i64) {
    // Use truncating division first
    let quot = a / b;
    let rem = a % b;

    // Adjust for floor division: if signs differ and remainder != 0, adjust
    if rem != 0 && (rem < 0) != (b < 0) {
        (quot - 1, rem + b)
    } else {
        (quot, rem)
    }
}

/// Computes Python-style floor division and modulo for BigInts.
///
/// Uses `div_mod_floor` from num_integer for correct floor semantics.
fn bigint_floor_divmod(a: &BigInt, b: &BigInt) -> (BigInt, BigInt) {
    a.div_mod_floor(b)
}
