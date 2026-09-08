//! Fase 1 (`benchmarks/suites/02`, Caso 2.1) — generador reproducible del CSV sintético
//! de gran escala: enteros, flotantes, strings y fechas (como texto — GHL no tiene un
//! tipo Date todavía), con NA y `NA:Razon` esparcidos para ejercitar el side-channel de
//! razones a escala real, no solo en pruebas de unidad con un puñado de filas.
//!
//! El archivo en sí NUNCA se versiona (TODO.md, Fase 1) — se regenera con este script.
//!
//! Uso: `cargo run --release --example generate_synthetic_csv -p ghl-runtime -- <rows> <out_path>`
//! Ejemplo para el caso completo de la Suite 02 (~25M filas, ~5GB):
//!   `cargo run --release --example generate_synthetic_csv -p ghl-runtime -- 25000000 target/synthetic_25m.csv`

use std::io::{BufWriter, Write};
use std::fs::File;

// PRNG determinista (mismo xorshift64 de `crates/ghl-runtime/src/io.rs::sample_indices`,
// no vale la pena una dependencia de `rand` solo para generar datos sintéticos).
fn xorshift_next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

const CATEGORIES: &[&str] = &["A", "B", "C", "D", "E"];
const STATUSES: &[&str] = &["OK", "OK", "OK", "OK", "WARN", "ERROR"]; // OK ponderado
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

    // 12 columnas mixtas, como pide el Caso 2.1: 3 enteras, 3 flotantes, 4 strings
    // (una de ellas fecha-como-texto), 2 booleanas.
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

        // ~4% NA sin razón, ~2% NA con razón, en las dos columnas "sensibles".
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
        "Generadas {rows} filas / 12 columnas en {out_path} ({:.1} MB) en {:.2?}",
        size as f64 / (1024.0 * 1024.0),
        elapsed
    );
}
