//! Tests for JSON serialization and deserialization of `PyObject`.
//!
//! JSON mapping:
//! - Bidirectional: null↔None, bool↔Bool, int↔Int, float↔Float, string↔String, array↔List, object↔Dict
//! - Output-only: Ellipsis, Tuple, Bytes, Exception, Repr (serialize but cannot deserialize)

use indexmap::IndexMap;
use monty::{ExcType, Executor, PyObject};

// === JSON Input Tests ===

#[test]
fn json_input_primitives() {
    // Test all primitive JSON types deserialize correctly and work as inputs
    let int: PyObject = serde_json::from_str("42").unwrap();
    let float: PyObject = serde_json::from_str("2.5").unwrap();
    let string: PyObject = serde_json::from_str(r#""hello""#).unwrap();
    let bool_val: PyObject = serde_json::from_str("true").unwrap();
    let null: PyObject = serde_json::from_str("null").unwrap();

    assert_eq!(int, PyObject::Int(42));
    assert_eq!(float, PyObject::Float(2.5));
    assert_eq!(string, PyObject::String("hello".to_string()));
    assert_eq!(bool_val, PyObject::Bool(true));
    assert_eq!(null, PyObject::None);
}

#[test]
fn json_input_run_code() {
    // Deserialize JSON and use as input to executor
    let input: PyObject = serde_json::from_str(r#"{"x": 10, "y": 32}"#).unwrap();
    let ex = Executor::new("data['x'] + data['y']", "test.py", &["data"]).unwrap();
    let result = ex.run_no_limits(vec![input]).unwrap();
    assert_eq!(result, PyObject::Int(42));
}

#[test]
fn json_input_nested() {
    let input: PyObject = serde_json::from_str(r#"{"outer": {"inner": [1, 2, 3]}}"#).unwrap();
    let ex = Executor::new("x['outer']['inner'][1]", "test.py", &["x"]).unwrap();
    let result = ex.run_no_limits(vec![input]).unwrap();
    assert_eq!(result, PyObject::Int(2));
}

// === JSON Output Tests ===

#[test]
fn json_output_primitives() {
    // Test all primitive types serialize to natural JSON
    assert_eq!(serde_json::to_string(&PyObject::Int(42)).unwrap(), "42");
    assert_eq!(serde_json::to_string(&PyObject::Float(1.5)).unwrap(), "1.5");
    assert_eq!(
        serde_json::to_string(&PyObject::String("hi".into())).unwrap(),
        r#""hi""#
    );
    assert_eq!(serde_json::to_string(&PyObject::Bool(true)).unwrap(), "true");
    assert_eq!(serde_json::to_string(&PyObject::None).unwrap(), "null");
}

#[test]
fn json_output_list() {
    let ex = Executor::new("[1, 'two', 3.0]", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    assert_eq!(serde_json::to_string(&result).unwrap(), r#"[1,"two",3.0]"#);
}

#[test]
fn json_output_dict() {
    let ex = Executor::new("{'a': 1, 'b': 2}", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    assert_eq!(serde_json::to_string(&result).unwrap(), r#"{"a":1,"b":2}"#);
}

#[test]
fn json_output_dict_nonstring_key() {
    // Dict with non-string key uses py_repr for the key
    let mut map = IndexMap::new();
    map.insert(PyObject::Int(42), PyObject::String("value".to_string()));
    let obj = PyObject::Dict(map);
    assert_eq!(serde_json::to_string(&obj).unwrap(), r#"{"42":"value"}"#);
}

// === Output-only types (cannot deserialize from JSON) ===

#[test]
fn json_output_tuple() {
    let ex = Executor::new("(1, 'two')", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    assert_eq!(serde_json::to_string(&result).unwrap(), r#"{"$tuple":[1,"two"]}"#);
}

#[test]
fn json_output_bytes() {
    let ex = Executor::new("b'hi'", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    assert_eq!(serde_json::to_string(&result).unwrap(), r#"{"$bytes":[104,105]}"#);
}

#[test]
fn json_output_ellipsis() {
    let ex = Executor::new("...", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    assert_eq!(serde_json::to_string(&result).unwrap(), r#"{"$ellipsis":true}"#);
}

#[test]
fn json_output_exception() {
    let obj = PyObject::Exception {
        exc_type: ExcType::ValueError,
        arg: Some("test".to_string()),
    };
    assert_eq!(
        serde_json::to_string(&obj).unwrap(),
        r#"{"$exception":{"type":"ValueError","arg":"test"}}"#
    );
}

#[test]
fn json_output_repr() {
    let obj = PyObject::Repr("<function foo>".to_string());
    assert_eq!(serde_json::to_string(&obj).unwrap(), r#"{"$repr":"<function foo>"}"#);
}

#[test]
fn json_output_cycle_list() {
    // Test JSON serialization of cyclic list
    let ex = Executor::new("a = []; a.append(a); a", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    // The cyclic reference becomes PyObject::Cycle("[...]")
    assert_eq!(serde_json::to_string(&result).unwrap(), r#"[{"$cycle":"[...]"}]"#);
}

#[test]
fn json_output_cycle_dict() {
    // Test JSON serialization of cyclic dict
    let ex = Executor::new("d = {}; d['self'] = d; d", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    // The cyclic reference becomes PyObject::Cycle("{...}")
    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        r#"{"self":{"$cycle":"{...}"}}"#
    );
}

// === Round-trip Tests ===

#[test]
fn json_roundtrip() {
    // Values that can round-trip through JSON
    let ex = Executor::new("{'items': [1, 'two', None], 'flag': True}", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();
    let json = serde_json::to_string(&result).unwrap();
    let parsed: PyObject = serde_json::from_str(&json).unwrap();
    assert_eq!(result, parsed);
}

#[test]
fn json_roundtrip_empty() {
    // Empty structures round-trip correctly
    let list: PyObject = serde_json::from_str("[]").unwrap();
    let dict: PyObject = serde_json::from_str("{}").unwrap();
    assert_eq!(serde_json::to_string(&list).unwrap(), "[]");
    assert_eq!(serde_json::to_string(&dict).unwrap(), "{}");
}

// === Cycle Equality Tests ===

#[test]
fn cycle_equality_same_id() {
    // Multiple references to the same cyclic object should produce equal Cycle values
    // because they share the same heap ID
    let ex = Executor::new("a = []; a.append(a); [a, a]", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();

    // Result should be a list containing two identical cyclic lists
    if let PyObject::List(outer) = &result {
        assert_eq!(outer.len(), 2, "outer list should have 2 elements");

        // Both inner lists should contain the same Cycle reference
        if let (PyObject::List(inner1), PyObject::List(inner2)) = (&outer[0], &outer[1]) {
            assert_eq!(inner1.len(), 1);
            assert_eq!(inner2.len(), 1);

            // The cycle references should be equal (same heap ID)
            assert_eq!(inner1[0], inner2[0], "cycles referencing same object should be equal");

            // Verify they are actually Cycle variants
            assert!(matches!(&inner1[0], PyObject::Cycle(..)));
        } else {
            panic!("expected inner lists");
        }
    } else {
        panic!("expected outer list");
    }
}

#[test]
fn cycle_equality_different_ids() {
    // Two separate cyclic objects should produce unequal Cycle values
    // because they have different heap IDs
    let ex = Executor::new("a = []; a.append(a); b = []; b.append(b); [a, b]", "test.py", &[]).unwrap();
    let result = ex.run_no_limits(vec![]).unwrap();

    // Result should be a list containing two different cyclic lists
    if let PyObject::List(outer) = &result {
        assert_eq!(outer.len(), 2, "outer list should have 2 elements");

        // Both inner lists contain their own cycle references
        if let (PyObject::List(inner1), PyObject::List(inner2)) = (&outer[0], &outer[1]) {
            assert_eq!(inner1.len(), 1);
            assert_eq!(inner2.len(), 1);

            // The cycle references should NOT be equal (different heap IDs)
            assert_ne!(
                inner1[0], inner2[0],
                "cycles referencing different objects should not be equal"
            );

            // Verify they are both Cycle variants with same placeholder but different IDs
            if let (PyObject::Cycle(id1, ph1), PyObject::Cycle(id2, ph2)) = (&inner1[0], &inner2[0]) {
                assert_ne!(id1, id2, "heap IDs should differ");
                assert_eq!(ph1, ph2, "placeholders should match (both are lists)");
                assert_eq!(*ph1, "[...]");
            } else {
                panic!("expected Cycle variants");
            }
        } else {
            panic!("expected inner lists");
        }
    } else {
        panic!("expected outer list");
    }
}
