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
use polars_core::prelude::*;

use crate::na_reasons::NaReasonTable;
use crate::polars_bridge;
use crate::value::Value;

/// The fixed internal "column name" `VectorData` uses when reusing `NaReasonTable`'s
/// (column, row) keying and `polars_bridge`'s column-oriented helpers for a single,
/// unnamed vector. Never seen by GHL scripts.
const VECTOR_COL: &str = "__ghl_vector__";

#[derive(Debug, Clone)]
pub struct VectorData {
    column: Column,
    na_reasons: Arc<NaReasonTable>,
    materialized: Arc<OnceLock<Vec<Value>>>,
}

impl VectorData {
    pub fn len(&self) -> usize {
        self.column.len()
    }

    pub fn is_empty(&self) -> bool {
        self.column.is_empty()
    }

    /// Builds from a boxed `Vec<Value>` — the compatibility path every pre-existing
    /// construction site uses. Reuses the exact dtype-inference/NA-reason logic
    /// `build_dataframe` already uses for DataFrame columns, so a vector literal or a
    /// legacy builtin's `Vec<Value>` result infers its type the same way a DataFrame
    /// column would.
    pub fn from_values(values: Vec<Value>) -> Self {
        let column = polars_bridge::value_column_to_polars(VECTOR_COL, &values);
        let mut na_reasons = NaReasonTable::new();
        for (row, v) in values.iter().enumerate() {
            if let Value::NA(Some(reason)) = v {
                na_reasons.set(VECTOR_COL, row, reason.clone());
            }
        }
        VectorData {
            column,
            na_reasons: Arc::new(na_reasons),
            materialized: Arc::new(OnceLock::new()),
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

    pub fn column(&self) -> &Column {
        &self.column
    }

    pub fn na_reasons(&self) -> &Arc<NaReasonTable> {
        &self.na_reasons
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
