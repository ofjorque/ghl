//! Phase 1 (`benchmarks/suites/02`, Case 2.3) — benchmarks vectorized `filter()` (native
//! polars comparison over the column without boxing to `Vec<Value>`) on a real scaled
//! DataFrame to verify benchmark improvements with empirical data.
//!
//! Usage: `cargo run --release --example spike_filter_latency -p ghl-runtime -- <parquet_path>`

use std::time::Instant;
use ghl_syntax::ast::BinaryOp;
use ghl_runtime::io::{read_parquet_file, df_filter_by_col_predicate};
use ghl_runtime::polars_bridge::pull_column_as_values;
use ghl_runtime::Value;

/// Reconstructs the overhead of the previous `filter()` implementation:
/// boxing the entire column to `Vec<Value>` and comparing cell by cell in Rust,
/// providing an empirical before/after comparison.
fn naive_predicate_indices(df: &Value, col: &str, threshold: f64) -> Vec<usize> {
    let (frame, na_reasons) = match df {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        _ => panic!("expected DataFrame"),
    };
    let vals = pull_column_as_values(frame, na_reasons, col).expect("column should exist");
    vals.iter()
        .enumerate()
        .filter(|(_, v)| v.as_f64().map(|x| x > threshold).unwrap_or(false))
        .map(|(i, _)| i)
        .collect()
}

fn height_of(df: &Value) -> usize {
    match df {
        Value::DataFrame { frame, .. } => frame.height(),
        _ => 0,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| "target/synthetic.parquet".to_string());

    let df = read_parquet_file(&path).unwrap_or_else(|e| {
        eprintln!("Could not read {path}: {e:?}");
        eprintln!("Generate it first (see generate_synthetic_csv + write_parquet).");
        std::process::exit(1);
    });
    let total_rows = height_of(&df);
    println!("File: {path} ({total_rows} rows)");

    // Numeric: value_a > 500000 (~50% of rows, vectorized i64 comparison).
    let start = Instant::now();
    let numeric_filtered = df_filter_by_col_predicate(&df, "value_a", BinaryOp::Gt, &Value::I64(500_000))
        .expect("numeric filter should succeed");
    let elapsed = start.elapsed();
    println!(
        "\n[new]    value_a > 500000: {} of {} rows in {:.2?} ({:.0} rows/s)",
        height_of(&numeric_filtered),
        total_rows,
        elapsed,
        total_rows as f64 / elapsed.as_secs_f64()
    );

    // Same predicate with the cost of the old implementation (boxing + cell-by-cell comparison)
    // for a direct comparison against the optimized path.
    let start = Instant::now();
    let naive_indices = naive_predicate_indices(&df, "value_a", 500_000.0);
    let elapsed = start.elapsed();
    println!(
        "[legacy] value_a > 500000: {} of {} rows in {:.2?} ({:.0} rows/s) -- mask/indices calculation only, without DataFrame reconstruction",
        naive_indices.len(),
        total_rows,
        elapsed,
        total_rows as f64 / elapsed.as_secs_f64()
    );

    // String: category == "A" (~20% of rows, vectorized string comparison).
    let start = Instant::now();
    let string_filtered = df_filter_by_col_predicate(&df, "category", BinaryOp::Eq, &Value::String("A".into()))
        .expect("string filter should succeed");
    let elapsed = start.elapsed();
    println!(
        "[category == \"A\"] {} of {} rows in {:.2?} ({:.0} rows/s)",
        height_of(&string_filtered),
        total_rows,
        elapsed,
        total_rows as f64 / elapsed.as_secs_f64()
    );
}
