//! Fase 0 — Spike #2: prototipo del side-channel de NA con razón.
//!
//! Arrow/polars solo modelan un bit de validez por celda — no tienen equivalente a
//! `NA:SensorDropout` (GHL, ver `Value::NA(Option<String>)`). Este spike valida que una
//! tabla lateral `(columna, fila) -> razón`, adjunta al futuro wrapper de
//! `Value::DataFrame(polars::DataFrame)` (TODO.md, Fase 0), sobrevive correctamente a
//! una operación que preserva filas (`filter`) reindexando por posición — antes de
//! aplicar el mismo mecanismo a los ~30 verbos en Fase 1.
//!
//! Regla validada: la razón de una fila que sobrevive se reindexa a su nueva posición;
//! la razón de una fila que se descarta desaparece (no hay "razón huérfana" reasignada
//! a la fila equivocada).

use std::collections::HashMap;

use polars_core::prelude::*;

#[derive(Debug, Default, Clone, PartialEq)]
struct NaReasonTable {
    reasons: HashMap<(String, usize), String>,
}

impl NaReasonTable {
    fn set(&mut self, col: &str, row: usize, reason: impl Into<String>) {
        self.reasons.insert((col.to_string(), row), reason.into());
    }

    fn get(&self, col: &str, row: usize) -> Option<&str> {
        self.reasons.get(&(col.to_string(), row)).map(|s| s.as_str())
    }

    /// Reindexa las razones tras una operación que selecciona un subconjunto de filas.
    /// `kept_rows[new_row] == old_row` — el mismo tipo de mapeo que produce un
    /// `BooleanChunked` (filter), un `Vec<IdxSize>` de slice/sort, o el lado izquierdo
    /// de un join.
    fn reindex(&self, kept_rows: &[usize]) -> NaReasonTable {
        let position_of_old: HashMap<usize, usize> = kept_rows
            .iter()
            .enumerate()
            .map(|(new_row, &old_row)| (old_row, new_row))
            .collect();

        let mut out = NaReasonTable::default();
        for ((col, old_row), reason) in &self.reasons {
            if let Some(&new_row) = position_of_old.get(old_row) {
                out.set(col, new_row, reason.clone());
            }
            // old_row no está en kept_rows -> la fila se filtró, la razón se descarta.
        }
        out
    }
}

/// Índices de fila (posiciones originales, en orden) que sobreviven a un `BooleanChunked`
/// — el mismo mask que se le pasa a `DataFrame::filter`.
fn kept_rows_from_mask(mask: &BooleanChunked) -> Vec<usize> {
    mask.iter()
        .enumerate()
        .filter_map(|(i, keep)| keep.unwrap_or(false).then_some(i))
        .collect()
}

/// `filter` con propagación de razones: aplica el mismo mask al DataFrame y a la tabla
/// de razones, dejando ambos alineados por posición.
fn filter_with_reasons(
    df: &DataFrame,
    reasons: &NaReasonTable,
    mask: &BooleanChunked,
) -> PolarsResult<(DataFrame, NaReasonTable)> {
    let kept = kept_rows_from_mask(mask);
    let filtered_df = df.filter(mask)?;
    let filtered_reasons = reasons.reindex(&kept);
    Ok((filtered_df, filtered_reasons))
}

fn score_and_keep_df(scores: &[Option<f64>], keep: &[i64]) -> DataFrame {
    let score: Float64Chunked = scores.iter().copied().collect::<Float64Chunked>().with_name("score".into());
    let keep_flag = Int64Chunked::from_vec("keep".into(), keep.to_vec());

    DataFrame::new_infer_height(vec![score.into_series().into(), keep_flag.into_series().into()])
        .expect("DataFrame construction should succeed")
}

#[test]
fn na_reason_reindexes_to_new_row_position_after_filter() {
    // score: [10.0, NA, 30.0, NA, 50.0] — razones registradas en las filas 1 y 3.
    let df = score_and_keep_df(&[Some(10.0), None, Some(30.0), None, Some(50.0)], &[1, 1, 0, 1, 1]);

    let mut reasons = NaReasonTable::default();
    reasons.set("score", 1, "SensorDropout");
    reasons.set("score", 3, "LowBattery");

    let keep_col = df.column("keep").unwrap().i64().unwrap().clone();
    let mask = keep_col.equal(1);
    let (filtered_df, filtered_reasons) = filter_with_reasons(&df, &reasons, &mask).unwrap();

    // Sobreviven las filas originales 0,1,3,4 (se cae la 2) -> nuevas posiciones 0,1,2,3.
    assert_eq!(filtered_df.height(), 4);
    assert_eq!(filtered_reasons.get("score", 0), None); // fila 0 nunca tuvo razón
    assert_eq!(filtered_reasons.get("score", 1), Some("SensorDropout")); // fila 1 -> 1
    assert_eq!(filtered_reasons.get("score", 2), Some("LowBattery")); // fila 3 -> 2
    assert_eq!(filtered_reasons.get("score", 3), None); // fila 4 -> 3, sin razón
}

#[test]
fn na_reason_is_dropped_when_its_row_is_filtered_out() {
    let df = score_and_keep_df(&[Some(1.0), None, Some(3.0)], &[1, 0, 1]);

    let mut reasons = NaReasonTable::default();
    reasons.set("score", 1, "SensorDropout"); // justo la fila que se va a filtrar

    let keep_col = df.column("keep").unwrap().i64().unwrap().clone();
    let mask = keep_col.equal(1);
    let (filtered_df, filtered_reasons) = filter_with_reasons(&df, &reasons, &mask).unwrap();

    assert_eq!(filtered_df.height(), 2);
    assert!(
        filtered_reasons.reasons.is_empty(),
        "la razón de una fila descartada no debe reaparecer en ninguna posición nueva"
    );
}

#[test]
fn na_reason_survives_a_no_op_filter_unchanged() {
    let df = score_and_keep_df(&[Some(1.0), None, Some(3.0)], &[1, 1, 1]);

    let mut reasons = NaReasonTable::default();
    reasons.set("score", 1, "SensorDropout");

    let keep_col = df.column("keep").unwrap().i64().unwrap().clone();
    let mask = keep_col.equal(1); // todas las filas sobreviven
    let (filtered_df, filtered_reasons) = filter_with_reasons(&df, &reasons, &mask).unwrap();

    assert_eq!(filtered_df.height(), 3);
    assert_eq!(filtered_reasons, reasons, "sin filas descartadas, la tabla de razones no debería cambiar");
}
