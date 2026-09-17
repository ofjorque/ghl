use std::fmt;
use ghl_diagnostics::{
    CockpitTable, RenderCaps, TableAlignment, TableColumn, Diagnostic,
    AestheticMap, GeomLayer, PlotSpec,
};
use ghl_syntax::ast::{Expr, BinaryOp, FormulaOp};
use ghl_types::ContrastScheme;
use crate::env::RuntimeEnv;
use crate::neko::FittedModel;
use crate::glm::FittedGlm;
use crate::gmm::FittedGmm;

pub type NativeFunction = fn(Vec<Value>) -> Result<Value, Diagnostic>;

/// First-class callable wrapper around Cranelift JIT-compiled native machine code (RFC 13).
#[derive(Clone)]
pub struct JitFunction(pub std::sync::Arc<dyn Fn(Vec<Value>) -> Result<Value, Diagnostic> + Send + Sync>);

impl JitFunction {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(Vec<Value>) -> Result<Value, Diagnostic> + Send + Sync + 'static,
    {
        Self(std::sync::Arc::new(f))
    }
}

impl std::fmt::Debug for JitFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JitFunction({:p})", std::sync::Arc::as_ptr(&self.0))
    }
}

impl std::ops::Deref for JitFunction {
    type Target = dyn Fn(Vec<Value>) -> Result<Value, Diagnostic> + Send + Sync;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// Wrapper around `polars_lazy::frame::LazyFrame` that provides `Debug` and `Deref`.
#[derive(Clone)]
pub struct LazyPlan(pub polars_lazy::frame::LazyFrame);

impl std::fmt::Debug for LazyPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LazyPlan")
    }
}

impl std::ops::Deref for LazyPlan {
    type Target = polars_lazy::frame::LazyFrame;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for LazyPlan {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// First-class runtime values in GHL.
#[derive(Debug, Clone)]
pub enum Value {
    I64(i64),
    F64(f64),
    Bool(bool),
    String(String),
    Unit,
    /// Missing value with optional semantic reason:
    /// - None => generic NA
    /// - Some("SensorDropout") => NA:SensorDropout
    NA(Option<String>),
    /// Flat, typed backing (TODO.md Fase 3, Track 2) — `VectorData` derefs to `Vec<Value>`
    /// (lazily materialized) so existing code that reads it as one keeps compiling and
    /// behaving unchanged; see `vector_data.rs` for why and how.
    Vector(crate::vector_data::VectorData),
    Matrix {
        rows: usize,
        cols: usize,
        data: std::sync::Arc<Vec<f64>>,
    },
    /// Backed by a real `polars_core::frame::DataFrame` (TODO.md Fase 0/1) — `frame`
    /// carries the columnar/typed data, `na_reasons` is the side-channel for GHL's
    /// `NA:reason` semantics that Arrow has no equivalent for (see `na_reasons.rs`).
    /// `na_reasons` is `Arc`-wrapped (TODO.md Fase 1, "Copy-on-Write" for Caso 2.3,
    /// scoped to `DataFrame`/`GroupedDataFrame` rather than the full ARC+CoW memory model
    /// Fase 5 covers for `Vector`/`Matrix`/etc.): every verb here is purely functional
    /// (it borrows its input and returns a brand-new `Value`, never mutates a `Value`
    /// another binding might still hold), so sharing the reasons table via `Arc` and
    /// cloning the `Arc` (not its contents) whenever a verb's output keeps the same
    /// reasons unchanged is always safe — there is no in-place mutation path to guard
    /// against, which is also why this needs no `Arc::make_mut`-style "clone on actual
    /// write" trigger: a "write" here always means constructing a fresh table before
    /// wrapping it, never mutating a shared one. `frame` itself doesn't need the same
    /// treatment: polars' own `Column`/`Series` are already `Arc`-backed internally, so
    /// `DataFrame::clone()` already only clones the small per-column handle vector, not
    /// row data.
    DataFrame {
        frame: polars_core::frame::DataFrame,
        na_reasons: std::sync::Arc<crate::na_reasons::NaReasonTable>,
    },

    /// A deferred execution plan backed by `polars_lazy::frame::LazyFrame` (TODO.md Fase 2).
    /// Query operations (filter, select, mutate, arrange, group_by, summarize, joins)
    /// accumulate in the query graph and benefit from Polars' optimizer (predicate pushdown,
    /// projection pushdown) until materialized with `collect()` or inspected with `explain()`.
    LazyFrame {
        plan: LazyPlan,
        na_reasons: std::sync::Arc<crate::na_reasons::NaReasonTable>,
    },
    /// A partitioned deferred query plan produced by `group_by()` on a `LazyFrame`.
    GroupedLazyFrame {
        plan: LazyPlan,
        na_reasons: std::sync::Arc<crate::na_reasons::NaReasonTable>,
        keys: Vec<String>,
    },
    ColRef(String),
    /// `col(x) > 5`-style single comparison. The leaf of the predicate tree `filter()`
    /// evaluates — see `IsNaPredicate`/`NotPredicate`/`AndPredicate`/`OrPredicate` for
    /// the rest of it (`is_na(x)`, `!`, `&&`, `||` combining predicates).
    ColPredicate {
        col: String,
        op: BinaryOp,
        rhs: Box<Value>,
    },
    /// `is_na(col(...))` used as a predicate rather than a standalone boolean check —
    /// produced by `native_is_na` when given a `ColRef` instead of a real value.
    IsNaPredicate(String),
    /// `!predicate` where `predicate` is itself deferred (a `ColPredicate`/
    /// `IsNaPredicate`/`AndPredicate`/`OrPredicate`) — negating a real `Bool`/`NA` still
    /// goes through the ordinary `UnaryNot` path unchanged.
    NotPredicate(Box<Value>),
    /// `predicate && predicate` — only built when at least one side is itself deferred;
    /// two real `Bool`s still go through ordinary Kleene `&&`.
    AndPredicate(Box<Value>, Box<Value>),
    /// `predicate || predicate` — see `AndPredicate`.
    OrPredicate(Box<Value>, Box<Value>),
    /// Rows partitioned by one or more key columns. Produced by `group_by()`,
    /// consumed by `summarize()`/`ungroup()` — never leaks past either. Grouping is
    /// recomputed from `frame`/`keys` at the point of use rather than stored, since
    /// polars' `GroupBy<'a>` borrows its source frame and can't live in an owned `Value`.
    GroupedDataFrame {
        frame: polars_core::frame::DataFrame,
        na_reasons: std::sync::Arc<crate::na_reasons::NaReasonTable>,
        keys: Vec<String>,
    },
    /// A deferred aggregation, e.g. `mean(x)` where `x` is a bare column reference
    /// rather than real data yet — resolved per-group inside `summarize()`.
    /// `col: None` means a row-count aggregate (`count()`/`n()`).
    AggSpec {
        kind: String,
        col: Option<String>,
    },
    /// A column sort key with direction, produced by a bare column (ascending)
    /// or `desc(col)` (descending) inside `arrange()`.
    SortSpec {
        col: String,
        desc: bool,
    },
    /// Runtime counterpart of `ExprKind::NamedArg` — `name = value` inside a call,
    /// e.g. `summarize(mean_x = mean(x))`.
    NamedArg(String, Box<Value>),
    Factor {
        levels: Vec<String>,
        indices: Vec<usize>,
        ordered: bool,
        contrast: ContrastScheme,
    },
    Formula {
        op: FormulaOp,
        response: String,
        terms: Vec<String>,
    },
    /// `sem_spec { ... }` result: an ordered list of `Formula` equations
    /// (measurement `=~`, covariance `~~`, and/or regression `~`).
    SemSpec(Vec<Value>),
    /// Fitted statistical model under NEKO framework
    ModelFit(Box<FittedModel>),
    /// Fitted GLM (logistic regression via IRLS, TODO.md Fase 6 Caso 3.2) -- a separate
    /// variant from `ModelFit` rather than reusing `FittedModel`, since OLS's
    /// diagnostics (R², F-stat, t-statistics) have no correct analog for logistic
    /// regression (see `glm.rs`'s module doc). `summary`/`tidy`/`glance`/`augment`/
    /// `predict`/`residuals`/`coef`/`vcov` all dispatch on either variant transparently.
    GlmFit(Box<FittedGlm>),
    /// Fitted Gaussian Mixture Model (EM clustering, NEKO framework)
    GmmFit(Box<FittedGmm>),
    /// `qr(m)` result (TODO.md Fase 3) — accessed via `qr_q(result)`/`qr_r(result)`
    /// rather than field syntax, which GHL's grammar doesn't have.
    QrDecomp {
        q: Box<Value>,
        r: Box<Value>,
    },
    /// `svd(m)` result — `svd_u`/`svd_s`/`svd_v(result)`. `s` is a `Vector` of singular
    /// values (nonincreasing), not a diagonal `Matrix` — matches how GHL already models
    /// "one value per component" everywhere else.
    SvdDecomp {
        u: Box<Value>,
        s: Box<Value>,
        v: Box<Value>,
    },
    /// `eigen(m)` result (symmetric matrices only — see `MatrixOps::eigen_symmetric`) —
    /// `eigen_values`/`eigen_vectors(result)`.
    EigenDecomp {
        values: Box<Value>,
        vectors: Box<Value>,
    },
    /// Grammar of Graphics statistical plot
    Plot(Box<PlotSpec>),
    Aesthetic(AestheticMap),
    Geom(GeomLayer),
    Closure {
        params: Vec<String>,
        body: Expr,
        env: RuntimeEnv,
    },
    NativeFn(NativeFunction),
    /// Native machine code function compiled JIT via Cranelift (RFC 13)
    JitFn {
        name: String,
        func: JitFunction,
    },
    /// A native function that needs to call back into the interpreter (TODO.md Fase 3,
    /// Track 2, Punto 3) -- `NativeFunction` is a plain `fn(Vec<Value>) -> ...` pointer
    /// with no way to invoke a `Closure`/`NativeFn` passed as an argument (e.g. `map`'s
    /// second argument). `crate::eval::Interpreter::call_value` is the only thing that
    /// knows how to do that, so a function needing it takes `&mut Interpreter` too.
    NativeFnCtx(fn(&mut crate::eval::Interpreter, Vec<Value>) -> Result<Value, Diagnostic>),
    /// Regional memory arena (RFC 03 §2.2) backed by `bumpalo`
    Arena(std::sync::Arc<std::sync::Mutex<crate::arena::ArenaState>>),
    /// Structural Record / Named Tuple (RFC 01 §3.4)
    Record(std::sync::Arc<std::collections::BTreeMap<String, Value>>),
    /// User-defined Struct instance (RFC 02 §4)
    Struct {
        name: String,
        fields: std::sync::Arc<std::collections::BTreeMap<String, Value>>,
    },
    /// Range expression `start..end` or `start..=end` (RFC 05 §2)
    Range {
        start: i64,
        end: i64,
        inclusive: bool,
    },
}

impl Value {
    /// Convenient constructor for `Value::Matrix` wrapping data in `Arc`.
    pub fn matrix(rows: usize, cols: usize, data: Vec<f64>) -> Self {
        Value::Matrix {
            rows,
            cols,
            data: std::sync::Arc::new(data),
        }
    }

    pub fn is_na(&self) -> bool {
        matches!(self, Value::NA(_))
    }

    pub fn na_reason(&self) -> Option<&str> {
        match self {
            Value::NA(Some(r)) => Some(r.as_str()),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::F64(x) => Some(*x),
            Value::I64(n) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I64(n) => Some(*n),
            Value::F64(x) if x.fract() == 0.0 && *x >= (i64::MIN as f64) && *x <= (i64::MAX as f64) => Some(*x as i64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::I64(_) => "i64",
            Value::F64(_) => "f64",
            Value::Bool(_) => "bool",
            Value::String(_) => "str",
            Value::Unit => "()",
            Value::NA(_) => "NA",
            Value::Vector(_) => "Vector",
            Value::Matrix { .. } => "Matrix",
            Value::DataFrame { .. } => "DataFrame",
            Value::LazyFrame { .. } => "LazyFrame",
            Value::GroupedLazyFrame { .. } => "GroupedLazyFrame",
            Value::ColRef(_) => "ColRef",
            Value::ColPredicate { .. } => "ColPredicate",
            Value::IsNaPredicate(_) => "IsNaPredicate",
            Value::NotPredicate(_) => "NotPredicate",
            Value::AndPredicate(..) => "AndPredicate",
            Value::OrPredicate(..) => "OrPredicate",
            Value::GroupedDataFrame { .. } => "GroupedDataFrame",
            Value::AggSpec { .. } => "AggSpec",
            Value::SortSpec { .. } => "SortSpec",
            Value::NamedArg(..) => "NamedArg",
            Value::Factor { .. } => "Factor",
            Value::Formula { .. } => "Formula",
            Value::SemSpec(_) => "SemSpec",
            Value::ModelFit(_) => "ModelFit",
            Value::GlmFit(_) => "GlmFit",
            Value::GmmFit(_) => "GmmFit",
            Value::QrDecomp { .. } => "QrDecomp",
            Value::SvdDecomp { .. } => "SvdDecomp",
            Value::EigenDecomp { .. } => "EigenDecomp",
            Value::Plot(_) => "Plot",
            Value::Aesthetic(_) => "Aesthetic",
            Value::Geom(_) => "Geom",
            Value::Closure { .. } => "Function",
            Value::NativeFn(_) => "NativeFunction",
            Value::NativeFnCtx(_) => "NativeFunction",
            Value::JitFn { .. } => "JitFunction",
            Value::Arena(_) => "Arena",
            Value::Record(_) => "Record",
            Value::Struct { .. } => "Struct",
            Value::Range { .. } => "Range",
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F64(a), Value::F64(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::NA(a), Value::NA(b)) => a == b,
            (Value::Vector(a), Value::Vector(b)) => a == b,
            (
                Value::Matrix { rows: r1, cols: c1, data: d1 },
                Value::Matrix { rows: r2, cols: c2, data: d2 },
            ) => r1 == r2 && c1 == c2 && d1 == d2,
            (
                Value::DataFrame { frame: f1, na_reasons: n1 },
                Value::DataFrame { frame: f2, na_reasons: n2 },
            ) => f1.columns() == f2.columns() && n1 == n2,
            (Value::ColRef(c1), Value::ColRef(c2)) => c1 == c2,
            (
                Value::ColPredicate { col: c1, op: o1, rhs: r1 },
                Value::ColPredicate { col: c2, op: o2, rhs: r2 },
            ) => c1 == c2 && o1 == o2 && r1 == r2,
            (Value::IsNaPredicate(c1), Value::IsNaPredicate(c2)) => c1 == c2,
            (Value::NotPredicate(a), Value::NotPredicate(b)) => a == b,
            (Value::AndPredicate(a1, b1), Value::AndPredicate(a2, b2)) => a1 == a2 && b1 == b2,
            (Value::OrPredicate(a1, b1), Value::OrPredicate(a2, b2)) => a1 == a2 && b1 == b2,
            (
                Value::Factor { levels: l1, indices: i1, ordered: o1, contrast: k1 },
                Value::Factor { levels: l2, indices: i2, ordered: o2, contrast: k2 },
            ) => l1 == l2 && i1 == i2 && o1 == o2 && k1 == k2,
            (
                Value::Formula { op: op1, response: r1, terms: t1 },
                Value::Formula { op: op2, response: r2, terms: t2 },
            ) => op1 == op2 && r1 == r2 && t1 == t2,
            (Value::SemSpec(a), Value::SemSpec(b)) => a == b,
            (Value::ModelFit(m1), Value::ModelFit(m2)) => m1 == m2,
            (Value::GlmFit(m1), Value::GlmFit(m2)) => m1 == m2,
            (Value::GmmFit(m1), Value::GmmFit(m2)) => m1 == m2,
            (
                Value::QrDecomp { q: q1, r: r1 },
                Value::QrDecomp { q: q2, r: r2 },
            ) => q1 == q2 && r1 == r2,
            (
                Value::SvdDecomp { u: u1, s: s1, v: v1 },
                Value::SvdDecomp { u: u2, s: s2, v: v2 },
            ) => u1 == u2 && s1 == s2 && v1 == v2,
            (
                Value::EigenDecomp { values: a1, vectors: b1 },
                Value::EigenDecomp { values: a2, vectors: b2 },
            ) => a1 == a2 && b1 == b2,
            (Value::Plot(p1), Value::Plot(p2)) => p1 == p2,
            (Value::Aesthetic(a1), Value::Aesthetic(a2)) => a1 == a2,
            (Value::Geom(g1), Value::Geom(g2)) => g1 == g2,
            (
                Value::GroupedDataFrame { frame: f1, na_reasons: n1, keys: k1 },
                Value::GroupedDataFrame { frame: f2, na_reasons: n2, keys: k2 },
            ) => f1.columns() == f2.columns() && n1 == n2 && k1 == k2,
            (
                Value::GroupedLazyFrame { keys: k1, .. },
                Value::GroupedLazyFrame { keys: k2, .. },
            ) => k1 == k2,
            (
                Value::AggSpec { kind: k1, col: c1 },
                Value::AggSpec { kind: k2, col: c2 },
            ) => k1 == k2 && c1 == c2,
            (
                Value::SortSpec { col: c1, desc: d1 },
                Value::SortSpec { col: c2, desc: d2 },
            ) => c1 == c2 && d1 == d2,
            (Value::NamedArg(n1, v1), Value::NamedArg(n2, v2)) => n1 == n2 && v1 == v2,
            (Value::Arena(a1), Value::Arena(a2)) => std::sync::Arc::ptr_eq(a1, a2),
            (Value::Record(a1), Value::Record(a2)) => a1 == a2,
            (
                Value::Struct { name: n1, fields: f1 },
                Value::Struct { name: n2, fields: f2 },
            ) => n1 == n2 && f1 == f2,
            (
                Value::Range { start: s1, end: e1, inclusive: i1 },
                Value::Range { start: s2, end: e2, inclusive: i2 },
            ) => s1 == s2 && e1 == e2 && i1 == i2,
            (
                Value::JitFn { name: n1, func: f1 },
                Value::JitFn { name: n2, func: f2 },
            ) => n1 == n2 && std::sync::Arc::ptr_eq(&f1.0, &f2.0),
            _ => false,
        }
    }
}

impl Value {
    /// Render the value with modern terminal styling, respecting RenderCaps (Unicode vs ASCII, Color vs Plain).
    pub fn render_styled(&self, caps: &RenderCaps) -> String {
        match self {
            Value::I64(n) => {
                if caps.color_enabled {
                    caps.cyan(&n.to_string())
                } else {
                    n.to_string()
                }
            }
            Value::F64(x) => {
                let s = if x.fract() == 0.0 && x.abs() < 1e12 {
                    format!("{:.1}", x)
                } else {
                    format!("{:.4}", x).trim_end_matches('0').trim_end_matches('.').to_string()
                };
                if caps.color_enabled {
                    caps.cyan(&s)
                } else {
                    s
                }
            }
            Value::Bool(b) => {
                if caps.color_enabled {
                    caps.magenta(&b.to_string())
                } else {
                    b.to_string()
                }
            }
            Value::String(s) => {
                let formatted = format!("\"{}\"", s);
                if caps.color_enabled {
                    caps.green(&formatted)
                } else {
                    formatted
                }
            }
            Value::Unit => "()".to_string(),
            Value::NA(None) => {
                if caps.color_enabled {
                    caps.yellow("NA")
                } else {
                    "NA".to_string()
                }
            }
            Value::NA(Some(r)) => {
                let s = format!("NA:{}", r);
                if caps.color_enabled {
                    caps.yellow(&s)
                } else {
                    s
                }
            }
            Value::Vector(items) => {
                let ty = if items.is_empty() {
                    "empty"
                } else {
                    let first_ty = items[0].type_name();
                    let homogeneous = items.iter().all(|it| it.type_name() == first_ty);
                    if homogeneous { first_ty } else { "any" }
                };

                let n = items.len();
                let header = format!("Vector[{}] (n={}): ", ty, n);

                if n <= 10 {
                    let rendered_items: Vec<String> = items.iter().map(|it| it.render_styled(caps)).collect();
                    format!("{}[{}]", header, rendered_items.join(", "))
                } else {
                    let head: Vec<String> = items[..5].iter().map(|it| it.render_styled(caps)).collect();
                    let tail: Vec<String> = items[(n - 3)..].iter().map(|it| it.render_styled(caps)).collect();
                    let omitted = n - 8;
                    let ell = if caps.unicode_enabled { "…" } else { "..." };
                    let ell_styled = caps.dim(&format!("{} ({} omitted) {}", ell, omitted, ell));
                    format!("{}[{}, {}, {}]", header, head.join(", "), ell_styled, tail.join(", "))
                }
            }
            Value::Matrix { rows, cols, data } => {
                if *rows == 0 || *cols == 0 {
                    let mult = if caps.unicode_enabled { "×" } else { "x" };
                    return format!("Matrix[f64] (0 {} 0): []", mult);
                }

                let mult = if caps.unicode_enabled { "×" } else { "x" };
                let mut out = format!("Matrix[f64] ({} {} {}):\n", rows, mult, cols);

                // Format numbers with clean decimal precision
                let mut formatted_cells: Vec<String> = Vec::with_capacity(rows * cols);
                let mut col_max_w: Vec<usize> = vec![0; *cols];

                for r in 0..*rows {
                    for c in 0..*cols {
                        let val = data[r * cols + c];
                        let s = format!("{val:>7.3}");
                        col_max_w[c] = col_max_w[c].max(s.chars().count());
                        formatted_cells.push(s);
                    }
                }

                for r in 0..*rows {
                    let (open_b, close_b) = if caps.unicode_enabled {
                        if *rows == 1 {
                            ("[", "]")
                        } else if r == 0 {
                            ("⎡", "⎤")
                        } else if r == rows - 1 {
                            ("⎣", "⎦")
                        } else {
                            ("⎢", "⎥")
                        }
                    } else {
                        ("[", "]")
                    };

                    out.push_str(" ");
                    out.push_str(&caps.dim(open_b));
                    for c in 0..*cols {
                        let s = &formatted_cells[r * cols + c];
                        let pad = " ".repeat(col_max_w[c].saturating_sub(s.chars().count()));
                        out.push_str("  ");
                        out.push_str(&pad);
                        out.push_str(s);
                    }
                    out.push_str("  ");
                    out.push_str(&caps.dim(close_b));
                    if r + 1 < *rows {
                        out.push('\n');
                    }
                }
                out
            }
            Value::DataFrame { frame, na_reasons } => {
                let columns: Vec<String> = frame.get_column_names().iter().map(|s| s.to_string()).collect();
                let num_rows = frame.height();

                // Reuse the same Value <-> polars extraction the language uses for
                // `pull()`, so display and data access never disagree on a cell's value.
                let col_values: Vec<Vec<Value>> = columns
                    .iter()
                    .map(|c| crate::polars_bridge::pull_column_as_values(frame, na_reasons, c).unwrap_or_default())
                    .collect();

                let mut table = CockpitTable::new();
                for (col, values) in columns.iter().zip(&col_values) {
                    let detected_ty = values.iter().find(|it| !it.is_na()).map(|it| it.type_name()).unwrap_or("any");

                    let align = match detected_ty {
                        "i64" | "f64" => TableAlignment::Right,
                        _ => TableAlignment::Left,
                    };

                    table.add_column(
                        TableColumn::new(col.clone())
                            .with_type(detected_ty)
                            .with_alignment(align),
                    );
                }

                for r in 0..num_rows {
                    let mut row_cells = Vec::with_capacity(columns.len());
                    for values in &col_values {
                        row_cells.push(match values.get(r) {
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::F64(f)) => format!("{f:>7.2}"),
                            Some(Value::I64(n)) => n.to_string(),
                            Some(Value::NA(None)) | None => "NA".to_string(),
                            Some(Value::NA(Some(reason))) => format!("NA:{}", reason),
                            Some(other) => format!("{other}"),
                        });
                    }
                    table.add_row(row_cells);
                }

                table.render(caps)
            }
            Value::ColRef(c) => format!("col(\"{}\")", c),
            Value::ColPredicate { col, op, rhs } => format!("col(\"{}\") {:?} {}", col, op, rhs),
            Value::IsNaPredicate(col) => format!("is_na(col(\"{}\"))", col),
            Value::NotPredicate(inner) => format!("!({})", inner),
            Value::AndPredicate(a, b) => format!("({}) && ({})", a, b),
            Value::OrPredicate(a, b) => format!("({}) || ({})", a, b),
            Value::GroupedDataFrame { frame, keys, .. } => {
                format!("GroupedDataFrame[keys={:?}, n_rows={}]", keys, frame.height())
            }
            Value::LazyFrame { .. } => {
                "LazyFrame [deferred execution plan — call `collect()` to materialize or `explain()` to inspect]".to_string()
            }
            Value::GroupedLazyFrame { keys, .. } => {
                format!("GroupedLazyFrame[keys={:?}, deferred execution plan]", keys)
            }
            Value::AggSpec { kind, col } => match col {
                Some(c) => format!("{}(\"{}\")", kind, c),
                None => format!("{}()", kind),
            },
            Value::SortSpec { col, desc } => {
                if *desc {
                    format!("desc(\"{}\")", col)
                } else {
                    format!("\"{}\"", col)
                }
            }
            Value::NamedArg(name, value) => format!("{} = {}", name, value),
            Value::Factor {
                levels,
                indices,
                ordered,
                contrast,
            } => {
                let kind = if *ordered { "OrderedFactor" } else { "Factor" };
                format!(
                    "{}[n={}, levels={:?}, contrast={:?}]",
                    kind,
                    indices.len(),
                    levels,
                    contrast
                )
            }
            Value::Formula { op, response, terms } => {
                format!("{} {} {}", response, op, terms.join(" + "))
            }
            Value::SemSpec(equations) => {
                let rendered: Vec<String> = equations.iter().map(|e| e.render_styled(caps)).collect();
                format!("sem_spec {{ {} }}", rendered.join("; "))
            }
            Value::ModelFit(m) => m.render_cockpit(caps),
            Value::GlmFit(m) => m.render_cockpit(caps),
            Value::GmmFit(m) => m.render_cockpit(caps),
            Value::QrDecomp { q, r } => {
                format!("QrDecomp {{\nQ =\n{}\nR =\n{}\n}}", q.render_styled(caps), r.render_styled(caps))
            }
            Value::SvdDecomp { u, s, v } => {
                format!(
                    "SvdDecomp {{\nU =\n{}\nS =\n{}\nV =\n{}\n}}",
                    u.render_styled(caps), s.render_styled(caps), v.render_styled(caps)
                )
            }
            Value::EigenDecomp { values, vectors } => {
                format!(
                    "EigenDecomp {{\nvalues =\n{}\nvectors =\n{}\n}}",
                    values.render_styled(caps), vectors.render_styled(caps)
                )
            }
            Value::Plot(p) => p.render(caps),
            Value::Aesthetic(a) => {
                let mut parts = vec![format!("x: \"{}\"", a.x)];
                if let Some(ref y) = a.y {
                    parts.push(format!("y: \"{}\"", y));
                }
                if let Some(ref c) = a.color {
                    parts.push(format!("color: \"{}\"", c));
                }
                format!("aes({})", parts.join(", "))
            }
            Value::Geom(g) => format!("{:?}", g.kind),
            Value::Closure { params, body, .. } => {
                format!("fn({}) {{\n    {}\n}}", params.join(", "), body)
            }
            Value::NativeFn(_) | Value::NativeFnCtx(_) => {
                "<builtin_fn> (type ?<name> or doc(\"<name>\") for details)".to_string()
            }
            Value::JitFn { name, .. } => {
                format!("<jit_fn `{}` (Cranelift x86_64)>", name)
            }
            Value::Arena(a) => {
                let bytes = a.lock().map(|st| st.allocated_bytes()).unwrap_or(0);
                format!("<Arena ({} bytes allocated)>", bytes)
            }
            Value::Record(map) => {
                let mut parts = Vec::new();
                for (k, v) in map.iter() {
                    parts.push(format!("{}: {}", k, v.render_styled(caps)));
                }
                format!("{{ {} }}", parts.join(", "))
            }
            Value::Struct { name, fields } => {
                let mut parts = Vec::new();
                for (k, v) in fields.iter() {
                    parts.push(format!("{}: {}", k, v.render_styled(caps)));
                }
                format!("{} {{ {} }}", name, parts.join(", "))
            }
            Value::Range { start, end, inclusive } => {
                if *inclusive {
                    format!("{}..={}", start, end)
                } else {
                    format!("{}..{}", start, end)
                }
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let caps = RenderCaps::detect();
        write!(f, "{}", self.render_styled(&caps))
    }
}
