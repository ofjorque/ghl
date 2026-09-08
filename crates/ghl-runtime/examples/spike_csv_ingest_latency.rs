//! Fase 1 (`benchmarks/suites/02`, Caso 2.1) — compara el parser CSV viejo
//! (`parse_csv_string`, single-thread, usado hoy solo por `parse_csv()` sobre texto en
//! memoria) contra el nuevo `read_csv_file` (polars-io multi-hilo para tokenizar +
//! inferencia/`NA:razon` de GHL sin cambios encima), sobre un archivo generado por
//! `generate_synthetic_csv` — con números reales, no supuestos.
//!
//! Uso: `cargo run --release --example spike_csv_ingest_latency -p ghl-runtime -- <csv_path>`

use std::time::Instant;
use ghl_runtime::io::{read_file, parse_csv_string, read_csv_file};

fn height_of(df: &ghl_runtime::Value) -> usize {
    match df {
        ghl_runtime::Value::DataFrame { frame, .. } => frame.height(),
        _ => 0,
    }
}

fn report(label: &str, size_mb: f64, height: usize, elapsed: std::time::Duration) {
    let throughput_mb_s = size_mb / elapsed.as_secs_f64();
    let rows_per_sec = height as f64 / elapsed.as_secs_f64();
    println!("\n[{label}] {height} filas en {elapsed:.2?}");
    println!("[{label}] Throughput: {throughput_mb_s:.1} MB/s, {rows_per_sec:.0} filas/s");
    if height > 0 {
        let projected_25m = elapsed.as_secs_f64() * (25_000_000.0 / height as f64);
        println!("[{label}] Proyeccion lineal a 25M filas: ~{:.1} min", projected_25m / 60.0);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| "target/synthetic.csv".to_string());

    let content = read_file(&path).unwrap_or_else(|e| {
        eprintln!("No se pudo leer {path}: {e:?}");
        eprintln!("Generalo primero con: cargo run --release --example generate_synthetic_csv -p ghl-runtime -- <rows> {path}");
        std::process::exit(1);
    });
    let size_mb = content.len() as f64 / (1024.0 * 1024.0);
    let num_lines = content.lines().count().saturating_sub(1);
    println!("Archivo: {path} ({size_mb:.1} MB, ~{num_lines} filas)");

    println!("\n== parse_csv_string (viejo, single-thread) ==");
    let start = Instant::now();
    let df_old = parse_csv_string(&content, None).expect("old CSV parse should succeed");
    report("viejo", size_mb, height_of(&df_old), start.elapsed());

    println!("\n== read_csv_file (nuevo, polars-io multi-hilo + inferencia GHL) ==");
    let start = Instant::now();
    let df_new = read_csv_file(&path, None).expect("new CSV read should succeed");
    report("nuevo", size_mb, height_of(&df_new), start.elapsed());
}
