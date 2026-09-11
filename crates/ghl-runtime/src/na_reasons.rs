//! NA-con-razón sobre un backend Arrow/polars.
//!
//! Arrow (y por lo tanto `polars_core`) solo modela un bit de validez por celda — no
//! tiene equivalente a la NA semántica de GHL (`NA:SensorDropout`, `Value::NA(Some(reason))`).
//! Esta tabla lateral `(columna, fila) -> razón` se adjunta a cada `Value::DataFrame` real
//! (backed por `polars_core::frame::DataFrame`) para no perder esa información.
//!
//! El mecanismo de reindexado (`reindex`) fue validado en el Spike #2 de la Fase 0
//! (`tests/spike_na_reason.rs`) sobre `filter`; este módulo es la versión de producción
//! del mismo diseño, ahora reutilizable desde cualquier verbo que preserve filas
//! (`filter`, `arrange`, `slice`, joins, ...).

use std::collections::HashMap;

/// Razones de NA para un `Value::DataFrame`, indexadas por `(nombre_columna, fila)`.
/// Una celda nula sin entrada aquí es una NA "genérica" (`Value::NA(None)`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NaReasonTable {
    reasons: HashMap<(String, usize), String>,
}

impl NaReasonTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.reasons.is_empty()
    }

    pub fn set(&mut self, col: &str, row: usize, reason: impl Into<String>) {
        self.reasons.insert((col.to_string(), row), reason.into());
    }

    pub fn get(&self, col: &str, row: usize) -> Option<&str> {
        self.reasons.get(&(col.to_string(), row)).map(|s| s.as_str())
    }

    /// Reindexa las razones tras una operación que selecciona un subconjunto de filas.
    /// `kept_rows[new_row] == old_row` — el mapeo que produce un `BooleanChunked`
    /// (filter), un slice/sort, o el lado izquierdo de un join. Las razones de filas
    /// no presentes en `kept_rows` se descartan (esas filas ya no existen).
    pub fn reindex(&self, kept_rows: &[usize]) -> NaReasonTable {
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
        }
        out
    }

    /// Renombra todas las entradas de `old_col` a `new_col` (usado por `rename()`).
    pub fn rename_column(&self, old_col: &str, new_col: &str) -> NaReasonTable {
        let mut out = NaReasonTable::default();
        for ((col, row), reason) in &self.reasons {
            let mapped = if col == old_col { new_col } else { col.as_str() };
            out.set(mapped, *row, reason.clone());
        }
        out
    }

    /// Conserva solo las razones de las columnas en `keep_cols` (usado por `select()`/`drop()`).
    pub fn retain_columns(&self, keep_cols: &[String]) -> NaReasonTable {
        let mut out = NaReasonTable::default();
        for ((col, row), reason) in &self.reasons {
            if keep_cols.iter().any(|c| c == col) {
                out.set(col, *row, reason.clone());
            }
        }
        out
    }

    /// Descarta las razones de una sola columna (usado por `fill_na()`: tras rellenar,
    /// esa columna ya no tiene celdas NA, así que ninguna razón sigue siendo válida).
    pub fn without_column(&self, col: &str) -> NaReasonTable {
        let mut out = NaReasonTable::default();
        for ((c, row), reason) in &self.reasons {
            if c != col {
                out.set(c, *row, reason.clone());
            }
        }
        out
    }

    /// Fusiona las entradas de otra tabla, sobrescribiendo en caso de choque
    /// (usado por `mutate()` al reemplazar una columna: primero se descartan sus
    /// razones viejas con [`without_column`], luego se insertan las nuevas).
    pub fn merge(&mut self, other: &NaReasonTable) {
        for (k, v) in &other.reasons {
            self.reasons.insert(k.clone(), v.clone());
        }
    }

    /// Desplaza las razones `periods` filas para una longitud total `len`.
    /// `periods > 0` (lag): fila `r` pasa a `r + periods` (si `< len`).
    /// `periods < 0` (lead): fila `r` pasa a `r + periods` (si `>= 0`).
    pub fn shift(&self, periods: i64, len: usize) -> NaReasonTable {
        if self.reasons.is_empty() || periods == 0 {
            return self.clone();
        }
        let mut out = NaReasonTable::default();
        for ((col, old_row), reason) in &self.reasons {
            let new_row = (*old_row as i64) + periods;
            if new_row >= 0 && (new_row as usize) < len {
                out.set(col, new_row as usize, reason.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindex_keeps_surviving_rows_and_drops_the_rest() {
        let mut t = NaReasonTable::new();
        t.set("score", 1, "SensorDropout");
        t.set("score", 3, "LowBattery");

        // Sobreviven las filas originales 0,1,3,4 -> nuevas posiciones 0,1,2,3.
        let reindexed = t.reindex(&[0, 1, 3, 4]);
        assert_eq!(reindexed.get("score", 1), Some("SensorDropout"));
        assert_eq!(reindexed.get("score", 2), Some("LowBattery"));
        assert_eq!(reindexed.get("score", 0), None);
        assert_eq!(reindexed.get("score", 3), None);
    }

    #[test]
    fn rename_column_moves_entries_without_touching_others() {
        let mut t = NaReasonTable::new();
        t.set("group", 0, "NoResponse");
        t.set("score", 0, "SensorDropout");

        let renamed = t.rename_column("group", "cohort");
        assert_eq!(renamed.get("cohort", 0), Some("NoResponse"));
        assert_eq!(renamed.get("group", 0), None);
        assert_eq!(renamed.get("score", 0), Some("SensorDropout"));
    }

    #[test]
    fn retain_columns_drops_reasons_for_removed_columns() {
        let mut t = NaReasonTable::new();
        t.set("keep_me", 0, "A");
        t.set("drop_me", 0, "B");

        let retained = t.retain_columns(&["keep_me".to_string()]);
        assert_eq!(retained.get("keep_me", 0), Some("A"));
        assert_eq!(retained.get("drop_me", 0), None);
    }

    #[test]
    fn test_shift_lag_and_lead() {
        let mut t = NaReasonTable::new();
        t.set("col", 1, "Reason1");

        // Lag by 1 on length 4: row 1 -> row 2
        let lagged = t.shift(1, 4);
        assert_eq!(lagged.get("col", 2), Some("Reason1"));
        assert_eq!(lagged.get("col", 1), None);

        // Lead by 1 on length 4: row 1 -> row 0
        let leaded = t.shift(-1, 4);
        assert_eq!(leaded.get("col", 0), Some("Reason1"));
        assert_eq!(leaded.get("col", 1), None);

        // Lag beyond len drops
        let dropped = t.shift(3, 4);
        assert_eq!(dropped.get("col", 4), None);
        assert!(dropped.is_empty());
    }
}

