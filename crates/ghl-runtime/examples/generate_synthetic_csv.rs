//! Phase 1 (`benchmarks/suites/02`, Case 2.1) — reproducible generator for large-scale
//! synthetic CSV: integers, floats, strings, and dates (as text — GHL does not have a
//! dedicated Date type yet), with NA and `NA:Reason` scattered to exercise the reason
//! side-channel at scale, not just in unit tests with a handful of rows.
//!
//! The generated file itself is NEVER committed to git — it is generated with this script.
//!
//! Usage: `cargo run --release --example generate_synthetic_csv -p ghl-runtime -- <rows> <out_path>`
//! Example for the full Suite 02 case (~25M rows, ~5GB):
//!   `cargo run --release --example generate_synthetic_csv -p ghl-runtime -- 25000000 target/synthetic_25m.csv`

use std::io::{BufWriter, Write};
use std::fs::File;

// Deterministic PRNG (same xorshift64 as `crates/ghl-runtime/src/io.rs::sample_indices`,
// avoiding a dedicated `rand` dependency just for synthetic data generation).
fn xorshift_next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

const CATEGORIES: &[&str] = &["A", "B", "C", "D", "E"];
const STATUSES: &[&str] = &["OK", "OK", "OK", "OK", "WARN", "ERROR"]; // Weighted OK
const NA_REASONS: &[&str] = &["SensorDropout", "LowBattery", "Timeout", "OutOfRange"];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rows: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
    let out_path = args.get(2).cloned().unwrap_or_else(|| "target/synthetic.csv".to_string());

    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(parent).expect("could not create output directory");
    }

    let file = File::create(&out_path).expect("could not create output CSV");
    let mut w = BufWriter::with_capacity(8 * 1024 * 1024, file);

    // 12 mixed columns as required by Case 2.1: 3 integers, 3 floats, 4 strings
    // (one of them date-as-text), 2 booleans.
    writeln!(
        w,
        "id,value_a,value_b,value_c,category,name,flag_a,flag_b,event_date,status,notes,score"
    )
    .unwrap();

    let mut state: u64 = 0x9E37_79B9_7F4A_7C15 ^ rows;
    let start = std::time::Instant::now();

    for i in 0..rows {
        let r1 = xorshift_next(&mut state);
        let r2 = xorshift_next(&mut state);
        let r3 = xorshift_next(&mut state);

        let value_a = (r1 % 1_000_000) as i64;
        let value_b = (r1 as f64 % 10_000.0) / 7.0;
        let value_c = (r2 as f64 % 10_000.0) / 3.0;
        let category = CATEGORIES[(r1 as usize) % CATEGORIES.len()];
        let flag_a = r2 % 2 == 0;
        let flag_b = r3 % 3 == 0;
        let day = 1 + (r2 % 28);
        let month = 1 + (r3 % 12);
        let year = 2020 + (r1 % 6);
        let status = STATUSES[(r2 as usize) % STATUSES.len()];

        // ~4% NA without reason, ~2% NA with reason, across the two sensitive columns.
        let notes_bucket = r3 % 100;
        let notes = if notes_bucket < 2 {
            format!("NA:{}", NA_REASONS[(r1 as usize) % NA_REASONS.len()])
        } else if notes_bucket < 6 {
            "NA".to_string()
        } else {
            format!("note-{}", r3 % 50)
        };

        let score_bucket = r1 % 100;
        let score = if score_bucket < 3 {
            format!("NA:{}", NA_REASONS[(r2 as usize) % NA_REASONS.len()])
        } else {
            format!("{:.4}", (r3 as f64 % 100.0) / 1.7)
        };

        writeln!(
            w,
            "{id},{value_a},{value_b:.4},{value_c:.4},{category},name-{id},{flag_a},{flag_b},{year:04}-{month:02}-{day:02},{status},{notes},{score}",
            id = i,
        )
        .unwrap();
    }

    w.flush().unwrap();
    let elapsed = start.elapsed();
    let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    println!(
        "Generated {rows} rows / 12 columns in {out_path} ({:.1} MB) in {:.2?}",
        size as f64 / (1024.0 * 1024.0),
        elapsed
    );
}
