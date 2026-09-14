//! Phase 1 (`benchmarks/README.md` mentions Parquet alongside CSV) — benchmarks `read_parquet_file`
//! on the same data (converted previously with `write_parquet`), providing real Parquet vs CSV
//! comparison data instead of assuming "Parquet is always faster".
//!
//! Usage: `cargo run --release --example spike_parquet_ingest_latency -p ghl-runtime -- <parquet_path>`

use std::time::Instant;
use ghl_runtime::io::read_parquet_file;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| "target/synthetic.parquet".to_string());

    let size_mb = std::fs::metadata(&path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or_else(|e| {
            eprintln!("Could not read {path}: {e:?}");
            eprintln!("Generate it first: read the CSV with read_csv() and save it with write_parquet() via a .gh script");
            std::process::exit(1);
        });

    println!("File: {path} ({size_mb:.1} MB)");

    let start = Instant::now();
    let df = read_parquet_file(&path).expect("Parquet read should succeed");
    let elapsed = start.elapsed();

    let height = match &df {
        ghl_runtime::Value::DataFrame { frame, .. } => frame.height(),
        _ => 0,
    };
    let throughput_mb_s = size_mb / elapsed.as_secs_f64();
    let rows_per_sec = height as f64 / elapsed.as_secs_f64();

    println!("Read {height} rows in {elapsed:.2?}");
    println!("Throughput: {throughput_mb_s:.1} MB/s, {rows_per_sec:.0} rows/s");

    // Note: the equivalent .csv file should be in the same directory with the
    // same base name for disk size comparison to be meaningful.
    let csv_path = path.replace(".parquet", ".csv");
    if let Ok(csv_meta) = std::fs::metadata(&csv_path) {
        let csv_mb = csv_meta.len() as f64 / (1024.0 * 1024.0);
        println!("Disk size: {size_mb:.1} MB (Parquet) vs {csv_mb:.1} MB (CSV) -> {:.1}x smaller", csv_mb / size_mb);
    }
}
