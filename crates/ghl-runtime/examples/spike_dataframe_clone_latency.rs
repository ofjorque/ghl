//! Phase 1 — measures bounded Copy-on-Write for `Value::DataFrame`/`GroupedDataFrame`
//! (Case 2.3): `na_reasons` transitioned from `NaReasonTable` (owned) to
//! `Arc<NaReasonTable>`, so cloning a `Value::DataFrame` — which occurs on every
//! variable lookup, binding, and pass-by-value — no longer clones the entire reason table.
//! This spike measures the overhead avoided: cloning the same table (with N registered
//! `NA:reason` cells, the worst case) via the old path (deep-cloning full `NaReasonTable`)
//! versus the new path (cloning the `Arc`).
//!
//! Usage: `cargo run --release --example spike_dataframe_clone_latency -p ghl-runtime -- <n_reasons>`

use std::sync::Arc;
use std::time::Instant;
use ghl_runtime::na_reasons::NaReasonTable;

const REPEATS: u32 = 50;

fn build_table(n: usize) -> NaReasonTable {
    let mut t = NaReasonTable::new();
    for i in 0..n {
        t.set("score", i, format!("SensorDropout-{i}"));
    }
    t
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200_000);

    let table = build_table(n);
    let arc_table = Arc::new(build_table(n));

    println!("NaReasonTable with {n} registered reasons (worst-case: all cells populated)");

    let mut best_deep = std::time::Duration::MAX;
    for _ in 0..REPEATS {
        let start = Instant::now();
        let _cloned = table.clone();
        best_deep = best_deep.min(start.elapsed());
    }
    println!("  Clone entire NaReasonTable (legacy path): min over {REPEATS} runs: {best_deep:>10.3?}");

    let mut best_arc = std::time::Duration::MAX;
    for _ in 0..REPEATS {
        let start = Instant::now();
        let _cloned = Arc::clone(&arc_table);
        best_arc = best_arc.min(start.elapsed());
    }
    println!("  Clone Arc<NaReasonTable> (new path):      min over {REPEATS} runs: {best_arc:>10.3?}");

    if !best_arc.is_zero() {
        let speedup = best_deep.as_secs_f64() / best_arc.as_secs_f64();
        println!("\nSpeedup: ~{speedup:.0}x (grows with N — new path is O(1), old path is O(N))");
    }
}
