//! Flat, typed backing for `Value::Vector` (TODO.md Fase 3, Track 2, Punto 1).
//!
//! `Value::Vector` used to store `Vec<Value>` — one boxed enum per element, exactly the
//! same "boxing" problem `Value::DataFrame` had before its Fase 0/1 migration to polars.
//! This reuses that fix: a single polars `Column` (contiguous, typed, Arrow-backed)
//! plus the same `NaReasonTable` side-channel design for `NA:reason` (keyed with one
//! fixed internal column name, since a `Vector` is conceptually "one column").
//!
//! To land this without rewriting the ~35 existing native functions that pattern-match
//! `Value::Vector(items)` and use it as a `Vec<Value>` in one pass, `VectorData` derefs to
//! `Vec<Value>` (lazily materialized on first use, cached after). Every existing consumer
//! keeps compiling and behaving identically — they pay the same boxing cost they always
//! did, no more, no less. Only *construction* sites (building a `Value::Vector` from a
//! `Vec<Value>`) need a one-line change to go through `VectorData::from_values`, and new
//! numeric fast-path code (Punto 2/3: SIMD reductions, fused `map`) can construct
//! `VectorData` directly from a flat `Vec<f64>` via `VectorData::from_f64` with zero
//! boxing at all — that new path never materializing `Vec<Value>` unless something
//! (a legacy consumer, `print`, ...) actually forces it is the entire point of this change.

use std::ops::Deref;
use std::sync::{Arc, OnceLock};
use ghl_diagnostics::Diagnostic;
use polars_core::prelude::*;

use crate::na_reasons::NaReasonTable;
use crate::polars_bridge;
use crate::value::Value;

/// The fixed internal "column name" `VectorData` uses when reusing `NaReasonTable`'s
/// (column, row) keying and `polars_bridge`'s column-oriented helpers for a single,
/// unnamed vector. Never seen by GHL scripts.
pub(crate) const VECTOR_COL: &str = "__ghl_vector__";

#[derive(Debug, Clone)]
pub struct VectorData {
    column: Column,
    na_reasons: Arc<NaReasonTable>,
    materialized: Arc<OnceLock<Vec<Value>>>,
}

impl VectorData {
    pub fn len(&self) -> usize {
        if let Some(mat) = self.materialized.get() {
            mat.len()
        } else {
            self.column.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Builds from a boxed `Vec<Value>` — the compatibility path every pre-existing
    /// construction site uses. Reuses the exact dtype-inference/NA-reason logic
    /// `build_dataframe` already uses for DataFrame columns, so a vector literal or a
    /// legacy builtin's `Vec<Value>` result infers its type the same way a DataFrame
    /// column would.
    pub fn from_values(values: Vec<Value>) -> Self {
        let has_complex = values.iter().any(|v| {
            matches!(
                v,
                Value::Struct { .. }
                    | Value::Record(_)
                    | Value::Closure { .. }
                    | Value::Vector(_)
            )
        });
        if has_complex {
            let column = polars_bridge::value_column_to_polars(VECTOR_COL, &[]);
            let cell = OnceLock::new();
            let _ = cell.set(values);
            return VectorData {
                column,
                na_reasons: Arc::new(NaReasonTable::new()),
                materialized: Arc::new(cell),
            };
        }
        let column = polars_bridge::value_column_to_polars(VECTOR_COL, &values);
        let mut na_reasons = NaReasonTable::new();
        for (row, v) in values.iter().enumerate() {
            if let Value::NA(Some(reason)) = v {
                na_reasons.set(VECTOR_COL, row, reason.clone());
            }
        }
        let cell = OnceLock::new();
        let _ = cell.set(values);
        VectorData {
            column,
            na_reasons: Arc::new(na_reasons),
            materialized: Arc::new(cell),
        }
    }

    /// Builds directly from a flat `Vec<f64>` — no NA reasons, no boxing at all. The
    /// fast-path constructor numeric-only vectors use (`random_uniform`, and arithmetic/
    /// `map`/`dot` results once Punto 2/3 land).
    pub fn from_f64(data: Vec<f64>) -> Self {
        let column: Column = Float64Chunked::from_vec(PlSmallStr::from_static(VECTOR_COL), data)
            .into_series()
            .into();
        VectorData {
            column,
            na_reasons: Arc::new(NaReasonTable::new()),
            materialized: Arc::new(OnceLock::new()),
        }
    }

    /// Builds from a flat `Vec<Option<f64>>` — zero `Value` boxing, but (unlike
    /// `from_f64`) can represent nulls directly as real Arrow nulls, no `NaReasonTable`
    /// entry (no reason string -- these are computed NaNs, not something with a
    /// meaningful "why", same convention `map_numeric_fn`'s boxed path already used).
    /// Needed because a numeric transform can produce a NaN result (`pow(-8.0, 0.5)`)
    /// even when every input element was NA-free -- `from_f64` alone can't express that.
    pub fn from_f64_opt(data: Vec<Option<f64>>) -> Self {
        let mut ca: Float64Chunked = data.into_iter().collect();
        ca.rename(PlSmallStr::from_static(VECTOR_COL));
        VectorData {
            column: ca.into_series().into(),
            na_reasons: Arc::new(NaReasonTable::new()),
            materialized: Arc::new(OnceLock::new()),
        }
    }

    /// Builds from a flat `Vec<bool>` — the fast-path constructor for `Vector[Bool]`
    /// results (`between()`) computed straight from a numeric `&[f64]` view, no `Value`
    /// boxing.
    pub fn from_bool(data: Vec<bool>) -> Self {
        let column: Column = BooleanChunked::new(PlSmallStr::from_static(VECTOR_COL), data)
            .into_series()
            .into();
        VectorData {
            column,
            na_reasons: Arc::new(NaReasonTable::new()),
            materialized: Arc::new(OnceLock::new()),
        }
    }

    /// Wraps an already-built `Column` directly, no `Vec<f64>`/`Vec<Value>` involved at
    /// all. Used by `sort_vector`'s dtype-preserving fast path: reordering a `Column` via
    /// `.take(&idx)` (same gather `io.rs::take_rows` uses for `sample_n`) keeps the
    /// original dtype (`Int64` stays `Int64`) -- reconstructing from `as_f64_view()`
    /// instead would have silently coerced every sorted `Int64` vector to `Float64`.
    /// Only valid to call when the caller already knows there are no NAs to carry over
    /// (the na-reason side-channel is left empty here).
    pub(crate) fn from_column_no_na(column: Column) -> Self {
        VectorData {
            column,
            na_reasons: Arc::new(NaReasonTable::new()),
            materialized: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn from_column_and_reasons(column: Column, na_reasons: Arc<NaReasonTable>) -> Self {
        VectorData {
            column,
            na_reasons,
            materialized: Arc::new(OnceLock::new()),
        }
    }

    pub fn slice(&self, offset: usize, length: usize) -> Self {
        let new_col = self.column.slice(offset as i64, length);
        let mut new_reasons = NaReasonTable::new();
        if self.null_count() > 0 {
            for row in 0..length {
                if let Some(r) = self.na_reasons.get(VECTOR_COL, offset + row) {
                    new_reasons.set(VECTOR_COL, row, r);
                }
            }
        }
        VectorData {
            column: new_col,
            na_reasons: Arc::new(new_reasons),
            materialized: Arc::new(OnceLock::new()),
        }
    }


    pub fn column(&self) -> &Column {
        &self.column
    }

    pub fn na_reasons(&self) -> &Arc<NaReasonTable> {
        &self.na_reasons
    }

    /// O(1) — Arrow tracks this in the array metadata, no scan needed.
    pub fn null_count(&self) -> usize {
        self.column.null_count()
    }

    /// Reads a single cell without materializing the whole vector (`first()`/`last()`
    /// used to pay a full `O(n)` `Deref` just to return one element) -- still
    /// reason-aware, same as reading through the fully materialized `Vec<Value>` would be.
    /// Deliberately *not* named `get` -- `VectorData` derefs to `Vec<Value>`, whose own
    /// `.get()` returns `Option<&Value>` and is already relied on elsewhere (`rank()`,
    /// `if_else()`'s `broadcast_get`); an inherent method of the same name would have
    /// silently shadowed that Deref-provided one at every existing call site instead of
    /// just the ones meant to use this.
    pub fn value_at(&self, index: usize) -> Option<Value> {
        let av = self.column.get(index).ok()?;
        Some(polars_bridge::any_value_to_value(&av, VECTOR_COL, index, &self.na_reasons))
    }

    /// If this vector has any null, returns `Some(Value::NA(reason))` for the *first* one
    /// found (by row position), preserving that cell's recorded `NA:reason` if it has one.
    /// Returns `None` when there are no nulls at all. `null_count()` (O(1)) is checked
    /// first, so the boolean scan below (still no per-cell `Value` boxing -- it walks a
    /// `BooleanChunked`, not `Vec<Value>`) only runs when it's actually needed. This is
    /// what lets `mean()`/`sum()`/`dot()`/etc. keep propagating a *specific* NA reason
    /// (TODO.md Fase 3, Punto 2 -- an explicit decision, not an oversight: unlike
    /// `summarize()`'s per-group aggregation, a single `Vector` has no "which group's
    /// reason wins" ambiguity to hide behind).
    pub fn first_na(&self) -> Option<Value> {
        if self.column.null_count() == 0 {
            return None;
        }
        let mask = self.column.is_null();
        let row = (0..mask.len()).find(|&i| mask.get(i) == Some(true))?;
        let reason = self.na_reasons.get(VECTOR_COL, row).map(|s| s.to_string());
        Some(Value::NA(reason))
    }

    /// A flat `&[f64]` view for numeric fast paths (`dot()`, and future SIMD reductions),
    /// avoiding `Vec<Value>` boxing entirely either way. Callers must have already checked
    /// `null_count() == 0` (`first_na()` returns `None`) -- this does not itself handle
    /// NA propagation, by design, so the Kleene-with-reason logic stays in one place at
    /// the call site rather than duplicated here.
    ///
    /// Fast path (common case: already `Float64`, one Arrow chunk, no nulls): zero-copy,
    /// borrows straight from the underlying buffer. Slower fallback (e.g. an `Int64`
    /// vector, or one with more than one chunk): casts to `Float64` and/or rechunks --
    /// real work, but still far cheaper than boxing through `Vec<Value>`, since it never
    /// allocates a `Value` enum per cell.
    pub fn as_f64_view(&self) -> Result<NumericView<'_>, Diagnostic> {
        column_as_f64_view(&self.column)
    }
}

/// Same logic as `VectorData::as_f64_view`, factored out as a free function so any
/// `Column` can use this fast path -- not just one already wrapped in a `VectorData`.
/// `Blueprint::bake` (`neko.rs`) reuses this directly on `DataFrame` columns, the same
/// way this migration already solved the identical boxed-`Value` problem for `Vector`.
pub(crate) fn column_as_f64_view(column: &Column) -> Result<NumericView<'_>, Diagnostic> {
    if column.dtype() == &DataType::Float64 {
        if let Ok(ca) = column.f64() {
            if let Ok(slice) = ca.cont_slice() {
                return Ok(NumericView::Borrowed(slice));
            }
        }
    }
    let casted = column.cast(&DataType::Float64).map_err(|e| {
        Diagnostic::compute_error("C0202", format!("expected a numeric column: {e}"))
    })?;
    let ca = casted.f64().map_err(|e| {
        Diagnostic::compute_error("C0210", format!("internal error extracting f64 data: {e}"))
    })?;
    let rechunked = ca.rechunk();
    let slice = rechunked.cont_slice().map_err(|e| {
        Diagnostic::compute_error("C0210", format!("internal error: expected no nulls after null_count() check: {e}"))
    })?;
    Ok(NumericView::Owned(slice.to_vec()))
}

/// See `VectorData::as_f64_view`.
pub enum NumericView<'a> {
    Borrowed(&'a [f64]),
    Owned(Vec<f64>),
}

impl NumericView<'_> {
    pub fn as_slice(&self) -> &[f64] {
        match self {
            NumericView::Borrowed(s) => s,
            NumericView::Owned(v) => v,
        }
    }
}

impl std::ops::Deref for VectorData {
    type Target = Vec<Value>;

    fn deref(&self) -> &Vec<Value> {
        self.materialized.get_or_init(|| {
            (0..self.column.len())
                .map(|i| {
                    let av = self.column.get(i).expect("VectorData row index is always in bounds");
                    polars_bridge::any_value_to_value(&av, VECTOR_COL, i, &self.na_reasons)
                })
                .collect()
        })
    }
}

impl PartialEq for VectorData {
    fn eq(&self, other: &Self) -> bool {
        // Compares materialized values (element-wise, reason-aware) rather than the raw
        // `Column`s, keeping `Value::Vector(a) == Value::Vector(b)`'s existing semantics
        // unchanged for the many tests/callers already relying on them.
        self.deref() == other.deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_element_string_vector_round_trips_without_quote_corruption() {
        // Regression test: polars represents a length-1 Column as a `ScalarColumn`, whose
        // `.get()` returns `AnyValue::StringOwned` rather than the borrowed `String(&str)`
        // a multi-row column returns. `any_value_to_plain_value` didn't handle that owned
        // variant, silently falling into its generic `Display`-based fallback -- which
        // quotes string values -- turning a one-element `["id"]` vector into `"\"id\""`.
        // This broke every single-element string vector, most visibly `select`/`drop`
        // (`df |> drop(["id"])` silently dropped nothing because the column name it looked
        // for was `"id"` with literal embedded quotes, matching no real column).
        let vd = VectorData::from_values(vec![Value::String("id".to_string())]);
        let materialized: &Vec<Value> = &vd;
        assert_eq!(materialized[0], Value::String("id".to_string()));
    }

    #[test]
    fn from_f64_round_trips_without_boxing_na() {
        let vd = VectorData::from_f64(vec![1.0, 2.0, 3.0]);
        assert_eq!(vd.len(), 3);
        let materialized: &Vec<Value> = &vd;
        assert_eq!(*materialized, vec![Value::F64(1.0), Value::F64(2.0), Value::F64(3.0)]);
    }
}
