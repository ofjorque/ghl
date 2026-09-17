//! Vector and window native functions (cumulative, ranking, broadcasting).
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use std::sync::Arc;
use ghl_diagnostics::Diagnostic;
use polars_core::prelude::*;
use rayon::prelude::*;
use crate::value::Value;
use crate::vector_data::{NumericView, VectorData};


// =========================================================================
// Vector / window helpers
// =========================================================================

#[allow(dead_code)]
pub(crate) fn as_vector(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::Vector(items) => Some(items),
        _ => None,
    }
}

/// Like `as_vector`, but returns the `VectorData` itself instead of forcing it through
/// `Deref` -- needed by fast paths that must check `null_count()`/`as_f64_view()` *before*
/// paying for a full `Vec<Value>` materialization.
pub(crate) fn as_vector_data(v: &Value) -> Option<&VectorData> {
    match v {
        Value::Vector(vd) => Some(vd),
        _ => None,
    }
}

pub(crate) fn cumulative(items: &[Value], init: f64, combine: impl Fn(f64, f64) -> f64) -> Vec<Value> {
    let mut acc = init;
    let mut out = Vec::with_capacity(items.len());
    let mut poisoned: Option<Option<String>> = None;
    for it in items.iter() {
        if let Some(reason) = &poisoned {
            out.push(Value::NA(reason.clone()));
            continue;
        }
        if let Value::NA(r) = it {
            poisoned = Some(r.clone());
            out.push(Value::NA(r.clone()));
            continue;
        }
        acc = combine(acc, it.as_f64().unwrap_or(0.0));
        out.push(Value::F64(acc));
    }
    out
}

/// `cumsum`/`cumprod`/`cummax`/`cummin`'s dispatcher. Fast path only when there's no NA
/// *anywhere* in the input: a cumulative scan has a strict sequential dependency (each
/// value depends on the one before it), so this deliberately stays single-threaded --
/// parallelizing a scan for real needs a dedicated parallel-scan algorithm, not just
/// `rayon::par_iter()`, and isn't attempted here. With any NA present, falls back
/// unchanged to `cumulative()`'s existing "one NA poisons everything after it" loop --
/// that boxed path already handles NA-mixed input correctly, no change needed there.
pub(crate) fn cumulative_fast(vd: &VectorData, init: f64, combine: impl Fn(f64, f64) -> f64) -> Value {
    if vd.null_count() == 0 {
        if let Ok(view) = vd.as_f64_view() {
            let mut acc = init;
            let data: Vec<f64> = view
                .as_slice()
                .iter()
                .map(|&x| {
                    acc = combine(acc, x);
                    acc
                })
                .collect();
            return Value::Vector(VectorData::from_f64(data));
        }
    }
    Value::Vector(VectorData::from_values(cumulative(vd, init, combine)))
}

pub(crate) fn native_cumsum(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cumsum()` requires a Vector argument")
    })?;
    Ok(cumulative_fast(vd, 0.0, |a, b| a + b))
}

pub(crate) fn native_cumprod(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cumprod()` requires a Vector argument")
    })?;
    Ok(cumulative_fast(vd, 1.0, |a, b| a * b))
}

pub(crate) fn native_cummax(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cummax()` requires a Vector argument")
    })?;
    Ok(cumulative_fast(vd, f64::NEG_INFINITY, f64::max))
}

pub(crate) fn native_cummin(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cummin()` requires a Vector argument")
    })?;
    Ok(cumulative_fast(vd, f64::INFINITY, f64::min))
}

pub(crate) fn native_lag(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`lag()` requires a Vector argument")
    })?;
    let len = vd.len();
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(1).max(0) as usize;
    if n == 0 {
        return Ok(Value::Vector(vd.clone()));
    }
    let shifted_col = vd.column().shift(n as i64);
    let shifted_reasons = vd.na_reasons().shift(n as i64, len);
    Ok(Value::Vector(VectorData::from_column_and_reasons(shifted_col, Arc::new(shifted_reasons))))
}

pub(crate) fn native_lead(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`lead()` requires a Vector argument")
    })?;
    let len = vd.len();
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(1).max(0) as usize;
    if n == 0 {
        return Ok(Value::Vector(vd.clone()));
    }
    let shifted_col = vd.column().shift(-(n as i64));
    let shifted_reasons = vd.na_reasons().shift(-(n as i64), len);
    Ok(Value::Vector(VectorData::from_column_and_reasons(shifted_col, Arc::new(shifted_reasons))))
}

pub(crate) fn broadcast_get(v: &Value, i: usize) -> Value {
    match v {
        Value::Vector(vd) => vd.value_at(i).unwrap_or(Value::NA(None)),
        scalar => scalar.clone(),
    }
}

enum NumericSource<'a> {
    Scalar(f64),
    Slice(&'a [f64]),
    Owned(Vec<f64>),
}

impl<'a> NumericSource<'a> {
    #[inline(always)]
    fn get(&self, i: usize) -> f64 {
        match self {
            NumericSource::Scalar(s) => *s,
            NumericSource::Slice(sl) => sl[i],
            NumericSource::Owned(v) => v[i],
        }
    }
}

fn as_numeric_source(v: &Value, expected_len: usize) -> Option<NumericSource<'_>> {
    match v {
        Value::F64(n) => Some(NumericSource::Scalar(*n)),
        Value::I64(n) => Some(NumericSource::Scalar(*n as f64)),
        Value::Vector(vd) => {
            if vd.len() == expected_len && vd.null_count() == 0 {
                match vd.as_f64_view() {
                    Ok(NumericView::Borrowed(s)) => Some(NumericSource::Slice(s)),
                    Ok(NumericView::Owned(v)) => Some(NumericSource::Owned(v)),
                    Err(_) => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn native_if_else(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let cond = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;
    let yes = args.get(1).ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;
    let no = args.get(2).ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;

    match cond {
        Value::Bool(b) => Ok(if *b { yes.clone() } else { no.clone() }),
        Value::NA(r) => Ok(Value::NA(r.clone())),
        Value::Vector(cond_data) => {
            let len = cond_data.len();
            // Fast-path: numeric yes/no, cond has no NAs and is boolean
            if cond_data.null_count() == 0 {
                if let (Some(yes_src), Some(no_src)) = (as_numeric_source(yes, len), as_numeric_source(no, len)) {
                    if let Ok(bool_ca) = cond_data.column().bool() {
                        let data: Vec<f64> = if len >= crate::eval::PARALLEL_THRESHOLD {
                            (0..len).into_par_iter().map(|i| {
                                if bool_ca.get(i).unwrap_or(false) {
                                    yes_src.get(i)
                                } else {
                                    no_src.get(i)
                                }
                            }).collect()
                        } else {
                            (0..len).map(|i| {
                                if bool_ca.get(i).unwrap_or(false) {
                                    yes_src.get(i)
                                } else {
                                    no_src.get(i)
                                }
                            }).collect()
                        };
                        return Ok(Value::Vector(VectorData::from_f64(data)));
                    }
                }
            }

            // General fallback: preserves arbitrary types and Kleene NA logic without full materialization
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                out.push(match cond_data.value_at(i) {
                    Some(Value::Bool(true)) => broadcast_get(yes, i),
                    Some(Value::Bool(false)) => broadcast_get(no, i),
                    Some(Value::NA(r)) => Value::NA(r),
                    _ => Value::NA(None),
                });
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("`if_else()` condition must be Bool or Vector[Bool], found `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn native_between(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`between()` requires 3 arguments"))?;
    let lo = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`between()` second argument (lo) must be numeric")
    })?;
    let hi = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`between()` third argument (hi) must be numeric")
    })?;

    fn check(x: &Value, lo: f64, hi: f64) -> Value {
        match x {
            Value::NA(r) => Value::NA(r.clone()),
            other => match other.as_f64() {
                Some(n) => Value::Bool(n >= lo && n <= hi),
                None => Value::NA(Some("NotNumeric".into())),
            },
        }
    }

    match v {
        // Fast path: no NA in the input, and the comparison itself can't produce a NaN
        // (unlike map_numeric_fn's transforms) -- so the Bool output is unconditionally
        // safe to build via `from_bool`, no `Option` needed.
        Value::Vector(vd) if vd.null_count() == 0 => {
            if let Ok(view) = vd.as_f64_view() {
                let base = view.as_slice();
                let data: Vec<bool> = if base.len() >= crate::eval::PARALLEL_THRESHOLD {
                    base.par_iter().map(|&x| x >= lo && x <= hi).collect()
                } else {
                    base.iter().map(|&x| x >= lo && x <= hi).collect()
                };
                Ok(Value::Vector(VectorData::from_bool(data)))
            } else {
                Ok(Value::Vector(VectorData::from_values(vd.iter().map(|it| check(it, lo, hi)).collect())))
            }
        }
        Value::Vector(items) => Ok(Value::Vector(VectorData::from_values(items.iter().map(|it| check(it, lo, hi)).collect()))),
        other => Ok(check(other, lo, hi)),
    }
}

pub(crate) fn sort_vector(args: Vec<Value>, desc: bool, fn_name: &str) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", format!("`{}()` requires a Vector argument", fn_name))
    })?;

    // Fast path: no NA, and the Vector is numeric -- get the sort order from a cheap
    // `&[f64]` read, then reorder the *original* Column via `.take()` (the same native
    // gather `io.rs::take_rows` uses for `sample_n`) instead of reconstructing from
    // floats. This is what preserves dtype (`Int64` stays `Int64`) -- `sort_asc`/
    // `sort_desc` return the *original values*, just reordered, so rebuilding from
    // `as_f64_view()`'s f64s would have silently turned every sorted Int64 vector into
    // Float64.
    if vd.null_count() == 0 {
        if let Ok(view) = vd.as_f64_view() {
            let base = view.as_slice();
            let n = base.len();
            let mut idx: Vec<IdxSize> = (0..n as IdxSize).collect();
            let cmp = |&a: &IdxSize, &b: &IdxSize| {
                let ord = base[a as usize].total_cmp(&base[b as usize]);
                if desc { ord.reverse() } else { ord }
            };
            if n >= crate::eval::PARALLEL_THRESHOLD_SORT {
                idx.par_sort_unstable_by(cmp);
            } else {
                idx.sort_unstable_by(cmp);
            }
            let idx_ca = IdxCa::from_vec(PlSmallStr::EMPTY, idx);
            let new_col = vd.column().take(&idx_ca).map_err(|e| {
                Diagnostic::compute_error("C0210", format!("internal error reordering `{}()`'s result: {e}", fn_name))
            })?;
            return Ok(Value::Vector(VectorData::from_column_no_na(new_col)));
        }
    }

    let mut sorted: Vec<Value> = vd.iter().cloned().collect();
    sorted.sort_by(|a, b| {
        let ord = crate::io::compare_values(Some(a), Some(b));
        if desc { ord.reverse() } else { ord }
    });
    Ok(Value::Vector(VectorData::from_values(sorted)))
}

pub(crate) fn native_sort_asc(args: Vec<Value>) -> Result<Value, Diagnostic> {
    sort_vector(args, false, "sort_asc")
}

pub(crate) fn native_sort_desc(args: Vec<Value>) -> Result<Value, Diagnostic> {
    sort_vector(args, true, "sort_desc")
}

/// Integer rank with ties resolved by averaging (matches R's default `rank()`). Output is
/// always `F64` regardless of the input's dtype (it's a rank, not the original values),
/// so there's no dtype-preservation concern here like `sort_vector`'s -- only the *read*
/// side needs to go fast: `as_f64_view()` + `total_cmp` when the input is NA-free and
/// numeric, instead of `compare_values` over a fully materialized `Vec<Value>`.
pub(crate) fn native_rank(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`rank()` requires a Vector argument")
    })?;

    if vd.null_count() == 0 {
        if let Ok(view) = vd.as_f64_view() {
            let base = view.as_slice();
            let n = base.len();
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by(|&a, &b| base[a].total_cmp(&base[b]));
            let mut ranks = vec![0.0; n];
            let mut i = 0;
            while i < n {
                let mut j = i;
                while j + 1 < n && base[order[j + 1]] == base[order[i]] {
                    j += 1;
                }
                let avg_rank = ((i + j) as f64 / 2.0) + 1.0;
                for slot in order.iter().take(j + 1).skip(i) {
                    ranks[*slot] = avg_rank;
                }
                i = j + 1;
            }
            return Ok(Value::Vector(VectorData::from_f64(ranks)));
        }
    }

    let items: &Vec<Value> = vd;
    let n = items.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| crate::io::compare_values(items.get(a), items.get(b)));

    let mut ranks = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n
            && crate::io::compare_values(items.get(order[j + 1]), items.get(order[i])) == std::cmp::Ordering::Equal
        {
            j += 1;
        }
        let avg_rank = ((i + j) as f64 / 2.0) + 1.0;
        for slot in order.iter().take(j + 1).skip(i) {
            ranks[*slot] = avg_rank;
        }
        i = j + 1;
    }
    Ok(Value::Vector(VectorData::from_f64(ranks)))
}
