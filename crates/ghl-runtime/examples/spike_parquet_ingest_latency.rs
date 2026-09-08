//! Fase 1 (`benchmarks/README.md` menciona Parquet junto a CSV) — mide `read_parquet_file`
//! contra el mismo archivo (convertido antes con `write_parquet`), para tener el
//! contraste real Parquet vs CSV en vez de asumir "Parquet siempre gana".
//!
//! Uso: `cargo run --release --example spike_parquet_ingest_latency -p ghl-runtime -- <parquet_path>`

use std::time::Instant;
use ghl_runtime::io::read_parquet_file;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| "target/synthetic.parquet".to_string());

    let size_mb = std::fs::metadata(&path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or_else(|e| {
            eprintln!("No se pudo leer {path}: {e:?}");
            eprintln!("Generalo primero: leer el CSV con read_csv() y escribirlo con write_parquet() vía un script .gh");
            std::process::exit(1);
        });

    println!("Archivo: {path} ({size_mb:.1} MB)");

    let start = Instant::now();
    let df = read_parquet_file(&path).expect("Parquet read should succeed");
    let elapsed = start.elapsed();

    let height = match &df {
        ghl_runtime::Value::DataFrame { frame, .. } => frame.height(),
        _ => 0,
    };
    let throughput_mb_s = size_mb / elapsed.as_secs_f64();
    let rows_per_sec = height as f64 / elapsed.as_secs_f64();

    println!("Leidas {height} filas en {elapsed:.2?}");
    println!("Throughput: {throughput_mb_s:.1} MB/s, {rows_per_sec:.0} filas/s");

    // Nota: el archivo .csv equivalente debería estar en el mismo directorio con el
    // mismo nombre base para que la comparación de tamaño en disco tenga sentido.
    let csv_path = path.replace(".parquet", ".csv");
    if let Ok(csv_meta) = std::fs::metadata(&csv_path) {
        let csv_mb = csv_meta.len() as f64 / (1024.0 * 1024.0);
        println!("Tamaño en disco: {size_mb:.1} MB (Parquet) vs {csv_mb:.1} MB (CSV) -> {:.1}x mas chico", csv_mb / size_mb);
    }
}
