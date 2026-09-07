//! La capa de conversión en los bordes entre `Value` (dinámico, GHL) y
//! `polars_core::frame::DataFrame` (columnar, tipado) — TODO.md, Fase 0.
//!
//! Dos únicas operaciones cruzan esta frontera hoy:
//! - [`build_dataframe`]: construcción — el literal `dataframe { col: [...], ... }` y,
//!   más adelante, `read_csv`/`parse_csv`, arman un DataFrame real a partir de columnas
//!   `Vec<Value>` de GHL.
//! - [`pull_column_as_values`]: extracción — `pull(df, col)` saca una columna de vuelta
//!   a un `Value::Vector` de GHL puro.
//!
//! Todo lo demás (los ~30 verbos de `io.rs`) opera directo sobre el `DataFrame` de
//! polars sin pasar por `Value` en absoluto — ese es exactamente el punto de adoptarlo
//! (Fase 0, Opción A) en vez de reboxear cada celda en cada verbo.
//!
//! Este módulo todavía no está enchufado a `Value::DataFrame` (esa variante sigue con
//! su forma `HashMap`-based); es la implementación real y probada de la conversión,
//! lista para que la reescritura de `io.rs` (Fase 1) la use al reemplazar la variante.

use ghl_diagnostics::Diagnostic;
use polars_core::prelude::*;

use crate::na_reasons::NaReasonTable;
use crate::value::Value;

/// Construye un `DataFrame` de polars + su tabla de razones de NA a partir de columnas
/// GHL (`Vec<Value>`, todas de la misma longitud). Infiere el dtype de cada columna
/// ensanchando al tipo más permisivo presente: `String` > `f64` > `i64` > `bool` — la
/// misma política de "si hay cualquier duda, quedate con lo más general" que ya usan
/// `select`/`drop` en `io.rs` para columnas heterogéneas (`format!("{other}")`).
pub fn build_dataframe(cols: &[(String, Vec<Value>)]) -> Result<(DataFrame, NaReasonTable), Diagnostic> {
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
            Diagnostic::compute_error("C0210", format!("No se pudo construir el DataFrame: {e}"))
        })?
    };

    Ok((frame, na_reasons))
}

fn value_column_to_polars(name: &str, values: &[Value]) -> Column {
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
        // Columna vacía o enteramente NA: sin celdas para inferir tipo, F64-nula por defecto.
        let data: Vec<Option<f64>> = vec![None; values.len()];
        data.into_iter().collect::<Float64Chunked>().with_name(name.into()).into_series()
    };

    series.into()
}

/// Extrae una columna de un `DataFrame` de polars de vuelta a `Vec<Value>` de GHL,
/// reconstruyendo `NA:razon` desde `na_reasons` donde exista una entrada — celdas nulas
/// sin entrada quedan como `Value::NA(None)`.
pub fn pull_column_as_values(
    frame: &DataFrame,
    na_reasons: &NaReasonTable,
    col: &str,
) -> Result<Vec<Value>, Diagnostic> {
    let column = frame.column(col).map_err(|_| {
        Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame", col))
    })?;

    let height = frame.height();
    let mut out = Vec::with_capacity(height);
    for row in 0..height {
        let av = column.get(row).map_err(|e| {
            Diagnostic::compute_error("C0210", format!("Error leyendo la celda ({col}, {row}): {e}"))
        })?;
        out.push(any_value_to_value(&av, col, row, na_reasons));
    }
    Ok(out)
}

fn any_value_to_value(av: &AnyValue, col: &str, row: usize, na_reasons: &NaReasonTable) -> Value {
    match av {
        AnyValue::Null => match na_reasons.get(col, row) {
            Some(reason) => Value::NA(Some(reason.to_string())),
            None => Value::NA(None),
        },
        AnyValue::Boolean(b) => Value::Bool(*b),
        AnyValue::String(s) => Value::String(s.to_string()),
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
        // Tipos que todavía no tienen contraparte en GHL (fechas, etc.) — se preservan
        // como texto en vez de perder el dato silenciosamente.
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
    fn pull_missing_column_reports_a_diagnostic() {
        let cols = vec![("x".to_string(), vec![Value::I64(1)])];
        let (frame, na_reasons) = build_dataframe(&cols).unwrap();

        let err = pull_column_as_values(&frame, &na_reasons, "does_not_exist").unwrap_err();
        assert_eq!(err.code, "S0201");
    }
}
