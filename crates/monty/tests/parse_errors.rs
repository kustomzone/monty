use monty::{ExcType, Executor, PythonException};

/// Tests that unimplemented features return `NotImplementedError` exceptions.
mod not_implemented_error {
    use super::*;

    /// Helper to extract the exception type from a parse error.
    fn get_exc_type(result: Result<Executor, PythonException>) -> ExcType {
        let err = result.expect_err("expected parse error");
        err.exc_type
    }

    /// Helper to extract the exception message from a parse error.
    fn get_exc_message(result: Result<Executor, PythonException>) -> String {
        let err = result.expect_err("expected parse error");
        err.message.unwrap_or_default()
    }

    #[test]
    fn complex_numbers_return_not_implemented_error() {
        let result = Executor::new("1 + 2j".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn complex_numbers_have_descriptive_message() {
        let result = Executor::new("1 + 2j".to_owned(), "test.py", &[]);
        let msg = get_exc_message(result);
        assert!(msg.contains("complex"), "message should mention 'complex', got: {msg}");
    }

    #[test]
    fn async_functions_return_not_implemented_error() {
        let result = Executor::new("async def foo(): pass".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn yield_expressions_return_not_implemented_error() {
        // Yield expressions are not supported and fail at parse time
        let result = Executor::new("def foo():\n    yield 1".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
        let result = Executor::new("def foo():\n    yield 1".to_owned(), "test.py", &[]);
        let msg = get_exc_message(result);
        assert!(msg.contains("yield"), "message should mention 'yield', got: {msg}");
    }

    #[test]
    fn classes_return_not_implemented_error() {
        let result = Executor::new("class Foo: pass".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn imports_return_not_implemented_error() {
        let result = Executor::new("import os".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn with_statement_returns_not_implemented_error() {
        let result = Executor::new("with open('f') as f: pass".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn try_except_returns_not_implemented_error() {
        let result = Executor::new("try:\n    pass\nexcept:\n    pass".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn lambda_returns_not_implemented_error() {
        let result = Executor::new("x = lambda: 1".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::NotImplementedError);
    }

    #[test]
    fn error_display_format() {
        // Verify the Display format matches Python's exception output
        let result = Executor::new("1 + 2j".to_owned(), "test.py", &[]);
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
    fn get_exc_type(result: Result<Executor, PythonException>) -> ExcType {
        let err = result.expect_err("expected parse error");
        err.exc_type
    }

    #[test]
    fn invalid_fstring_format_spec_returns_syntax_error() {
        let result = Executor::new("f'{1:10xyz}'".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::SyntaxError);
    }

    #[test]
    fn invalid_fstring_format_spec_str_returns_syntax_error() {
        let result = Executor::new("f'{\"hello\":abc}'".to_owned(), "test.py", &[]);
        assert_eq!(get_exc_type(result), ExcType::SyntaxError);
    }

    #[test]
    fn syntax_error_display_format() {
        let result = Executor::new("f'{1:10xyz}'".to_owned(), "test.py", &[]);
        let err = result.expect_err("expected parse error");
        let display = err.to_string();
        assert!(
            display.contains("SyntaxError:"),
            "display should contain 'SyntaxError:', got: {display}"
        );
    }
}
