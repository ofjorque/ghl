//! Fase 1 (`benchmarks/suites/02`, Caso 2.3) — mide `filter()` vectorizado (comparación
//! nativa de polars sobre la columna, sin boxear a `Vec<Value>`) sobre un DataFrame real
//! de escala, para verificar con datos reales que el cambio valió la pena en vez de
//! asumirlo.
//!
//! Uso: `cargo run --release --example spike_filter_latency -p ghl-runtime -- <parquet_path>`

use std::time::Instant;
use ghl_syntax::ast::BinaryOp;
use ghl_runtime::io::{read_parquet_file, df_filter_by_col_predicate};
use ghl_runtime::polars_bridge::pull_column_as_values;
use ghl_runtime::Value;

/// Reconstruye, solo para este spike, el costo que tenía la implementación anterior de
/// `filter()`: boxear la columna entera a `Vec<Value>` y comparar celda por celda en
/// Rust -- para tener un antes/después real, no solo el número absoluto del camino nuevo.
/// No mide el paso final de reconstruir el DataFrame (`take_rows` no es público), pero
/// ese paso es idéntico en ambos caminos -- lo que cambió es exactamente esto.
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
        eprintln!("No se pudo leer {path}: {e:?}");
        eprintln!("Generalo primero (ver generate_synthetic_csv + write_parquet).");
        std::process::exit(1);
    });
    let total_rows = height_of(&df);
    println!("Archivo: {path} ({total_rows} filas)");

    // Numérico: value_a > 500000 (~50% de las filas, comparación i64 vectorizada).
    let start = Instant::now();
    let numeric_filtered = df_filter_by_col_predicate(&df, "value_a", BinaryOp::Gt, &Value::I64(500_000))
        .expect("numeric filter should succeed");
    let elapsed = start.elapsed();
    println!(
        "\n[nuevo]  value_a > 500000: {} de {} filas en {:.2?} ({:.0} filas/s)",
        height_of(&numeric_filtered),
        total_rows,
        elapsed,
        total_rows as f64 / elapsed.as_secs_f64()
    );

    // Mismo predicado con el costo de la implementación anterior (boxear + comparar
    // celda por celda) para tener el antes/después real, no solo el número nuevo.
    let start = Instant::now();
    let naive_indices = naive_predicate_indices(&df, "value_a", 500_000.0);
    let elapsed = start.elapsed();
    println!(
        "[viejo]  value_a > 500000: {} de {} filas en {:.2?} ({:.0} filas/s) -- solo el cómputo del mask/indices, sin reconstruir el DataFrame",
        naive_indices.len(),
        total_rows,
        elapsed,
        total_rows as f64 / elapsed.as_secs_f64()
    );

    // String: category == "A" (~20% de las filas, comparación de strings vectorizada).
    let start = Instant::now();
    let string_filtered = df_filter_by_col_predicate(&df, "category", BinaryOp::Eq, &Value::String("A".into()))
        .expect("string filter should succeed");
    let elapsed = start.elapsed();
    println!(
        "[category == \"A\"] {} de {} filas en {:.2?} ({:.0} filas/s)",
        height_of(&string_filtered),
        total_rows,
        elapsed,
        total_rows as f64 / elapsed.as_secs_f64()
    );
}
