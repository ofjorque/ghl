//! Phase 3, Track 2, Point 3 (Case 1.4, Suite 01) — benchmarks the reference expression
//! `log(1.0 + exp(-abs(xi))) + sin(xi)` via the pre-`map()` path: chaining existing
//! vectorized helpers (`abs` -> `exp` -> scalar add -> `log` -> `sin` -> final add),
//! requiring six full passes over the vector with intermediate `Vector` buffers,
//! against the `map()` approach with a single pass.
//!
//! The input vector is injected directly into the interpreter environment (`interp.env.set`)
//! instead of writing an N-element `.gh` literal to avoid parsing overhead dominating the benchmark.
//!
//! The speedup here comes from eliminating intermediate `Vector` allocations per sub-operation.
//!
//! Default N is 2M elements to avoid memory exhaustion from allocating 6 full vectors concurrently.
//!
//! Usage: `cargo run --release --example spike_map_fusion_latency -p ghl-runtime -- <n>`

use std::time::Instant;
use ghl_syntax::parser::parse;
use ghl_runtime::value::Value;
use ghl_runtime::vector_data::VectorData;
use ghl_runtime::Interpreter;

const REPEATS: u32 = 5;

fn run(code: &str, base_vector: &VectorData) -> std::time::Duration {
    let program = parse(code).expect("syntax ok");
    let mut best = std::time::Duration::MAX;
    for _ in 0..REPEATS {
        let mut interp = Interpreter::new();
        // Clones `VectorData` (cheap Arc clone), avoiding copying the underlying `Vec<f64>`.
        interp.env.set("x".to_string(), Value::Vector(base_vector.clone()));
        let start = Instant::now();
        interp.eval_program(&program).expect("evaluation ok");
        best = best.min(start.elapsed());
    }
    best
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2_000_000);

    println!("N = {n} elements");

    let xs: Vec<f64> = (0..n).map(|i| (i as f64 * 0.001).sin() * 3.0).collect();
    let base_vector = VectorData::from_f64(xs);

    let chained = r#"
        let a = abs(x);
        let b = exp(a);
        let c = b + 1.0;
        let d = log(c);
        let e = sin(x);
        let y = d .+ e;
    "#;
    let fused = r#"
        let y = map(x, \xi -> log(1.0 + exp(-abs(xi))) + sin(xi));
    "#;

    let best_chained = run(chained, &base_vector);
    println!("  Chained (6 full Vectors, legacy path): min over {REPEATS} runs: {best_chained:>10.3?}");

    let best_fused = run(fused, &base_vector);
    println!("  Fused map() (1 pass, new path):        min over {REPEATS} runs: {best_fused:>10.3?}");

    if !best_fused.is_zero() {
        let speedup = best_chained.as_secs_f64() / best_fused.as_secs_f64();
        println!("\nSpeedup: ~{speedup:.2}x");
    }
}
