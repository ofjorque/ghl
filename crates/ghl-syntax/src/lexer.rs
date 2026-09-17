use logos::Logos;

fn skip_block_comment(lex: &mut logos::Lexer<Token>) -> logos::Skip {
    match lex.remainder().find("*/") {
        Some(pos) => {
            lex.bump(pos + 2);
            logos::Skip
        }
        None => {
            lex.bump(lex.remainder().len());
            logos::Skip
        }
    }
}

#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\n\r\f]+")] // Skip whitespace
#[logos(skip r"//[^\n]*")]     // Skip line comments
pub enum Token {
    #[regex(r"/\*", skip_block_comment)]
    BlockComment,
    // Keywords: Declarations
    #[token("fn")]
    Fn,
    #[token("let")]
    Let,
    #[token("mut")]
    Mut,
    #[token("const")]
    Const,
    #[token("struct")]
    Struct,
    #[token("enum")]
    Enum,
    #[token("trait")]
    Trait,
    #[token("impl")]
    Impl,
    #[token("type")]
    Type,
    #[token("Self")]
    SelfType,
    #[token("self")]
    SelfValue,

    // Keywords: Control Flow
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("match")]
    Match,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("while")]
    While,
    #[token("return")]
    Return,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,

    // Keywords: Modules & Visibility
    #[token("use")]
    Use,
    #[token("pub")]
    Pub,
    #[token("mod")]
    Mod,
    #[token("as")]
    As,
    #[token("extern")]
    Extern,
    #[token("async")]
    Async,
    #[token("await")]
    Await,

    // Special Language Constructors
    #[token("dataframe")]
    DataFrame,
    #[token("mat")]
    Mat,
    #[token("col")]
    Col,
    #[token("sem_spec")]
    SemSpec,

    // Boolean & Missing Values
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("NA")]
    NA,

    // Reasoned NA, e.g. NA:NoResponse, NA:NotObservable
    #[regex(r"NA:[a-zA-Z_][a-zA-Z0-9_]*", |lex| {
        let s = lex.slice();
        s[3..].to_string()
    })]
    NAReason(String),

    // Identifiers
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // Numeric Literals
    #[regex(r"[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?", |lex| lex.slice().to_string())]
    FloatLit(String),

    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    IntLit(i64),

    // String Literals
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let raw = lex.slice();
        let s = &raw[1..raw.len() - 1];
        unescape_string(s)
    })]
    StringLit(String),

    // Statistical & Flow Operators
    #[token("|>")]
    Pipe,
    #[token("~")]
    Tilde,
    /// `=~` — SEM latent measurement operator (lavaan-style), e.g. `f1 =~ x1 + x2`.
    #[token("=~")]
    MeasuredBy,
    /// `~~` — SEM covariance/residual-variance operator, e.g. `x1 ~~ x2`.
    #[token("~~")]
    TildeTilde,
    #[token("_", priority = 3)]
    Underscore,

    // Element-wise Matrix Operations
    #[token(".*")]
    DotStar,
    #[token(".+")]
    DotPlus,
    #[token(".-")]
    DotMinus,
    #[token("./")]
    DotSlash,

    // Linear Algebra & Arithmetic Operators
    #[token("\\")]
    Backslash,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("^")]
    Caret,

    // Comparison & Logic
    #[token("==")]
    EqEq,
    #[token("=")]
    Eq,
    #[token("!=")]
    NotEq,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("&&")]
    AndAnd,
    #[token("&")]
    Amp,
    #[token("||")]
    OrOr,
    #[token("|")]
    VBar,
    #[token("!")]
    Bang,

    // Punctuation & Delimiters
    #[token("::")]
    PathSep,
    #[token(":")]
    Colon,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("?")]
    Question,
    #[token("..=")]
    DotDotEq,
    #[token("..")]
    DotDot,
    #[token(".")]
    Dot,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token("#")]
    Hash,
}

pub type SpannedToken = (Result<Token, ()>, std::ops::Range<usize>);

pub fn lex(source: &str) -> Vec<SpannedToken> {
    Token::lexer(source).spanned().collect()
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Ident(s) => write!(f, "{}", s),
            Token::IntLit(n) => write!(f, "{}", n),
            Token::FloatLit(s) => write!(f, "{}", s),
            Token::StringLit(s) => write!(f, "\"{}\"", s),
            Token::NAReason(r) => write!(f, "NA:{}", r),
            Token::NA => write!(f, "NA"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::Pipe => write!(f, "|>"),
            Token::VBar => write!(f, "|"),
            Token::Tilde => write!(f, "~"),
            other => write!(f, "{:?}", other),
        }
    }
}

pub fn unescape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('0') => out.push('\0'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_basic_pipeline() {
        let src = r#"
            // Example pipeline
            let df = dataframe {
                age: [25, 30, NA, 45],
                income: [50000.0, 75000.50, 60000.0, NA:NoResponse]
            };

            let res = df |> filter(col("age") > 18);
        "#;

        let tokens = lex(src);
        let token_kinds: Vec<Token> = tokens
            .into_iter()
            .map(|(res, _)| res.expect("Valid token"))
            .collect();

        assert!(token_kinds.contains(&Token::Let));
        assert!(token_kinds.contains(&Token::DataFrame));
        assert!(token_kinds.contains(&Token::NA));
        assert!(token_kinds.contains(&Token::NAReason("NoResponse".into())));
        assert!(token_kinds.contains(&Token::Pipe));
        assert!(token_kinds.contains(&Token::Col));
        assert!(token_kinds.contains(&Token::Gt));
        assert!(token_kinds.contains(&Token::IntLit(18)));
    }

    #[test]
    fn test_lex_matrix_operators() {
        let src = r#"let c = A \ b + (X .* Y);"#;
        let tokens: Vec<Token> = lex(src)
            .into_iter()
            .map(|(r, _)| r.unwrap())
            .collect();

        assert_eq!(
            tokens,
            vec![
                Token::Let,
                Token::Ident("c".into()),
                Token::Eq,
                Token::Ident("A".into()),
                Token::Backslash,
                Token::Ident("b".into()),
                Token::Plus,
                Token::LParen,
                Token::Ident("X".into()),
                Token::DotStar,
                Token::Ident("Y".into()),
                Token::RParen,
                Token::Semicolon,
            ]
        );
    }

    #[test]
    fn test_lex_block_comments() {
        let src = r#"
            let a = 10; /* this is a block comment
            spanning multiple lines */ let b = 20;
        "#;
        let tokens: Vec<Token> = lex(src)
            .into_iter()
            .map(|(r, _)| r.unwrap())
            .collect();

        assert_eq!(
            tokens,
            vec![
                Token::Let,
                Token::Ident("a".into()),
                Token::Eq,
                Token::IntLit(10),
                Token::Semicolon,
                Token::Let,
                Token::Ident("b".into()),
                Token::Eq,
                Token::IntLit(20),
                Token::Semicolon,
            ]
        );
    }

    #[test]
    fn test_lex_pipe_and_vbar() {
        let src = r#"|arena| || |> |"#;
        let tokens: Vec<Token> = lex(src)
            .into_iter()
            .map(|(r, _)| r.unwrap())
            .collect();

        assert_eq!(
            tokens,
            vec![
                Token::VBar,
                Token::Ident("arena".into()),
                Token::VBar,
                Token::OrOr,
                Token::Pipe,
                Token::VBar,
            ]
        );
    }
}

