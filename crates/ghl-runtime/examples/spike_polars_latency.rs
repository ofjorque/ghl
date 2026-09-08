//! Fase 0 — Spike #1: latencia de construir/operar sobre un `polars::DataFrame`
//! a escala chica (10 - 100k filas), para confirmar que adoptar polars como backend
//! de `Value::DataFrame` (TODO.md, Fase 0/1) no rompe las metas de arranque de
//! `benchmarks/suites/04-runtime-characteristics.md` (Prueba C: filtrar + media
//! sobre una tabla y responder en el rango de milisegundos).
//!
//! Corridas: `cargo run --release --example spike_polars_latency -p ghl-runtime`

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
    // `df_summarize` (io.rs, TODO.md Fase 1 "Optimización de seguimiento") uses the same
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
        "\nLee estos numeros contra la meta de Suite 04 (Prueba C, menos de 25ms de punta \
         a punta para 100k filas) antes de decidir si hace falta un camino separado \
         para DataFrames chicos."
    );
}
