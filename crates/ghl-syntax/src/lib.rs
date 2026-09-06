//! Syntax, Tokenizer, AST and Parser for GHL (Generalized Hypothesis Language).

pub mod lexer;
pub mod ast;
pub mod parser;

pub use lexer::{Token, SpannedToken, lex};
pub use ast::*;
pub use parser::parse;
