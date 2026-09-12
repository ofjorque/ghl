//! Abstract Syntax Tree (AST) definitions for GHL.

pub type Span = std::ops::Range<usize>;

/// Verbs whose call arguments are evaluated/checked in "column context": a bare
/// identifier that isn't otherwise bound resolves to a column reference (`ColRef`)
/// instead of erroring. Shared between the interpreter (`ghl-runtime::eval`) and the
/// type checker (`ghl-types::checker`) so the two never drift out of sync.
pub const COLUMN_CONTEXT_VERBS: &[&str] = &[
    "filter", "select", "arrange", "desc", "group_by", "summarize",
    "mutate", "drop", "distinct", "pull", "count", "fill_na",
    "slice_min", "slice_max", "inner_join", "left_join", "na_reasons",
];

pub fn is_column_context_verb(name: &str) -> bool {
    COLUMN_CONTEXT_VERBS.contains(&name)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    NA(Option<String>), // None = plain NA, Some("NoResponse") = semantic NA
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnnotation {
    Simple(String),
    Generic(String, Vec<TypeAnnotation>),
}

impl std::fmt::Display for TypeAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simple(s) => write!(f, "{}", s),
            Self::Generic(name, args) => {
                write!(f, "{}[", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, "]")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnParam {
    pub name: String,
    pub ty: Option<TypeAnnotation>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    // Matrix ops
    MatSolve,
    DotMul,
    DotAdd,
    DotSub,
    DotDiv,
    // Comparison
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    // Logic
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard, // _
    Lit(Literal),
    Ident(String),
    NA,
    NAReason(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Lit(Literal),
    Ident(String),
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    UnaryNot(Box<Expr>),
    UnaryNeg(Box<Expr>),
    Pipe {
        expr: Box<Expr>,
        target: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Block {
        stmts: Vec<Stmt>,
        expr: Option<Box<Expr>>,
    },
    Formula {
        response: Box<Expr>,
        terms: Vec<Expr>,
    },
    If {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
    },
    /// `for var in start..end { body }` — integer range loop (exclusive end).
    /// Desugars to a native Rust loop at eval time: zero heap allocations per iteration.
    For {
        var: String,
        start: Box<Expr>,
        end: Box<Expr>,
        body: Box<Expr>,
    },
    DataFrameLit(Vec<(String, Expr)>),
    MatrixLit {
        rows: Vec<Vec<Expr>>,
    },
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },
    VectorLit(Vec<Expr>),
    Placeholder, // _
    /// `name = expr` inside a call's argument list, e.g. `summarize(n = count())`.
    NamedArg {
        name: String,
        value: Box<Expr>,
    },
    /// Qualified path, e.g. `std::math::sqrt` or `std::linalg::eye`
    Path(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseItem {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UseKind {
    Items(Vec<UseItem>),
    Glob,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseStmt {
    pub path: Vec<String>,
    pub kind: UseKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let {
        name: String,
        is_mut: bool,
        ty: Option<TypeAnnotation>,
        init: Expr,
    },
    Fn {
        name: String,
        params: Vec<FnParam>,
        ret_ty: Option<TypeAnnotation>,
        body: Expr,
    },
    Expr(Expr),
    Return(Option<Expr>),
    /// `name = value;` -- reassigns an existing `let mut` binding in place
    /// (`RuntimeEnv::assign`), not a new declaration.
    Assign {
        name: String,
        value: Expr,
    },
    Use(UseStmt),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

