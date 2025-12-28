//! Tests for passing input values to the executor.
//!
//! These tests verify that `PyObject` inputs are correctly converted to `Object`
//! and can be used in Python code execution.

use indexmap::IndexMap;
use monty::{ExcType, Executor, PyObject};

// === Immediate Value Tests ===

#[test]
fn input_int() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Int(42)]).unwrap();
    assert_eq!(result, PyObject::Int(42));
}

#[test]
fn input_int_arithmetic() {
    let ex = Executor::new("x + 1".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Int(41)]).unwrap();
    assert_eq!(result, PyObject::Int(42));
}

#[test]
fn input_bool_true() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Bool(true)]).unwrap();
    assert_eq!(result, PyObject::Bool(true));
}

#[test]
fn input_bool_false() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Bool(false)]).unwrap();
    assert_eq!(result, PyObject::Bool(false));
}

#[test]
fn input_float() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Float(2.5)]).unwrap();
    assert_eq!(result, PyObject::Float(2.5));
}

#[test]
fn input_none() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::None]).unwrap();
    assert_eq!(result, PyObject::None);
}

#[test]
fn input_ellipsis() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Ellipsis]).unwrap();
    assert_eq!(result, PyObject::Ellipsis);
}

// === Heap-Allocated Value Tests ===

#[test]
fn input_string() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::String("hello".to_string())]).unwrap();
    assert_eq!(result, PyObject::String("hello".to_string()));
}

#[test]
fn input_string_concat() {
    let ex = Executor::new("x + ' world'".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::String("hello".to_string())]).unwrap();
    assert_eq!(result, PyObject::String("hello world".to_string()));
}

#[test]
fn input_bytes() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Bytes(vec![1, 2, 3])]).unwrap();
    assert_eq!(result, PyObject::Bytes(vec![1, 2, 3]));
}

#[test]
fn input_list() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::List(vec![PyObject::Int(1), PyObject::Int(2)])])
        .unwrap();
    assert_eq!(result, PyObject::List(vec![PyObject::Int(1), PyObject::Int(2)]));
}

#[test]
fn input_list_append() {
    let ex = Executor::new("x.append(3)\nx".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::List(vec![PyObject::Int(1), PyObject::Int(2)])])
        .unwrap();
    assert_eq!(
        result,
        PyObject::List(vec![PyObject::Int(1), PyObject::Int(2), PyObject::Int(3)])
    );
}

#[test]
fn input_tuple() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::Tuple(vec![
            PyObject::Int(1),
            PyObject::String("two".to_string()),
        ])])
        .unwrap();
    assert_eq!(
        result,
        PyObject::Tuple(vec![PyObject::Int(1), PyObject::String("two".to_string())])
    );
}

#[test]
fn input_dict() {
    let mut map = IndexMap::new();
    map.insert(PyObject::String("a".to_string()), PyObject::Int(1));

    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Dict(map)]).unwrap();

    // Build expected map for comparison
    let mut expected = IndexMap::new();
    expected.insert(PyObject::String("a".to_string()), PyObject::Int(1));
    assert_eq!(result, PyObject::Dict(expected));
}

#[test]
fn input_dict_get() {
    let mut map = IndexMap::new();
    map.insert(PyObject::String("key".to_string()), PyObject::Int(42));

    let ex = Executor::new("x['key']".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Dict(map)]).unwrap();
    assert_eq!(result, PyObject::Int(42));
}

// === Multiple Inputs ===

#[test]
fn multiple_inputs_two() {
    let ex = Executor::new("x + y".to_owned(), "test.py", &["x", "y"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Int(10), PyObject::Int(32)]).unwrap();
    assert_eq!(result, PyObject::Int(42));
}

#[test]
fn multiple_inputs_three() {
    let ex = Executor::new("x + y + z".to_owned(), "test.py", &["x", "y", "z"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::Int(10), PyObject::Int(20), PyObject::Int(12)])
        .unwrap();
    assert_eq!(result, PyObject::Int(42));
}

#[test]
fn multiple_inputs_mixed_types() {
    // Create a list from two inputs
    let ex = Executor::new("[x, y]".to_owned(), "test.py", &["x", "y"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::Int(1), PyObject::String("two".to_string())])
        .unwrap();
    assert_eq!(
        result,
        PyObject::List(vec![PyObject::Int(1), PyObject::String("two".to_string())])
    );
}

// === Edge Cases ===

#[test]
fn no_inputs() {
    let ex = Executor::new("42".to_owned(), "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    assert_eq!(result, PyObject::Int(42));
}

#[test]
fn nested_list() {
    let ex = Executor::new("x[0][1]".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::List(vec![PyObject::List(vec![
            PyObject::Int(1),
            PyObject::Int(2),
        ])])])
        .unwrap();
    assert_eq!(result, PyObject::Int(2));
}

#[test]
fn empty_list_input() {
    let ex = Executor::new("len(x)".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::List(vec![])]).unwrap();
    assert_eq!(result, PyObject::Int(0));
}

#[test]
fn empty_string_input() {
    let ex = Executor::new("len(x)".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::String(String::new())]).unwrap();
    assert_eq!(result, PyObject::Int(0));
}

// === Exception Input Tests ===

#[test]
fn input_exception() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::Exception {
            exc_type: ExcType::ValueError,
            arg: Some("test message".to_string()),
        }])
        .unwrap();
    assert_eq!(
        result,
        PyObject::Exception {
            exc_type: ExcType::ValueError,
            arg: Some("test message".to_string()),
        }
    );
}

#[test]
fn input_exception_no_arg() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::Exception {
            exc_type: ExcType::TypeError,
            arg: None,
        }])
        .unwrap();
    assert_eq!(
        result,
        PyObject::Exception {
            exc_type: ExcType::TypeError,
            arg: None,
        }
    );
}

#[test]
fn input_exception_in_list() {
    let ex = Executor::new("x[0]".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex
        .run_no_limits(vec![PyObject::List(vec![PyObject::Exception {
            exc_type: ExcType::KeyError,
            arg: Some("key".to_string()),
        }])])
        .unwrap();
    assert_eq!(
        result,
        PyObject::Exception {
            exc_type: ExcType::KeyError,
            arg: Some("key".to_string()),
        }
    );
}

#[test]
fn input_exception_raise() {
    // Test that an exception passed as input can be raised
    let ex = Executor::new("raise x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Exception {
        exc_type: ExcType::ValueError,
        arg: Some("input error".to_string()),
    }]);
    let exc = result.unwrap_err();
    assert_eq!(exc.exc_type, ExcType::ValueError);
    assert_eq!(exc.message, Some("input error".to_string()));
}

// === Invalid Input Tests ===

#[test]
fn invalid_input_repr() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![PyObject::Repr("some repr".to_string())]);
    assert!(result.is_err(), "Repr should not be a valid input");
}

#[test]
fn invalid_input_repr_nested_in_list() {
    let ex = Executor::new("x".to_owned(), "test.py", &["x"]).unwrap();
    // Repr nested inside a list should still be invalid
    let result = ex.run_no_limits(vec![PyObject::List(vec![PyObject::Repr("nested repr".to_string())])]);
    assert!(result.is_err(), "Repr nested in list should be invalid");
}
