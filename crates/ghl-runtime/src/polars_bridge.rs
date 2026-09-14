//! Boundary conversion layer between GHL's dynamic `Value` and
//! `polars_core::frame::DataFrame` (columnar, typed).
//!
//! Two main operations cross this boundary:
//! - [`build_dataframe`]: construction — the `dataframe { col: [...], ... }` literal
//!   and tabular file ingestion construct a native DataFrame from GHL `Vec<Value>` columns.
//! - [`pull_column_as_values`]: extraction — `pull(df, col)` extracts a column back
//!   to a pure GHL `Value::Vector`.
//!
//! All other operations (the ~30 verbs in `io.rs`, and NEKO data representations
//! including `Blueprint::bake`/`FittedModel`/`FittedGlm` in `neko.rs`/`glm.rs`)
//! operate directly on the Polars `DataFrame` without round-tripping through `Value`.
//! `f64_opt_column`/`bool_column`/`string_column` provide the same pattern as
//! [`value_column_to_polars`] for fixed column names, adding derived columns
//! (e.g. `augment()`) without `Vec<Value>` boxing.

use std::sync::Arc;

use ghl_diagnostics::Diagnostic;
use polars_core::prelude::*;

use crate::na_reasons::NaReasonTable;
use crate::value::Value;

/// Constructs a Polars `DataFrame` and its associated NA reason table from GHL columns
/// (`Vec<Value>`, all of equal length). Infers the dtype of each column by widening
/// to the most permissive type present: `String` > `f64` > `i64` > `bool`.
/// Returns reasons wrapped in an `Arc` for Copy-on-Write sharing across `Value::DataFrame`.
pub fn build_dataframe(cols: &[(String, Vec<Value>)]) -> Result<(DataFrame, Arc<NaReasonTable>), Diagnostic> {
    let mut na_reasons = NaReasonTable::new();
    let mut columns = Vec::with_capacity(cols.len());

    for (name, values) in cols {
        for (row, v) in values.iter().enumerate() {
            if let Value::NA(Some(reason)) = v {
                na_reasons.set(name, row, reason.clone());
            }
        }
        columns.push(value_column_to_polars(name, values));
    }

    let frame = if columns.is_empty() {
        DataFrame::empty()
    } else {
        DataFrame::new_infer_height(columns).map_err(|e| {
            Diagnostic::compute_error("C0210", format!("Failed to construct DataFrame: {e}"))
        })?
    };

    Ok((frame, Arc::new(na_reasons)))
}

pub(crate) fn value_column_to_polars(name: &str, values: &[Value]) -> Column {
    let has_string = values.iter().any(|v| matches!(v, Value::String(_)));
    let has_f64 = values.iter().any(|v| matches!(v, Value::F64(_)));
    let has_i64 = values.iter().any(|v| matches!(v, Value::I64(_)));
    let has_bool = values.iter().any(|v| matches!(v, Value::Bool(_)));

    let series = if has_string {
        let data: Vec<Option<String>> = values
            .iter()
            .map(|v| match v {
                Value::NA(_) => None,
                Value::String(s) => Some(s.clone()),
                other => Some(other.to_string()),
            })
            .collect();
        data.into_iter().collect::<StringChunked>().with_name(name.into()).into_series()
    } else if has_f64 {
        let data: Vec<Option<f64>> = values.iter().map(|v| v.as_f64()).collect();
        data.into_iter().collect::<Float64Chunked>().with_name(name.into()).into_series()
    } else if has_i64 {
        let data: Vec<Option<i64>> = values.iter().map(|v| v.as_i64()).collect();
        data.into_iter().collect::<Int64Chunked>().with_name(name.into()).into_series()
    } else if has_bool {
        let data: Vec<Option<bool>> = values.iter().map(|v| v.as_bool()).collect();
        data.into_iter().collect::<BooleanChunked>().with_name(name.into()).into_series()
    } else {
        // Empty column or entirely NA: default to null Float64 series.
        let data: Vec<Option<f64>> = vec![None; values.len()];
        data.into_iter().collect::<Float64Chunked>().with_name(name.into()).into_series()
    };

    series.into()
}

/// Constructs a nullable `f64` column with a given name, using the same
/// `collect::<Float64Chunked>()` pattern as the f64 branch of [`value_column_to_polars`]
/// for a single fixed name without inferring dtype from `Vec<Value>`. Used by
/// `FittedModel::augment`/`FittedGlm::augment` to append `.fitted`/`.residual`.
pub(crate) fn f64_opt_column(name: &str, data: Vec<Option<f64>>) -> Column {
    data.into_iter().collect::<Float64Chunked>().with_name(name.into()).into_series().into()
}

/// Like [`f64_opt_column`] but for non-nullable `bool` (`.used_in_fit` is never NA).
pub(crate) fn bool_column(name: &str, data: Vec<bool>) -> Column {
    data.into_iter().map(Some).collect::<BooleanChunked>().with_name(name.into()).into_series().into()
}

/// Like [`f64_opt_column`] but for non-nullable `String` (`.na_reason` is always text
/// -- `"none"`, `"unspecified"`, or the actual reason -- never `NA`).
pub(crate) fn string_column(name: &str, data: Vec<String>) -> Column {
    data.into_iter().map(Some).collect::<StringChunked>().with_name(name.into()).into_series().into()
}

/// Like [`f64_opt_column`] but for non-nullable `i64` (e.g. `.cluster`).
pub(crate) fn i64_column(name: &str, data: Vec<i64>) -> Column {
    data.into_iter().map(Some).collect::<Int64Chunked>().with_name(name.into()).into_series().into()
}

/// Extracts a column from a Polars `DataFrame` back to a GHL `Vec<Value>`,
/// restoring `NA:Reason` from `na_reasons` when an entry exists — null cells
/// without an entry become `Value::NA(None)`.
pub fn pull_column_as_values(
    frame: &DataFrame,
    na_reasons: &NaReasonTable,
    col: &str,
) -> Result<Vec<Value>, Diagnostic> {
    let all_rows: Vec<usize> = (0..frame.height()).collect();
    pull_rows_as_values(frame, na_reasons, col, &all_rows)
}

/// Like [`pull_column_as_values`] but for an arbitrary subset of rows — used by
/// `summarize()` to extract values for a group without materializing the whole column.
pub(crate) fn pull_rows_as_values(
    frame: &DataFrame,
    na_reasons: &NaReasonTable,
    col: &str,
    rows: &[usize],
) -> Result<Vec<Value>, Diagnostic> {
    let column = frame.column(col).map_err(|_| {
        Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame", col))
    })?;

    let mut out = Vec::with_capacity(rows.len());
    for &row in rows {
        let av = column.get(row).map_err(|e| {
            Diagnostic::compute_error("C0210", format!("Error reading cell ({col}, {row}): {e}"))
        })?;
        out.push(any_value_to_value(&av, col, row, na_reasons));
    }
    Ok(out)
}

/// Single cell extraction — used by `group_by()`/`summarize()` to read group key values.
pub(crate) fn get_cell_as_value(
    frame: &DataFrame,
    na_reasons: &NaReasonTable,
    col: &str,
    row: usize,
) -> Result<Value, Diagnostic> {
    let column = frame.column(col).map_err(|_| {
        Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame", col))
    })?;
    let av = column.get(row).map_err(|e| {
        Diagnostic::compute_error("C0210", format!("Error reading cell ({col}, {row}): {e}"))
    })?;
    Ok(any_value_to_value(&av, col, row, na_reasons))
}

pub(crate) fn any_value_to_value(av: &AnyValue, col: &str, row: usize, na_reasons: &NaReasonTable) -> Value {
    match av {
        AnyValue::Null => match na_reasons.get(col, row) {
            Some(reason) => Value::NA(Some(reason.to_string())),
            None => Value::NA(None),
        },
        other => any_value_to_plain_value(other),
    }
}

/// Like [`any_value_to_value`] but without `(col, row)` context — for values not
/// originating from a specific DataFrame cell, such as scalar aggregation results
/// (`summarize()`'s `compute_agg` in `io.rs`). A `Null` here always becomes
/// `Value::NA(None)` since there is no reason lookup for synthetic scalars.
pub(crate) fn any_value_to_plain_value(av: &AnyValue) -> Value {
    match av {
        AnyValue::Null => Value::NA(None),
        AnyValue::Boolean(b) => Value::Bool(*b),
        AnyValue::String(s) => Value::String(s.to_string()),
        // `Column::get()` on a length-1 `ScalarColumn` (e.g. a single-element `Vector`
        // literal like `["id"]`, common as a column-name argument to `select`/`drop`/etc.)
        // returns this owned variant instead of the borrowed `String(&str)` above -- found
        // via `VectorData`'s round-trip corrupting single-string vectors (the fallback
        // arm below stringifies via `AnyValue`'s `Display`, which quotes string values,
        // turning `"id"` into the literal text `"\"id\""`).
        AnyValue::StringOwned(s) => Value::String(s.to_string()),
        AnyValue::Int8(n) => Value::I64(*n as i64),
        AnyValue::Int16(n) => Value::I64(*n as i64),
        AnyValue::Int32(n) => Value::I64(*n as i64),
        AnyValue::Int64(n) => Value::I64(*n),
        AnyValue::UInt8(n) => Value::I64(*n as i64),
        AnyValue::UInt16(n) => Value::I64(*n as i64),
        AnyValue::UInt32(n) => Value::I64(*n as i64),
        AnyValue::UInt64(n) => Value::I64(*n as i64),
        AnyValue::Float32(x) => Value::F64(*x as f64),
        AnyValue::Float64(x) => Value::F64(*x),
        // Types without direct GHL counterparts (dates, etc.) — preserved as strings
        // rather than dropping data silently.
        other => Value::String(format!("{other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_values_and_na_reasons_per_type() {
        let cols = vec![
            ("id".to_string(), vec![Value::I64(1), Value::I64(2), Value::NA(None), Value::I64(4)]),
            (
                "score".to_string(),
                vec![
                    Value::F64(10.0),
                    Value::NA(Some("SensorDropout".into())),
                    Value::F64(30.0),
                    Value::NA(Some("LowBattery".into())),
                ],
            ),
            (
                "label".to_string(),
                vec![Value::String("a".into()), Value::String("b".into()), Value::NA(None), Value::String("d".into())],
            ),
            ("active".to_string(), vec![Value::Bool(true), Value::Bool(false), Value::NA(None), Value::Bool(true)]),
        ];

        let (frame, na_reasons) = build_dataframe(&cols).expect("build_dataframe should succeed");
        assert_eq!(frame.height(), 4);

        for (name, original) in &cols {
            let pulled = pull_column_as_values(&frame, &na_reasons, name).expect("pull should succeed");
            assert_eq!(&pulled, original, "round-trip mismatch on column `{name}`");
        }
    }

    #[test]
    fn mixed_int_and_float_column_widens_to_f64() {
        let cols = vec![("x".to_string(), vec![Value::I64(1), Value::F64(2.5), Value::NA(None)])];
        let (frame, na_reasons) = build_dataframe(&cols).unwrap();

        let pulled = pull_column_as_values(&frame, &na_reasons, "x").unwrap();
        assert_eq!(pulled, vec![Value::F64(1.0), Value::F64(2.5), Value::NA(None)]);
    }

    #[test]
    fn all_na_column_defaults_to_null_without_panicking() {
        let cols = vec![("gap".to_string(), vec![Value::NA(None), Value::NA(Some("NoResponse".into()))])];
        let (frame, na_reasons) = build_dataframe(&cols).unwrap();

        let pulled = pull_column_as_values(&frame, &na_reasons, "gap").unwrap();
        assert_eq!(pulled, vec![Value::NA(None), Value::NA(Some("NoResponse".into()))]);
    }

    #[test]
    fn cloning_a_dataframe_value_shares_the_na_reasons_arc_instead_of_deep_cloning() {
        // TODO.md Fase 1, "Copy-on-Write" scoped to Value::DataFrame/GroupedDataFrame:
        // this is the property that actually matters (cloning a Value::DataFrame -- which
        // happens on every plain variable lookup/binding -- must not re-clone the whole
        // reasons table), not just "still correct after wrapping in Arc" (the other tests
        // already cover correctness and wouldn't fail even without the Arc).
        let cols = vec![("score".to_string(), vec![Value::NA(Some("SensorDropout".into())), Value::F64(1.0)])];
        let (frame, na_reasons) = build_dataframe(&cols).unwrap();
        let df = Value::DataFrame { frame, na_reasons };

        let cloned = df.clone();
        let (Value::DataFrame { na_reasons: original, .. }, Value::DataFrame { na_reasons: from_clone, .. }) = (&df, &cloned) else {
            panic!("expected both to be DataFrame");
        };
        assert!(
            std::sync::Arc::ptr_eq(original, from_clone),
            "Value::clone() on a DataFrame should share the same NaReasonTable allocation, not deep-clone it"
        );
    }

    #[test]
    fn pull_missing_column_reports_a_diagnostic() {
        let cols = vec![("x".to_string(), vec![Value::I64(1)])];
        let (frame, na_reasons) = build_dataframe(&cols).unwrap();

        let err = pull_column_as_values(&frame, &na_reasons, "does_not_exist").unwrap_err();
        assert_eq!(err.code, "S0201");
    }
}
