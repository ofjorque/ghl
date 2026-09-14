//! Side-channel table tracking NA reasons on top of an Arrow/Polars backend.
//!
//! Arrow (and consequently `polars_core`) only models a 1-bit validity mask per cell
//! — it has no native equivalent for GHL's semantic NAs (`NA:SensorDropout`,
//! `Value::NA(Some(reason))`). This side table `(column, row) -> reason` attaches to
//! each real `Value::DataFrame` (backed by `polars_core::frame::DataFrame`) to preserve
//! that semantic metadata across operations.
//!
//! The reindexing mechanism (`reindex`) was validated in Fase 0 Spike #2
//! (`tests/spike_na_reason.rs`) on `filter`; this module represents the production
//! version of that design, reusable across all row-preserving verbs
//! (`filter`, `arrange`, `slice`, joins, etc.).

use std::collections::HashMap;
use std::sync::Arc;

/// NA reasons for a `Value::DataFrame`, indexed by `(column_name, row)`.
/// A null cell without an entry here is a generic NA (`Value::NA(None)`).
/// Reasons are stored as `Arc<str>` so multiple entries with the same text
/// (e.g. 100,000 cells with `"SensorDropout"`) share the same immutable heap descriptor
/// without string duplication on insertion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NaReasonTable {
    reasons: HashMap<(String, usize), Arc<str>>,
}

impl NaReasonTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.reasons.is_empty()
    }

    pub fn set(&mut self, col: &str, row: usize, reason: impl AsRef<str>) {
        self.reasons.insert((col.to_string(), row), Arc::from(reason.as_ref()));
    }

    pub fn get(&self, col: &str, row: usize) -> Option<&str> {
        self.reasons.get(&(col.to_string(), row)).map(|s| s.as_ref())
    }

    pub fn remove(&mut self, col: &str, row: usize) {
        self.reasons.remove(&(col.to_string(), row));
    }

    /// Reindexes reasons after an operation that selects a subset of rows.
    /// `kept_rows[new_row] == old_row` — the mapping produced by a `BooleanChunked`
    /// (filter), a slice/sort, or the left side of a join. Reasons for rows
    /// not present in `kept_rows` are dropped (those rows no longer exist).
    pub fn reindex(&self, kept_rows: &[usize]) -> NaReasonTable {
        let position_of_old: HashMap<usize, usize> = kept_rows
            .iter()
            .enumerate()
            .map(|(new_row, &old_row)| (old_row, new_row))
            .collect();

        let mut out = NaReasonTable::default();
        for ((col, old_row), reason) in &self.reasons {
            if let Some(&new_row) = position_of_old.get(old_row) {
                out.reasons.insert((col.clone(), new_row), Arc::clone(reason));
            }
        }
        out
    }

    /// Renames all entries from `old_col` to `new_col` (used by `rename()`).
    pub fn rename_column(&self, old_col: &str, new_col: &str) -> NaReasonTable {
        let mut out = NaReasonTable::default();
        for ((col, row), reason) in &self.reasons {
            let mapped = if col == old_col { new_col } else { col.as_str() };
            out.reasons.insert((mapped.to_string(), *row), Arc::clone(reason));
        }
        out
    }

    /// Retains only the reasons for columns in `keep_cols` (used by `select()`/`drop()`).
    pub fn retain_columns(&self, keep_cols: &[String]) -> NaReasonTable {
        let mut out = NaReasonTable::default();
        for ((col, row), reason) in &self.reasons {
            if keep_cols.iter().any(|c| c == col) {
                out.reasons.insert((col.clone(), *row), Arc::clone(reason));
            }
        }
        out
    }

    /// Drops the reasons for a single column (used by `fill_na()`: after filling,
    /// that column no longer has NA cells, so none of its reasons remain valid).
    pub fn without_column(&self, col: &str) -> NaReasonTable {
        let mut out = NaReasonTable::default();
        for ((c, row), reason) in &self.reasons {
            if c != col {
                out.reasons.insert((c.clone(), *row), Arc::clone(reason));
            }
        }
        out
    }

    /// Merges entries from another table, overwriting on key collision
    /// (used by `mutate()` when replacing a column: old reasons are first dropped
    /// with [`without_column`], then new ones are inserted).
    pub fn merge(&mut self, other: &NaReasonTable) {
        for (k, v) in &other.reasons {
            self.reasons.insert(k.clone(), Arc::clone(v));
        }
    }

    /// Shifts reasons by `periods` rows for a total column length `len`.
    /// `periods > 0` (lag): row `r` moves to `r + periods` (if `< len`).
    /// `periods < 0` (lead): row `r` moves to `r + periods` (if `>= 0`).
    pub fn shift(&self, periods: i64, len: usize) -> NaReasonTable {
        if self.reasons.is_empty() || periods == 0 {
            return self.clone();
        }
        let mut out = NaReasonTable::default();
        for ((col, old_row), reason) in &self.reasons {
            let new_row = (*old_row as i64) + periods;
            if new_row >= 0 && (new_row as usize) < len {
                out.reasons.insert((col.clone(), new_row as usize), Arc::clone(reason));
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

        // Original rows 0, 1, 3, 4 survive -> new positions 0, 1, 2, 3.
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

    #[test]
    fn test_reasons_arc_clone_is_zero_copy() {
        // After reindex, each surviving entry must share the *same* Arc pointer as in
        // the original table — no string data is duplicated. This is the key property
        // that makes operating on large NA-annotated DataFrames O(1) in reason memory.
        let mut t = NaReasonTable::new();
        t.set("col", 0, "SensorDropout");
        t.set("col", 1, "LowBattery");

        // Keep only row 0 → new row 0.
        let reindexed = t.reindex(&[0]);

        let orig = t.reasons.get(&("col".to_string(), 0)).unwrap();
        let copy = reindexed.reasons.get(&("col".to_string(), 0)).unwrap();
        assert!(Arc::ptr_eq(orig, copy),
            "reindex must share Arc pointers, not clone string data");
    }
}
