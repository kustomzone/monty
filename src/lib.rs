mod evaluate;
mod exceptions;
mod object;
mod parse;
mod parse_error;
mod prepare;
mod run;
mod types;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use crate::exceptions::{InternalRunError, RunError};
pub use crate::object::Object;
use crate::parse::parse;
pub use crate::parse_error::{ParseError, ParseResult};
use crate::prepare::prepare;
use crate::run::RunFrame;
pub use crate::types::Exit;
use crate::types::Node;

#[derive(Debug)]
pub struct Executor<'c> {
    initial_namespace: Vec<Object>,
    nodes: Vec<Node<'c>>,
}

impl<'c> Executor<'c> {
    pub fn new(code: &'c str, filename: &'c str, input_names: &[&str]) -> ParseResult<'c, Self> {
        let nodes = parse(code, filename)?;
        // dbg!(&nodes);
        let (initial_namespace, nodes) = prepare(nodes, input_names)?;
        // dbg!(&initial_namespace, &nodes);
        Ok(Self {
            initial_namespace,
            nodes,
        })
    }

    pub fn run(&self, inputs: Vec<Object>) -> Result<Exit<'c>, InternalRunError> {
        let mut namespace = self.initial_namespace.clone();
        for (i, input) in inputs.into_iter().enumerate() {
            namespace[i] = input;
        }
        match RunFrame::new(namespace).execute(&self.nodes) {
            Ok(v) => Ok(v),
            Err(e) => match e {
                RunError::Exc(exc) => Ok(Exit::Raise(exc)),
                RunError::Internal(internal) => Err(internal),
            },
        }
    }
}

/// parse code and show the parsed AST, mostly for testing
pub fn parse_show(code: &str, filename: &str) -> Result<String, String> {
    match parse(code, filename) {
        Ok(ast) => Ok(format!("{ast:#?}")),
        Err(e) => Err(e.to_string()),
    }
}
