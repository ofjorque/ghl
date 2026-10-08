//! Phase 0 — Spike #2: Prototype of NA with semantic reason side-channel.
//!
//! Arrow/Polars only models a 1-bit validity mask per cell — it has no equivalent
//! for GHL's `NA:SensorDropout` (`Value::NA(Option<String>)`). This spike validates
//! that a side table `(column, row) -> reason`, attached to the wrapper of
//! `Value::DataFrame(polars::DataFrame)`, correctly survives row-preserving
//! operations (`filter`) by reindexing positional offsets.
//!
//! Validated rule: the reason of a surviving row is reindexed to its new position;
//! the reason of a discarded row disappears (no orphan reasons reassigned to incorrect rows).

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
        self.reasons
            .get(&(col.to_string(), row))
            .map(|s| s.as_str())
    }

    /// Reindexes reasons after an operation selecting a subset of rows.
    /// `kept_rows[new_row] == old_row` — the mapping produced by a
    /// `BooleanChunked` (filter), a `Vec<IdxSize>` slice/sort, or the left side of a join.
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
            // old_row is not in kept_rows -> row was filtered out, reason is discarded.
        }
        out
    }
}

/// Row indices (original positions in order) surviving a `BooleanChunked` mask
/// — the same mask passed to `DataFrame::filter`.
fn kept_rows_from_mask(mask: &BooleanChunked) -> Vec<usize> {
    mask.iter()
        .enumerate()
        .filter_map(|(i, keep)| keep.unwrap_or(false).then_some(i))
        .collect()
}

/// `filter` with reason propagation: applies the same mask to the DataFrame and
/// the reason table, keeping both positionally aligned.
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
    let score: Float64Chunked = scores
        .iter()
        .copied()
        .collect::<Float64Chunked>()
        .with_name("score".into());
    let keep_flag = Int64Chunked::from_vec("keep".into(), keep.to_vec());

    DataFrame::new_infer_height(vec![
        score.into_series().into(),
        keep_flag.into_series().into(),
    ])
    .expect("DataFrame construction should succeed")
}

#[test]
fn na_reason_reindexes_to_new_row_position_after_filter() {
    // score: [10.0, NA, 30.0, NA, 50.0] — reasons recorded on rows 1 and 3.
    let df = score_and_keep_df(
        &[Some(10.0), None, Some(30.0), None, Some(50.0)],
        &[1, 1, 0, 1, 1],
    );

    let mut reasons = NaReasonTable::default();
    reasons.set("score", 1, "SensorDropout");
    reasons.set("score", 3, "LowBattery");

    let keep_col = df.column("keep").unwrap().i64().unwrap().clone();
    let mask = keep_col.equal(1);
    let (filtered_df, filtered_reasons) = filter_with_reasons(&df, &reasons, &mask).unwrap();

    // Original rows 0, 1, 3, 4 survive (row 2 dropped) -> new positions 0, 1, 2, 3.
    assert_eq!(filtered_df.height(), 4);
    assert_eq!(filtered_reasons.get("score", 0), None); // row 0 never had a reason
    assert_eq!(filtered_reasons.get("score", 1), Some("SensorDropout")); // row 1 -> 1
    assert_eq!(filtered_reasons.get("score", 2), Some("LowBattery")); // row 3 -> 2
    assert_eq!(filtered_reasons.get("score", 3), None); // row 4 -> 3, without reason
}

#[test]
fn na_reason_is_dropped_when_its_row_is_filtered_out() {
    let df = score_and_keep_df(&[Some(1.0), None, Some(3.0)], &[1, 0, 1]);

    let mut reasons = NaReasonTable::default();
    reasons.set("score", 1, "SensorDropout"); // row that will be filtered out

    let keep_col = df.column("keep").unwrap().i64().unwrap().clone();
    let mask = keep_col.equal(1);
    let (filtered_df, filtered_reasons) = filter_with_reasons(&df, &reasons, &mask).unwrap();

    assert_eq!(filtered_df.height(), 2);
    assert!(
        filtered_reasons.reasons.is_empty(),
        "a discarded row's reason must not reappear at any new position"
    );
}

#[test]
fn na_reason_survives_a_no_op_filter_unchanged() {
    let df = score_and_keep_df(&[Some(1.0), None, Some(3.0)], &[1, 1, 1]);

    let mut reasons = NaReasonTable::default();
    reasons.set("score", 1, "SensorDropout");

    let keep_col = df.column("keep").unwrap().i64().unwrap().clone();
    let mask = keep_col.equal(1); // all rows survive
    let (filtered_df, filtered_reasons) = filter_with_reasons(&df, &reasons, &mask).unwrap();

    assert_eq!(filtered_df.height(), 3);
    assert_eq!(
        filtered_reasons, reasons,
        "with no discarded rows, the reason table should remain unchanged"
    );
}
