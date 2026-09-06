//! Syntax, Tokenizer, and AST definitions for GHL (Generalized Hypothesis Language).
//!
//! Implements the normative lexicon defined in RFC 01 Section 7.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords (Declarations)
    Fn,
    Let,
    Mut,
    Const,
    Struct,
    Enum,
    Trait,
    Impl,
    Type,

    // Keywords (Control Flow)
    If,
    Else,
    Match,
    For,
    In,
    While,
    Return,
    Break,
    Continue,

    // Keywords (Modules & Systems)
    Use,
    Pub,
    Mod,
    As,
    Extern,
    Async,
    Await,

    // Special Language Constructors
    DataFrame,
    Mat,
    Col,

    // Literals
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    BoolLit(bool),
    NA,
    NAReason(String), // e.g. NA:NoResponse, NA:NotObservable

    // Statistical & Flow Operators
    Pipe,       // |>
    Tilde,      // ~
    Underscore, // _

    // Linear Algebra & Arithmetic Operators
    Backslash, // \ (Matrix solve)
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %
    Caret,     // ^

    // Element-wise Matrix Ops
    DotStar,  // .*
    DotPlus,  // .+
    DotMinus, // .-
    DotSlash, // ./

    // Comparison & Logic
    EqEq,       // ==
    NotEq,      // !=
    Lt,         // <
    LtEq,       // <=
    Gt,         // >
    GtEq,       // >=
    AndAnd,     // &&
    OrOr,       // ||
    Bang,       // !

    // Delimiters & Separators
    Colon,      // :
    PathSep,    // ::
    Arrow,      // ->
    FatArrow,   // =>
    Question,   // ?
    DotDot,     // ..
    DotDotEq,   // ..=
    LParen,     // (
    RParen,     // )
    LBracket,   // [
    RBracket,   // ]
    LBrace,     // {
    RBrace,     // }
    Comma,      // ,
    Semicolon,  // ;
}

/// Abstract Syntax Tree node placeholders for GHL expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Token),
    Ident(String),
    Binary {
        op: Token,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Pipe {
        expr: Box<Expr>,
        target: Box<Expr>,
    },
    Formula {
        response: Box<Expr>,
        terms: Vec<Expr>,
    },
    Block(Vec<Expr>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let tok_na = Token::NA;
        let tok_na_reason = Token::NAReason("NoResponse".into());
        let tok_pipe = Token::Pipe;
        let tok_tilde = Token::Tilde;

        assert_eq!(tok_na, Token::NA);
        assert_eq!(tok_na_reason, Token::NAReason("NoResponse".into()));
        assert_eq!(tok_pipe, Token::Pipe);
        assert_eq!(tok_tilde, Token::Tilde);
    }
}
