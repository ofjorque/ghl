//! Phase 3, Track 2, Point 2 (Case 1.1, Suite 01) — benchmarks native `dot()` (`faer`'s blocked
//! GEMM kernel via `RowRef * ColRef`, zero-copy over the `&[f64]` exposed by `VectorData`)
//! against an equivalent hand-rolled dot product on boxed `Vec<Value>` (one `Value::F64`
//! per cell) -- representing what `dot()` would have cost if implemented naively over
//! the legacy `Value::Vector` representation (Phase 3 Point 1).
//!
//! Usage: `cargo run --release --example spike_vector_dot_latency -p ghl-runtime -- <n>`

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

    println!("N = {n} elements");

    // Legacy path: two boxed Vec<Value> (one Value::F64 per cell), as dot() would have
    // performed naively on the pre-Point 1 Vector representation.
    let boxed_a = build_boxed(n);
    let boxed_b = build_boxed(n);

    let mut best_boxed = std::time::Duration::MAX;
    let mut boxed_result = 0.0;
    for _ in 0..REPEATS {
        let start = Instant::now();
        boxed_result = boxed_dot(&boxed_a, &boxed_b);
        best_boxed = best_boxed.min(start.elapsed());
    }
    println!("  dot() hand-rolled on boxed Vec<Value>: min over {REPEATS} runs: {best_boxed:>10.3?}");

    // New path: VectorData::from_f64 (unboxed) + MatrixOps::dot (faer, RowRef *
    // ColRef, the same blocked GEMM kernel used by matrix multiplication).
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
    println!("  dot() real (faer, VectorData::as_f64_view): min over {REPEATS} runs: {best_fast:>10.3?}");

    assert!(
        (boxed_result - fast_result).abs() < 1e-3 * boxed_result.abs().max(1.0),
        "both paths must produce (approximately) the same result: {boxed_result} vs {fast_result}"
    );

    if !best_fast.is_zero() {
        let speedup = best_boxed.as_secs_f64() / best_fast.as_secs_f64();
        println!("\nSpeedup: ~{speedup:.1}x");
    }
}
