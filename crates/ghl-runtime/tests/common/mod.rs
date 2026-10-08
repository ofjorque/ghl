//! Shared test helpers for ghl-runtime integration tests.
//!
//! Each `tests/*.rs` file is its own compilation unit and only uses a subset
//! of these - `#[allow(dead_code)]` suppresses the resulting per-binary
//! "never used" warnings, which don't indicate an actual problem here.

#![allow(dead_code)]

use ghl_runtime::polars_bridge;
use ghl_runtime::value::Value;

/// Test helper: extract a column's values from a `Value::DataFrame`, panicking with
/// a clear message if `v` isn't one or the column doesn't exist — keeps assertions
/// below focused on what they're checking rather than on the polars accessor API.
pub fn df_column(v: &Value, col: &str) -> Vec<Value> {
    match v {
        Value::DataFrame { frame, na_reasons } => {
            polars_bridge::pull_column_as_values(frame, na_reasons, col)
                .unwrap_or_else(|e| panic!("column `{col}` not found: {e:?}"))
        }
        other => panic!("Expected DataFrame, found `{}`", other.type_name()),
    }
}

pub fn df_columns(v: &Value) -> Vec<String> {
    match v {
        Value::DataFrame { frame, .. } => frame
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect(),
        other => panic!("Expected DataFrame, found `{}`", other.type_name()),
    }
}

pub fn df_height(v: &Value) -> usize {
    match v {
        Value::DataFrame { frame, .. } => frame.height(),
        other => panic!("Expected DataFrame, found `{}`", other.type_name()),
    }
}

pub fn matrix_data(v: &Value) -> Vec<f64> {
    match v {
        Value::Matrix { data, .. } => data.to_vec(),
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

pub fn vector_f64(v: &Value) -> Vec<f64> {
    match v {
        Value::Vector(items) => items.iter().map(|x| x.as_f64().unwrap()).collect(),
        other => panic!("Expected Vector, found {other:?}"),
    }
}
