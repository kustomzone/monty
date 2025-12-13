use monty::exceptions::ExcType;
use monty::{Executor, ParseError, PyObject, RunError};

/// Tests that unimplemented features return `NotImplementedError` exceptions.
mod not_implemented_error {
    use super::*;

    /// Helper to extract the exception type from a parse error.
    fn get_exc_type(result: Result<Executor, ParseError>) -> ExcType {
        let err = result.expect_err("expected parse error");
        match err {
            ParseError::PreEvalExc(exc) => exc.exc.exc_type(),
            other => panic!("expected PreEvalExc, got: {other}"),
        }
    }

    /// Helper to extract the exception message from a parse error.
    fn get_exc_message(result: Result<Executor, ParseError>) -> String {
        let err = result.expect_err("expected parse error");
        match err {
            ParseError::PreEvalExc(exc) => exc.exc.arg().map_or(String::new(), std::string::ToString::to_string),
            other => panic!("expected PreEvalExc, got: {other}"),
        }
    }

    #[test]
    fn complex_numbers_return_not_implemented_error() {
        let result = Executor::new("1 + 2j", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn complex_numbers_have_descriptive_message() {
        let result = Executor::new("1 + 2j", "test.py", &[]);
        let msg = get_exc_message(result);
        assert!(msg.contains("complex"), "message should mention 'complex', got: {msg}");
    }

    #[test]
    fn async_functions_return_not_implemented_error() {
        let result = Executor::new("async def foo(): pass", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn generators_return_not_implemented_error_at_runtime() {
        // Yield inside a function parses successfully but fails at runtime when called.
        // The yield statement is parsed as module-level yield is now supported.
        // When the function is called, it raises NotImplementedError.

        let code = "def foo():\n    yield 1\nfoo()";
        let exec = monty::ExecutorIter::new(code, "test.py", &[]).expect("should parse successfully");
        let result = exec.run_no_limits(vec![] as Vec<PyObject>);
        // Should fail at runtime with NotImplementedError when calling foo()
        let err = result.expect_err("expected runtime error");
        match err {
            RunError::Exc(exc) => {
                assert_eq!(exc.exc.exc_type(), ExcType::NotImplementedError);
                let msg = exc.exc.arg().map_or(String::new(), std::string::ToString::to_string);
                assert!(
                    msg.contains("yield inside functions"),
                    "message should mention 'yield inside functions', got: {msg}"
                );
            }
            other => panic!("expected Exc error, got: {other:?}"),
        }
    }

    #[test]
    fn classes_return_not_implemented_error() {
        let result = Executor::new("class Foo: pass", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn imports_return_not_implemented_error() {
        let result = Executor::new("import os", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn with_statement_returns_not_implemented_error() {
        let result = Executor::new("with open('f') as f: pass", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn try_except_returns_not_implemented_error() {
        let result = Executor::new("try:\n    pass\nexcept:\n    pass", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn lambda_returns_not_implemented_error() {
        let result = Executor::new("x = lambda: 1", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn error_display_format() {
        // Verify the Display format matches Python's exception output
        let result = Executor::new("1 + 2j", "test.py", &[]);
        let err = result.expect_err("expected parse error");
        let display = err.to_string();
        assert!(
            display.starts_with("NotImplementedError:"),
            "display should start with 'NotImplementedError:', got: {display}"
        );
        assert!(
            display.contains("monty syntax parser"),
            "display should mention 'monty syntax parser', got: {display}"
        );
    }
}

/// Tests that syntax errors return `SyntaxError` exceptions.
mod syntax_error {
    use super::*;

    /// Helper to extract the exception type from a parse error.
    fn get_exc_type(result: Result<Executor, ParseError>) -> ExcType {
        let err = result.expect_err("expected parse error");
        match err {
            ParseError::PreEvalExc(exc) => exc.exc.exc_type(),
            other => panic!("expected PreEvalExc, got: {other}"),
        }
    }

    #[test]
    fn invalid_fstring_format_spec_returns_syntax_error() {
        let result = Executor::new("f'{1:10xyz}'", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::SyntaxError);
    }

    #[test]
    fn invalid_fstring_format_spec_str_returns_syntax_error() {
        let result = Executor::new("f'{\"hello\":abc}'", "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::SyntaxError);
    }

    #[test]
    fn syntax_error_display_format() {
        let result = Executor::new("f'{1:10xyz}'", "test.py", &[]);
        let err = result.expect_err("expected parse error");
        let display = err.to_string();
        assert!(
            display.starts_with("SyntaxError:"),
            "display should start with 'SyntaxError:', got: {display}"
        );
    }
}
