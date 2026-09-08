//! Fase 1 — mide `group_by() |> summarize()` tras la "optimización pendiente" que
//! TODO.md dejó anotada al enviar el primer `summarize()`: `compute_agg` ya no boxea
//! cada grupo a `Vec<Value>` y delega en `native_mean`/`native_sum`/etc. -- calcula
//! `mean_reduce`/`sum_reduce`/etc. nativos de polars directo sobre el subconjunto de
//! cada grupo (`Column::take` + `Column::mean_reduce`/...).
//!
//! Uso: `cargo run --release --example spike_summarize_latency -p ghl-runtime -- <parquet_path>`

use std::time::Instant;
use ghl_runtime::io::{read_parquet_file, df_group_by, df_summarize};
use ghl_runtime::Value;

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
        eprintln!("No se pudo leer {path}: {e:?}");
        std::process::exit(1);
    });
    let total_rows = height_of(&df);
    println!("Archivo: {path} ({total_rows} filas)");

    let specs = vec![
        ("n".to_string(), "count".to_string(), None),
        ("mean_value_b".to_string(), "mean".to_string(), Some("value_b".to_string())),
        ("max_value_b".to_string(), "max".to_string(), Some("value_b".to_string())),
        ("sum_value_c".to_string(), "sum".to_string(), Some("value_c".to_string())),
    ];

    let start = Instant::now();
    let grouped = df_group_by(&df, &["category".to_string()]).expect("group_by should succeed");
    let summary = df_summarize(&grouped, &specs).expect("summarize should succeed");
    let elapsed = start.elapsed();

    println!(
        "group_by(category) |> summarize(n, mean_value_b, max_value_b, sum_value_c): \
         {} filas -> {} grupos en {:.2?} ({:.0} filas/s)",
        total_rows,
        height_of(&summary),
        elapsed,
        total_rows as f64 / elapsed.as_secs_f64()
    );

    // Spike #1 (Fase 0) midio group_by+mean nativo de polars en ~601us para 100k filas
    // (`spike_polars_latency.rs`) -- referencia para juzgar si esto escala razonablemente,
    // no una comparacion exacta (ahi era una sola agregacion, aca son cuatro distintas).
    let projected_100k = elapsed.as_secs_f64() * (100_000.0 / total_rows as f64);
    println!("Para contexto, esto mismo a 100k filas: ~{:.0}us (Spike #1 con 1 sola agregacion nativa: ~601us)", projected_100k * 1_000_000.0);
}
