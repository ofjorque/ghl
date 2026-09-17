//! Aggregation, grouping, sorting, and column/row-selection DataFrame verbs.
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use ghl_diagnostics::Diagnostic;
use crate::value::Value;
use crate::vector_data::VectorData;
use crate::native_core::*;


// =========================================================================
// Extra aggregation functions (dual behavior: real compute over a Vector,
// deferred `AggSpec` over a `ColRef` for use inside `summarize()`)
// =========================================================================

pub(crate) fn native_first(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`first()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "first".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => Ok(vd.value_at(0).unwrap_or(Value::NA(None))),
        other => Err(Diagnostic::compute_error("C0202", format!("`first()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_last(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`last()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "last".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => Ok(vd.value_at(vd.len().saturating_sub(1)).unwrap_or(Value::NA(None))),
        other => Err(Diagnostic::compute_error("C0202", format!("`last()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_median(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`median()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "median".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::NA(None));
            }
            vector_native_reduce(vd, "median")
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`median()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_n_distinct(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`n_distinct()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "n_distinct".into(), col: Some(name.clone()) }),
        // Delegates straight to polars' own `Column::n_unique()` instead of
        // `format!("{:?}", it)`-ing every element into a `HashSet<String>` -- a real
        // correctness fix, not just a speedup: two NAs with different reasons
        // (`NA(Some("EmptyVector"))` vs `NA(Some("NaN"))`) used to Debug-format
        // differently and count as *two* distinct values, when semantically there's just
        // one kind of "missing" here. `n_unique()` counts null as at most one distinct
        // value, matching how every other statistics library treats it.
        Value::Vector(vd) => {
            let n = vd.column().n_unique().map_err(|e| {
                Diagnostic::compute_error("C0210", format!("internal error computing `n_distinct()`: {e}"))
            })?;
            Ok(Value::I64(n as i64))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`n_distinct()` expects a Vector, found `{}`", other.type_name()))),
    }
}

/// `count()` (zero args, inside `summarize()`) — row-count aggregate.
/// `count(df, col)` — standalone frequency-table verb, returns `DataFrame[col, n]`.
pub(crate) fn native_count(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Ok(Value::AggSpec { kind: "count".into(), col: None });
    }
    let df = &args[0];
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`count(df, col)` requires a column name")
    })?;
    crate::io::df_count(df, &col)
}

pub(crate) fn native_coalesce(args: Vec<Value>) -> Result<Value, Diagnostic> {
    for a in &args {
        if !a.is_na() {
            return Ok(a.clone());
        }
    }
    Ok(Value::NA(None))
}

/// Extracts a column name from a `ColRef`/`String` value, used throughout the
/// verbs below wherever a column argument may be bare (`col_ctx`-resolved) or quoted.
pub(crate) fn col_name_of(v: &Value) -> Option<String> {
    match v {
        Value::ColRef(s) | Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

// =========================================================================
// Grouping / summarizing
// =========================================================================

/// `inner_join(left, right, on1, on2, ...)` — hash join, keeping only matching rows.
pub(crate) fn native_inner_join(args: Vec<Value>) -> Result<Value, Diagnostic> {
    native_join(args, "inner_join", crate::io::df_inner_join)
}

/// `left_join(left, right, on1, on2, ...)` — hash join, keeping every row of `left`.
pub(crate) fn native_left_join(args: Vec<Value>) -> Result<Value, Diagnostic> {
    native_join(args, "left_join", crate::io::df_left_join)
}

pub(crate) fn native_join(
    args: Vec<Value>,
    verb: &str,
    join_fn: fn(&Value, &Value, &[String]) -> Result<Value, Diagnostic>,
) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`{verb}()` requires two DataFrames: `{verb}(left, right, on)`"),
        ));
    }
    let left = &args[0];
    let right = &args[1];

    let mut on = Vec::new();
    for arg in &args[2..] {
        match arg {
            Value::Vector(items) => {
                for it in items.iter() {
                    if let Some(name) = col_name_of(it) {
                        on.push(name);
                    }
                }
            }
            other => {
                if let Some(name) = col_name_of(other) {
                    on.push(name);
                }
            }
        }
    }
    if on.is_empty() {
        return Err(Diagnostic::compute_error("C0201", format!("`{verb}()` requires at least one join column")));
    }

    join_fn(left, right, &on)
}

pub(crate) fn native_group_by(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`group_by()` requires a DataFrame as first argument")
    })?;

    let mut keys = Vec::new();
    for arg in &args[1..] {
        match arg {
            Value::Vector(items) => {
                for it in items.iter() {
                    if let Some(name) = col_name_of(it) {
                        keys.push(name);
                    }
                }
            }
            other => {
                if let Some(name) = col_name_of(other) {
                    keys.push(name);
                }
            }
        }
    }
    if keys.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`group_by()` requires at least one grouping column"));
    }

    crate::io::df_group_by(df, &keys)
}

/// `summarize(gdf, n = count(), mean_x = mean(x))` — consumes the `GroupedDataFrame`,
/// returns a plain `DataFrame`. Bare column names inside the aggregation calls resolve
/// to `Value::ColRef` (see `col_ctx` in `eval.rs`), which `mean`/`sum`/etc. turn into
/// a deferred `Value::AggSpec` rather than computing anything (there's no data yet).
pub(crate) fn native_summarize(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let gdf = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`summarize()` requires a GroupedDataFrame as first argument (did you forget `group_by()`?)",
        )
    })?;

    let mut specs: Vec<(String, String, Option<String>)> = Vec::new();
    for arg in &args[1..] {
        match arg {
            Value::NamedArg(name, value) => match value.as_ref() {
                Value::AggSpec { kind, col } => specs.push((name.clone(), kind.clone(), col.clone())),
                other => {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!(
                            "`summarize()`: `{}` must be an aggregation like `mean(x)` or `count()`, found `{}`",
                            name, other.type_name()
                        ),
                    ));
                }
            },
            other => {
                return Err(Diagnostic::compute_error(
                    "C0201",
                    format!("`summarize()` expects named arguments like `n = count()`, found `{}`", other.type_name()),
                ));
            }
        }
    }
    if specs.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`summarize()` requires at least one named aggregation, e.g. `summarize(n = count())`",
        ));
    }

    crate::io::df_summarize(gdf, &specs)
}

pub(crate) fn native_ungroup(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::GroupedDataFrame { frame, na_reasons, .. }) => {
            Ok(Value::DataFrame { frame: frame.clone(), na_reasons: na_reasons.clone() })
        }
        Some(Value::GroupedLazyFrame { plan, na_reasons, .. }) => {
            Ok(Value::LazyFrame { plan: plan.clone(), na_reasons: na_reasons.clone() })
        }
        Some(df @ Value::DataFrame { .. }) => Ok(df.clone()),
        Some(lf @ Value::LazyFrame { .. }) => Ok(lf.clone()),
        Some(other) => Err(Diagnostic::compute_error(
            "C0201",
            format!("`ungroup()` requires a GroupedDataFrame or GroupedLazyFrame, found `{}`", other.type_name()),
        )),
        None => Err(Diagnostic::compute_error("C0201", "`ungroup()` requires an argument")),
    }
}

// =========================================================================
// Sorting: multi-column `arrange()` + `desc()`
// =========================================================================

/// `desc(col)` — marks a column descending inside `arrange()`.
pub(crate) fn native_desc(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let name = args.first().and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`desc()` requires a column name")
    })?;
    Ok(Value::SortSpec { col: name, desc: true })
}

/// `arrange(df, col1, col2, ...)` — variadic, multi-column sort.
/// Each column may be bare/`ColRef`/string (ascending) or `desc(col)` (descending).
pub(crate) fn native_arrange(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`arrange()` requires a DataFrame as first argument")
    })?;

    let mut specs: Vec<(String, bool)> = Vec::new();
    for arg in &args[1..] {
        match arg {
            Value::Vector(items) => {
                for it in items.iter() {
                    match it {
                        Value::SortSpec { col, desc } => specs.push((col.clone(), *desc)),
                        other => match col_name_of(other) {
                            Some(name) => specs.push((name, false)),
                            None => {
                                return Err(Diagnostic::compute_error(
                                    "C0201",
                                    format!("`arrange()` expects column names or `desc(col)`, found `{}`", other.type_name()),
                                ));
                            }
                        },
                    }
                }
            }
            Value::SortSpec { col, desc } => specs.push((col.clone(), *desc)),
            other => match col_name_of(other) {
                Some(name) => specs.push((name, false)),
                None => {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("`arrange()` expects column names or `desc(col)`, found `{}`", other.type_name()),
                    ));
                }
            },
        }
    }
    if specs.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`arrange()` requires at least one column"));
    }

    crate::io::df_arrange(df, &specs)
}

// =========================================================================
// Column / row-selection helpers (pull, fill_na, glimpse, slice family)
// =========================================================================

pub(crate) fn native_pull(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`pull()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`pull()` requires a column name")
    })?;
    crate::io::df_pull(df, &col)
}

/// `na_reason(x)` — the recorded reason for a single NA value, or `NA` if `x` isn't NA
/// or has no reason attached. Mirrors RFC 02 §2.2's `x.na_reason()`.
pub(crate) fn native_na_reason(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`na_reason()` requires 1 argument"))?;
    match v.na_reason() {
        Some(reason) => Ok(Value::String(reason.to_string())),
        None => Ok(Value::NA(None)),
    }
}

/// `na_reasons(df, col)` — the recorded reasons for a whole column, aligned by row.
pub(crate) fn native_na_reasons(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`na_reasons()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`na_reasons()` requires a column name")
    })?;
    crate::io::df_na_reasons(df, &col)
}

/// `is_na(x)` — RFC 02's `x.is_na()` as a callable function, vectorized over a `Vector`
/// the same way the math/string helpers are (`is_na(pull(df, "col"))` -> `Vector[Bool]`).
pub(crate) fn native_is_na(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`is_na()` requires 1 argument"))?;
    match v {
        // `is_na(col(...))` / `is_na(bare_column)` inside filter()'s argument tree:
        // no data to check yet, stay deferred as a predicate the same way `col(x) > 5`
        // does (see Value::IsNaPredicate).
        Value::ColRef(col) => Ok(Value::IsNaPredicate(col.clone())),
        Value::Vector(items) => Ok(Value::Vector(VectorData::from_values(items.iter().map(|it| Value::Bool(it.is_na())).collect()))),
        other => Ok(Value::Bool(other.is_na())),
    }
}

pub(crate) fn native_fill_na(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`fill_na()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`fill_na()` requires a column name")
    })?;
    let default = args.get(2).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`fill_na()` requires a default value")
    })?;
    crate::io::df_fill_na(df, &col, default)
}

pub(crate) fn native_fill_na_all(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`fill_na_all()` requires a DataFrame"))?;
    let default = args.get(1).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`fill_na_all()` requires a default value")
    })?;
    crate::io::df_fill_na_all(df, default)
}

pub(crate) fn native_glimpse(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`glimpse()` requires a DataFrame"))?;
    crate::io::df_glimpse(df)
}

pub(crate) fn native_slice_min(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`slice_min()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`slice_min()` requires a column name")
    })?;
    let n = args.get(2).and_then(|v| v.as_i64()).unwrap_or(1) as usize;
    crate::io::df_slice_min(df, &col, n)
}

pub(crate) fn native_slice_max(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`slice_max()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`slice_max()` requires a column name")
    })?;
    let n = args.get(2).and_then(|v| v.as_i64()).unwrap_or(1) as usize;
    crate::io::df_slice_max(df, &col, n)
}

pub(crate) fn native_sample_n(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`sample_n()` requires a DataFrame"))?;
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(1) as usize;
    crate::io::df_sample_n(df, n)
}

pub(crate) fn native_sample_frac(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`sample_frac()` requires a DataFrame"))?;
    let frac = args.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0);
    crate::io::df_sample_frac(df, frac)
}
