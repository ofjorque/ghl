//! Syntax, Tokenizer, AST and Parser for GHL (Generalized Hypothesis Language).

pub mod ast;
pub mod doc_comment;
pub mod fmt;
pub mod lexer;
pub mod parser;
pub mod source;

pub use ast::*;
pub use doc_comment::{
    ItemDoc, ItemKind, extract_doc_comments, extract_raw_doc_comment, generate_project_docs_html,
    generate_project_docs_markdown, parse_doc_comment_fields,
};
pub use fmt::{
    CommentTrivia, TextRange, extract_comments, format_program, format_program_with_comments,
    format_range, format_source,
};
pub use lexer::{SpannedToken, Token, lex};
pub use parser::{parse, parse_spanned};
pub use source::{SourceIndex, SyntaxError};
