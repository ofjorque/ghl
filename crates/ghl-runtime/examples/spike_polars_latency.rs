//! Phase 0 — Spike #1: latency of building/operating on a `polars::DataFrame`
//! at small scale (10 - 100k rows), confirming that adopting polars as the backend
//! for `Value::DataFrame` (Phase 0/1) does not violate the startup and small-scale goals
//! of `benchmarks/suites/04-runtime-characteristics.md` (Test C: filter + mean
//! on a table responding within the millisecond range).
//!
//! Run: `cargo run --release --example spike_polars_latency -p ghl-runtime`

use std::time::Instant;

use polars_core::prelude::*;
use polars_lazy::prelude::*;

const REPEATS: u32 = 20;

fn build_df(n: usize) -> DataFrame {
    let id = Int64Chunked::from_vec("id".into(), (0..n as i64).collect());
    let group = Int64Chunked::from_vec("group".into(), (0..n as i64).map(|i| i % 10).collect());
    let value = Float64Chunked::from_vec("value".into(), (0..n).map(|i| i as f64 * 0.5).collect());

    DataFrame::new_infer_height(vec![
        id.into_series().into(),
        group.into_series().into(),
        value.into_series().into(),
    ])
    .expect("DataFrame construction should succeed")
}

fn time_it<T>(label: &str, mut f: impl FnMut() -> T) {
    // One untimed warm-up run (first-call allocator/branch-predictor noise),
    // then take the minimum over REPEATS as the representative latency.
    let _ = f();

    let mut best = std::time::Duration::MAX;
    for _ in 0..REPEATS {
        let start = Instant::now();
        let _ = f();
        best = best.min(start.elapsed());
    }
    println!("  {label:<28} min over {REPEATS} runs: {best:>10.3?}");
}

fn run_case(n: usize) {
    println!("\n== n = {n} rows ==");

    time_it("construct DataFrame", || build_df(n));

    let df = build_df(n);
    let value_ca = df.column("value").unwrap().f64().unwrap().clone();
    let threshold = (n as f64 * 0.5) / 2.0;

    time_it("filter (value > threshold)", || {
        let mask = value_ca.gt(threshold);
        df.filter(&mask).expect("filter should succeed")
    });

    // `GroupBy::select().mean()` (the eager API) has been deprecated since polars-core
    // 0.24.1 in favor of exactly this lazy-query path -- `ghl-runtime`'s own
    // `df_summarize` (io.rs, Phase 1 follow-up optimization) uses the same
    // `LazyFrame::group_by().agg([...])` shape, so this spike matches the real code path
    // instead of measuring an API the runtime no longer calls.
    time_it("group_by(group).agg(mean(value))", || {
        df.clone()
            .lazy()
            .group_by([col("group")])
            .agg([col("value").mean()])
            .collect()
            .expect("mean aggregation should succeed")
    });
}

fn main() {
    println!("Spike #1 — polars::DataFrame latency at small scale");
    println!("(release mode matters here — run with --release)");

    for n in [10usize, 1_000, 100_000] {
        run_case(n);
    }

    println!(
        "\nCompare these numbers against Suite 04 targets (Test C, under 25ms end-to-end \
         for 100k rows) before deciding if a separate small-DataFrame code path is needed."
    );
}
