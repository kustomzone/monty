use monty::{ExecutorIter, PyObject, ResourceLimits};

#[test]
fn executor_iter_no_yield_completes() {
    let exec = ExecutorIter::new("x + 1", "test.py", &["x"]).unwrap();
    let result = exec.run_no_limits(vec![PyObject::Int(41)]).unwrap();
    assert_eq!(result.into_complete().expect("complete"), PyObject::Int(42));
}

#[test]
fn executor_iter_single_yield() {
    let exec = ExecutorIter::new("yield 42", "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("yield");
    assert_eq!(value, PyObject::Int(42));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::None);
}

#[test]
fn executor_iter_multiple_yields() {
    let exec = ExecutorIter::new("yield 1\nyield 2\n3", "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("first yield");
    assert_eq!(value, PyObject::Int(1));

    let (value, state) = state.run().unwrap().into_yield().expect("second yield");
    assert_eq!(value, PyObject::Int(2));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::Int(3));
}

#[test]
fn executor_iter_yield_preserves_state() {
    // Test that variables persist across yields
    let code = "x = 1\nyield x\nx = x + 1\nyield x\nx = x + 1\nx";
    let exec = ExecutorIter::new(code, "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("first yield");
    assert_eq!(value, PyObject::Int(1));

    let (value, state) = state.run().unwrap().into_yield().expect("second yield");
    assert_eq!(value, PyObject::Int(2));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::Int(3));
}

#[test]
fn executor_iter_yield_with_inputs() {
    let code = "yield x\nyield x + 1\nx + 2";
    let exec = ExecutorIter::new(code, "test.py", &["x"]).unwrap();

    let (value, state) = exec
        .run_with_limits(vec![PyObject::Int(10)], ResourceLimits::new().max_allocations(10))
        .unwrap()
        .into_yield()
        .expect("first yield");
    assert_eq!(value, PyObject::Int(10));

    let (value, state) = state.run().unwrap().into_yield().expect("second yield");
    assert_eq!(value, PyObject::Int(11));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::Int(12));
}

#[test]
fn executor_iter_yield_none() {
    // yield without a value yields None
    let exec = ExecutorIter::new("yield\n42", "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("yield");
    assert_eq!(value, PyObject::None);

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::Int(42));
}

#[test]
fn executor_iter_yield_expression() {
    // yield with a complex expression
    let code = "x = [1, 2, 3]\nyield x[1] + 10\nlen(x)";
    let exec = ExecutorIter::new(code, "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("yield");
    assert_eq!(value, PyObject::Int(12)); // x[1] + 10 = 2 + 10

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::Int(3)); // len(x)
}

#[test]
fn clone_executor_iter() {
    let exec1 = ExecutorIter::new("yield 42", "test.py", &[]).unwrap();
    let exec2 = exec1.clone();

    let (value, state) = exec1.run_no_limits(vec![]).unwrap().into_yield().expect("yield");
    assert_eq!(value, PyObject::Int(42));
    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::None);

    let (value, state) = exec2.run_no_limits(vec![]).unwrap().into_yield().expect("yield");
    assert_eq!(value, PyObject::Int(42));
    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::None);
}

#[test]
fn executor_iter_yield_in_if_true_branch() {
    // Test yield inside an if block when condition is true
    let code = "x = 1\nif x == 1:\n    yield 10\n    yield 20\n30";
    let exec = ExecutorIter::new(code, "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("first yield");
    assert_eq!(value, PyObject::Int(10));

    let (value, state) = state.run().unwrap().into_yield().expect("second yield");
    assert_eq!(value, PyObject::Int(20));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::Int(30));
}

#[test]
fn executor_iter_yield_in_if_false_branch() {
    // Test yield inside an else block when condition is false
    let code = "x = 0\nif x == 1:\n    yield 10\nelse:\n    yield 20\n    yield 30\n40";
    let exec = ExecutorIter::new(code, "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("first yield");
    assert_eq!(value, PyObject::Int(20));

    let (value, state) = state.run().unwrap().into_yield().expect("second yield");
    assert_eq!(value, PyObject::Int(30));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::Int(40));
}

#[test]
fn executor_iter_yield_in_both_if_branches() {
    // Test that correct branch is taken based on condition
    let code = "if 1 == 1:\n    yield 'true'\nelse:\n    yield 'false'\n'done'";
    let exec = ExecutorIter::new(code, "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("yield");
    assert_eq!(value, PyObject::String("true".to_string()));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::String("done".to_string()));
}

#[test]
fn executor_iter_yield_in_for_loop() {
    // Test yield inside a for loop
    let code = "for i in range(3):\n    yield i\n'done'";
    let exec = ExecutorIter::new(code, "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("yield 0");
    assert_eq!(value, PyObject::Int(0));

    let (value, state) = state.run().unwrap().into_yield().expect("yield 1");
    assert_eq!(value, PyObject::Int(1));

    let (value, state) = state.run().unwrap().into_yield().expect("yield 2");
    assert_eq!(value, PyObject::Int(2));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::String("done".to_string()));
}

#[test]
fn executor_iter_yield_multiple_in_for_loop() {
    // Test multiple yields per iteration
    let code = "for i in range(2):\n    yield i\n    yield i + 10\n'done'";
    let exec = ExecutorIter::new(code, "test.py", &[]).unwrap();

    // First iteration
    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("yield 0");
    assert_eq!(value, PyObject::Int(0));

    let (value, state) = state.run().unwrap().into_yield().expect("yield 10");
    assert_eq!(value, PyObject::Int(10));

    // Second iteration
    let (value, state) = state.run().unwrap().into_yield().expect("yield 1");
    assert_eq!(value, PyObject::Int(1));

    let (value, state) = state.run().unwrap().into_yield().expect("yield 11");
    assert_eq!(value, PyObject::Int(11));

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::String("done".to_string()));
}

#[test]
fn executor_iter_yield_in_for_with_state() {
    // Test that state persists across for loop iterations
    let code = "total = 0\nfor i in range(3):\n    total = total + i\n    yield total\ntotal";
    let exec = ExecutorIter::new(code, "test.py", &[]).unwrap();

    let (value, state) = exec.run_no_limits(vec![]).unwrap().into_yield().expect("first yield");
    assert_eq!(value, PyObject::Int(0)); // total = 0 + 0

    let (value, state) = state.run().unwrap().into_yield().expect("second yield");
    assert_eq!(value, PyObject::Int(1)); // total = 0 + 1

    let (value, state) = state.run().unwrap().into_yield().expect("third yield");
    assert_eq!(value, PyObject::Int(3)); // total = 1 + 2

    let result = state.run().unwrap().into_complete().expect("complete");
    assert_eq!(result, PyObject::Int(3));
}
