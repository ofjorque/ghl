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
    "pivot_wider", "pivot_longer", "impute", "filter_na_reason",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,    // =
    AddAssign, // +=
    SubAssign, // -=
    MulAssign, // *=
    DivAssign, // /=
}

impl AssignOp {
    pub fn to_binary_op(&self) -> Option<BinaryOp> {
        match self {
            AssignOp::Assign => None,
            AssignOp::AddAssign => Some(BinaryOp::Add),
            AssignOp::SubAssign => Some(BinaryOp::Sub),
            AssignOp::MulAssign => Some(BinaryOp::Mul),
            AssignOp::DivAssign => Some(BinaryOp::Div),
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            AssignOp::Assign => "=",
            AssignOp::AddAssign => "+=",
            AssignOp::SubAssign => "-=",
            AssignOp::MulAssign => "*=",
            AssignOp::DivAssign => "/=",
        }
    }
}

impl std::fmt::Display for AssignOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
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
pub struct ComprehensionClause {
    pub var: String,
    pub iter: Expr,
    pub span: Span,
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
        op: FormulaOp,
        response: Box<Expr>,
        terms: Vec<Expr>,
        parts: Vec<Expr>,
    },
    /// `sem_spec { f1 =~ x1 + x2; f1 ~~ f2; f2 ~ f1; }` — a SEM/CFA model specification:
    /// an ordered list of `Formula` equations (measurement, covariance, and/or
    /// regression), each mandatorily `;`-terminated since there's no last-expression
    /// return-value convention here (unlike a regular `{ ... }` block).
    SemSpec {
        equations: Vec<Expr>,
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
    Comprehension {
        expr: Box<Expr>,
        clauses: Vec<ComprehensionClause>,
        condition: Option<Box<Expr>>,
    },
    Placeholder, // _
    /// `name = expr` inside a call's argument list, e.g. `summarize(n = count())`.
    NamedArg {
        name: String,
        value: Box<Expr>,
    },
    RecordLit(Vec<(String, Expr)>),
    StructLit {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    FieldAccess {
        target: Box<Expr>,
        field: String,
    },
    Index {
        target: Box<Expr>,
        indices: Vec<IndexSpec>,
    },
    /// Qualified path, e.g. `std::math::sqrt` or `std::linalg::eye`
    Path(Vec<String>),
    /// Standalone range expression: `start..end` or `start..=end`
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
}

/// The three tilde-family formula operators. `Regression` (`~`) is the original
/// modeling formula used by `ols`/`fit_logistic`; `Measurement` (`=~`) and
/// `Covariance` (`~~`) are lavaan-style SEM/CFA operators, only meaningful inside
/// a `sem_spec { ... }` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaOp {
    /// `~` — response modeled by predictors, e.g. `y ~ x1 + x2`.
    Regression,
    /// `=~` — latent variable measured by indicators, e.g. `f1 =~ x1 + x2`.
    Measurement,
    /// `~~` — covariance or residual variance, e.g. `x1 ~~ x2`.
    Covariance,
    /// `==` — equality constraint or parameter fixing, e.g. `m1.a == m2.a` or `m3.a == 1.0`.
    Constraint,
}

impl std::fmt::Display for FormulaOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormulaOp::Regression => write!(f, "~"),
            FormulaOp::Measurement => write!(f, "=~"),
            FormulaOp::Covariance => write!(f, "~~"),
            FormulaOp::Constraint => write!(f, "=="),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub ty: TypeAnnotation,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethodSig {
    pub name: String,
    pub params: Vec<FnParam>,
    pub ret_ty: Option<TypeAnnotation>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraitItem {
    Method(TraitMethodSig),
    AssociatedType(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl {
    pub name: String,
    pub items: Vec<TraitItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImplItem {
    Method {
        name: String,
        params: Vec<FnParam>,
        ret_ty: Option<TypeAnnotation>,
        body: Expr,
        span: Span,
    },
    AssociatedType {
        name: String,
        ty: TypeAnnotation,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImplDecl {
    pub trait_name: Option<String>,
    pub target_type: String,
    pub items: Vec<ImplItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndexSpec {
    Expr(Expr),
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
    },
    All, // `:` or `..`
}

impl std::fmt::Display for IndexSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexSpec::Expr(e) => write!(f, "{e}"),
            IndexSpec::Range { start, end, inclusive } => {
                if let Some(s) = start {
                    write!(f, "{s}")?;
                }
                if *inclusive {
                    write!(f, "..=")?;
                } else {
                    write!(f, "..")?;
                }
                if let Some(e) = end {
                    write!(f, "{e}")?;
                }
                Ok(())
            }
            IndexSpec::All => write!(f, ".."),
        }
    }
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
        export_ffi: bool,
    },
    Struct(StructDecl),
    Trait(TraitDecl),
    Impl(ImplDecl),
    Expr(Expr),
    Return(Option<Expr>),
    Break,
    Continue,
    /// `name <op> value;` -- assignment or compound assignment (`+=`, `-=`, `*=`, `/=`)
    Assign {
        name: String,
        op: AssignOp,
        value: Expr,
    },
    /// `target[indices...] <op> value;` -- in-place mutation of a collection / slice / dataframe.
    IndexAssign {
        target: String,
        indices: Vec<IndexSpec>,
        op: AssignOp,
        value: Expr,
    },
    /// `target.field1[.field2...] <op> value;` -- in-place mutation of a struct / record / dataframe field.
    FieldAssign {
        target: String,
        fields: Vec<String>,
        op: AssignOp,
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

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Literal::Int(i) => write!(f, "{i}"),
            Literal::Float(v) => {
                if v.fract() == 0.0 {
                    write!(f, "{v:.1}")
                } else {
                    write!(f, "{v}")
                }
            }
            Literal::String(s) => write!(f, "\"{s}\""),
            Literal::Bool(b) => write!(f, "{b}"),
            Literal::NA(None) => write!(f, "NA"),
            Literal::NA(Some(r)) => write!(f, "NA:{r}"),
        }
    }
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let op_str = match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Pow => "^",
            BinaryOp::MatSolve => "\\",
            BinaryOp::DotMul => ".*",
            BinaryOp::DotAdd => ".+",
            BinaryOp::DotSub => ".-",
            BinaryOp::DotDiv => "./",
            BinaryOp::Eq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::LtEq => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::GtEq => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        };
        write!(f, "{op_str}")
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::fmt::Display for ExprKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprKind::Lit(l) => write!(f, "{l}"),
            ExprKind::Ident(id) => write!(f, "{id}"),
            ExprKind::Binary { op, lhs, rhs } => write!(f, "({lhs} {op} {rhs})"),
            ExprKind::UnaryNot(e) => write!(f, "!{e}"),
            ExprKind::UnaryNeg(e) => write!(f, "-{e}"),
            ExprKind::Pipe { expr, target } => write!(f, "{expr} |> {target}"),
            ExprKind::Call { callee, args } => {
                write!(f, "{callee}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
            ExprKind::Block { stmts, expr } => {
                writeln!(f, "{{")?;
                for stmt in stmts {
                    writeln!(f, "    {stmt}")?;
                }
                if let Some(e) = expr {
                    writeln!(f, "    {e}")?;
                }
                write!(f, "}}")
            }
            ExprKind::Formula { op, response, terms, parts } => {
                write!(f, "{response} {op} ")?;
                if parts.is_empty() {
                    for (i, t) in terms.iter().enumerate() {
                        if i > 0 {
                            write!(f, " + ")?;
                        }
                        write!(f, "{t}")?;
                    }
                } else {
                    for (i, part) in parts.iter().enumerate() {
                        if i > 0 {
                            write!(f, " | ")?;
                        }
                        write!(f, "{part}")?;
                    }
                }
                Ok(())
            }
            ExprKind::SemSpec { equations } => {
                writeln!(f, "sem_spec {{")?;
                for eq in equations {
                    writeln!(f, "    {eq};")?;
                }
                write!(f, "}}")
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                write!(f, "if {cond} {then_branch}")?;
                if let Some(eb) = else_branch {
                    write!(f, " else {eb}")?;
                }
                Ok(())
            }
            ExprKind::Match { expr, arms } => {
                writeln!(f, "match {expr} {{")?;
                for arm in arms {
                    write!(f, "    {} => {},", arm.pattern, arm.body)?;
                }
                write!(f, "}}")
            }
            ExprKind::While { cond, body } => write!(f, "while {cond} {body}"),
            ExprKind::For { var, start, end, body } => write!(f, "for {var} in {start}..{end} {body}"),
            ExprKind::DataFrameLit(cols) => {
                write!(f, "dataframe [")?;
                for (i, (col, expr)) in cols.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{col}: {expr}")?;
                }
                write!(f, "]")
            }
            ExprKind::MatrixLit { rows } => {
                write!(f, "mat [")?;
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ; ")?;
                    }
                    for (j, cell) in row.iter().enumerate() {
                        if j > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{cell}")?;
                    }
                }
                write!(f, "]")
            }
            ExprKind::Lambda { params, body } => {
                write!(f, "\\{} -> {body}", params.join(", "))
            }
            ExprKind::VectorLit(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            ExprKind::Comprehension { expr, clauses, condition } => {
                write!(f, "[{expr}")?;
                for (i, clause) in clauses.iter().enumerate() {
                    if i == 0 {
                        write!(f, " for {} in {}", clause.var, clause.iter)?;
                    } else {
                        write!(f, ", {} in {}", clause.var, clause.iter)?;
                    }
                }
                if let Some(cond) = condition {
                    write!(f, " if {cond}")?;
                }
                write!(f, "]")
            }
            ExprKind::Placeholder => write!(f, "_"),
            ExprKind::NamedArg { name, value } => write!(f, "{name} = {value}"),
            ExprKind::RecordLit(fields) => {
                write!(f, "#[")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k} = {v}")?;
                }
                write!(f, "]")
            }
            ExprKind::StructLit { name, fields } => {
                write!(f, "{name} {{ ")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, " }}")
            }
            ExprKind::FieldAccess { target, field } => write!(f, "{target}.{field}"),
            ExprKind::Index { target, indices } => {
                write!(f, "{target}[")?;
                for (i, idx) in indices.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    match idx {
                        IndexSpec::Expr(e) => write!(f, "{e}")?,
                        IndexSpec::Range { start, end, inclusive } => {
                            if let Some(s) = start { write!(f, "{s}")?; }
                            if *inclusive { write!(f, "..=")?; } else { write!(f, "..")?; }
                            if let Some(e) = end { write!(f, "{e}")?; }
                        }
                        IndexSpec::All => write!(f, "..")?,
                    }
                }
                write!(f, "]")
            }
            ExprKind::Path(parts) => write!(f, "{}", parts.join("::")),
            ExprKind::Range { start, end, inclusive } => {
                if *inclusive {
                    write!(f, "{start}..={end}")
                } else {
                    write!(f, "{start}..{end}")
                }
            }
        }
    }
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pattern::Wildcard => write!(f, "_"),
            Pattern::Lit(l) => write!(f, "{l}"),
            Pattern::Ident(id) => write!(f, "{id}"),
            Pattern::NA => write!(f, "NA"),
            Pattern::NAReason(r) => write!(f, "NA:{r}"),
        }
    }
}

impl std::fmt::Display for Stmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            StmtKind::Let { name, is_mut, ty, init } => {
                let mut_str = if *is_mut { "mut " } else { "" };
                if let Some(t) = ty {
                    write!(f, "let {mut_str}{name}: {t} = {init};")
                } else {
                    write!(f, "let {mut_str}{name} = {init};")
                }
            }
            StmtKind::Fn { name, params, ret_ty, body, .. } => {
                write!(f, "fn {name}(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", p.name)?;
                    if let Some(t) = &p.ty { write!(f, ": {t}")?; }
                }
                write!(f, ")")?;
                if let Some(r) = ret_ty { write!(f, " -> {r}")?; }
                write!(f, " {body}")
            }
            StmtKind::Expr(e) => write!(f, "{e};"),
            StmtKind::Return(e) => {
                if let Some(expr) = e {
                    write!(f, "return {expr};")
                } else {
                    write!(f, "return;")
                }
            }
            StmtKind::Assign { name, op, value } => write!(f, "{name} {op} {value};"),
            StmtKind::FieldAssign { target, fields, op, value } => {
                write!(f, "{target}")?;
                for field in fields {
                    write!(f, ".{field}")?;
                }
                write!(f, " {op} {value};")
            }
            StmtKind::IndexAssign { target, indices, op, value } => {
                write!(f, "{target}[")?;
                for (i, idx) in indices.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{idx}")?;
                }
                write!(f, "] {op} {value};")
            }
            StmtKind::Use(u) => write!(f, "use {};", u.path.join("::")),
            _ => write!(f, "<stmt>"),
        }
    }
}

