//! Fase 1 — mide `group_by() |> summarize()` a alta cardinalidad de grupos (el escenario
//! que la "optimización de seguimiento" de TODO.md apuntaba: muchas llamadas nativas
//! pequeñas, una por grupo por agregación, contra pocas llamadas nativas grandes, una
//! fusionada por `(kind, col)` sobre todos los grupos a la vez). `spike_summarize_latency`
//! ya mide el caso de baja cardinalidad (5 grupos, dataset de Suite 02 Caso 2.1); este
//! spike genera datos en memoria con `n_groups` grupos (por defecto 100.000, la escala que
//! TODO.md cita para Suite 02) para ver el mismo `df_summarize()` en el régimen donde la
//! fusión debería importar.
//!
//! Uso: `cargo run --release --example spike_summarize_high_cardinality -p ghl-runtime -- <rows> <n_groups>`

use std::time::Instant;
use ghl_runtime::io::{df_group_by, df_summarize};
use ghl_runtime::polars_bridge::build_dataframe;
use ghl_runtime::Value;

fn xorshift_next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn height_of(df: &Value) -> usize {
    match df {
        Value::DataFrame { frame, .. } => frame.height(),
        _ => 0,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rows: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2_000_000);
    let n_groups: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);

    let mut state: u64 = 0x9E37_79B9_7F4A_7C15 ^ rows;
    let mut group_col = Vec::with_capacity(rows as usize);
    let mut value_b = Vec::with_capacity(rows as usize);
    let mut value_c = Vec::with_capacity(rows as usize);
    for _ in 0..rows {
        let r1 = xorshift_next(&mut state);
        let r2 = xorshift_next(&mut state);
        group_col.push(Value::I64((r1 % n_groups) as i64));
        value_b.push(Value::F64((r1 % 10_000) as f64 / 7.0));
        value_c.push(Value::F64((r2 % 10_000) as f64 / 3.0));
    }
    let (frame, na_reasons) = build_dataframe(&[
        ("group_key".to_string(), group_col),
        ("value_b".to_string(), value_b),
        ("value_c".to_string(), value_c),
    ]).expect("build_dataframe should succeed");
    let df = Value::DataFrame { frame, na_reasons };

    println!("Dataset en memoria: {rows} filas, {n_groups} grupos");

    let specs = vec![
        ("n".to_string(), "count".to_string(), None),
        ("mean_value_b".to_string(), "mean".to_string(), Some("value_b".to_string())),
        ("max_value_b".to_string(), "max".to_string(), Some("value_b".to_string())),
        ("sum_value_c".to_string(), "sum".to_string(), Some("value_c".to_string())),
    ];

    let start = Instant::now();
    let grouped = df_group_by(&df, &["group_key".to_string()]).expect("group_by should succeed");
    let summary = df_summarize(&grouped, &specs).expect("summarize should succeed");
    let elapsed = start.elapsed();

    println!(
        "group_by(group_key) |> summarize(n, mean_value_b, max_value_b, sum_value_c): \
         {} filas -> {} grupos en {:.2?} ({:.0} filas/s)",
        rows,
        height_of(&summary),
        elapsed,
        rows as f64 / elapsed.as_secs_f64()
    );
}
