//! Fase 3, Track 2, Punto 2 (Caso 1.1, Suite 01) — mide `dot()` real (`faer`'s blocked
//! GEMM kernel vía `RowRef * ColRef`, zero-copy sobre el `&[f64]` que `VectorData`
//! expone) contra un dot product equivalente hecho a mano sobre `Vec<Value>` boxeado
//! (un `Value::F64` por celda) -- lo que este mismo `dot()` habría costado si se hubiera
//! escrito ingenuamente sobre la representación vieja de `Value::Vector` (Fase 3 Punto 1),
//! o lo que cuesta hoy cualquier reducción que todavía no migró (ver TODO.md).
//!
//! Uso: `cargo run --release --example spike_vector_dot_latency -p ghl-runtime -- <n>`

use std::time::Instant;
use ghl_runtime::matrix::MatrixOps;
use ghl_runtime::value::Value;
use ghl_runtime::vector_data::VectorData;

const REPEATS: u32 = 10;

fn build_boxed(n: usize) -> Vec<Value> {
    (0..n).map(|i| Value::F64(i as f64 * 0.5)).collect()
}

fn boxed_dot(a: &[Value], b: &[Value]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x.as_f64().unwrap_or(0.0) * y.as_f64().unwrap_or(0.0))
        .sum()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10_000_000);

    println!("N = {n} elementos");

    // Camino viejo: dos Vec<Value> boxeados (un Value::F64 por celda), como habría sido
    // dot() si se hubiera escrito ingenuamente sobre la representación de Vector previa
    // al Punto 1, o como sigue costando cualquier reducción que todavía no migró.
    let boxed_a = build_boxed(n);
    let boxed_b = build_boxed(n);

    let mut best_boxed = std::time::Duration::MAX;
    let mut boxed_result = 0.0;
    for _ in 0..REPEATS {
        let start = Instant::now();
        boxed_result = boxed_dot(&boxed_a, &boxed_b);
        best_boxed = best_boxed.min(start.elapsed());
    }
    println!("  dot() a mano sobre Vec<Value> boxeado:  min sobre {REPEATS} corridas: {best_boxed:>10.3?}");

    // Camino nuevo: VectorData::from_f64 (sin boxing) + MatrixOps::dot (faer, RowRef *
    // ColRef, el mismo kernel GEMM bloqueado que ya usa la multiplicación de matrices).
    let flat_a: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
    let flat_b: Vec<f64> = (0..n).map(|i| i as f64 * 0.5).collect();
    let vd_a = VectorData::from_f64(flat_a);
    let vd_b = VectorData::from_f64(flat_b);

    let mut best_fast = std::time::Duration::MAX;
    let mut fast_result = 0.0;
    for _ in 0..REPEATS {
        let view_a = vd_a.as_f64_view().expect("no nulls, should hit the fast path");
        let view_b = vd_b.as_f64_view().expect("no nulls, should hit the fast path");
        let start = Instant::now();
        fast_result = MatrixOps::dot(view_a.as_slice(), view_b.as_slice()).expect("equal lengths");
        best_fast = best_fast.min(start.elapsed());
    }
    println!("  dot() real (faer, VectorData::as_f64_view): min sobre {REPEATS} corridas: {best_fast:>10.3?}");

    assert!(
        (boxed_result - fast_result).abs() < 1e-3 * boxed_result.abs().max(1.0),
        "los dos caminos deben dar (aprox) el mismo resultado: {boxed_result} vs {fast_result}"
    );

    if !best_fast.is_zero() {
        let speedup = best_boxed.as_secs_f64() / best_fast.as_secs_f64();
        println!("\nSpeedup: ~{speedup:.1}x");
    }
}
