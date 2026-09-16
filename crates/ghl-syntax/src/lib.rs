//! Syntax, Tokenizer, AST and Parser for GHL (Generalized Hypothesis Language).

pub mod lexer;
pub mod ast;
pub mod parser;
pub mod source;
pub mod fmt;
pub mod doc_comment;

pub use lexer::{Token, SpannedToken, lex};
pub use ast::*;
pub use parser::{parse, parse_spanned};
pub use source::{SourceIndex, SyntaxError};
pub use fmt::{format_source, format_program};
pub use doc_comment::{
    ItemDoc, ItemKind, extract_doc_comments, extract_raw_doc_comment,
    parse_doc_comment_fields, generate_project_docs_markdown, generate_project_docs_html,
};

