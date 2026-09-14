//! Phase 6, Case 3.2 -- benchmarks IRLS logistic regression matrix solver efficiency
//! and numerical stability at N=1,000,000 observations x P=40 continuous predictors.
//!
//! Two empirical measurements:
//!   1. Complete fit (`FittedGlm::fit_logistic`) at full scale: total time,
//!      IRLS iterations until convergence, and coefficient recovery error.
//!   2. Assembly of `X^T W X` / `X^T W z` (the O(n*p^2) step repeated every IRLS iteration,
//!      parallelized with rayon above `PARALLEL_THRESHOLD`) -- implemented locally in both
//!      sequential and parallel variants with real converged weights.
//!
//! Usage: `cargo run --release --example spike_irls_latency -p ghl-runtime`

use std::time::Instant;
use polars_core::prelude::*;
use rand::{RngExt, SeedableRng};
use rayon::prelude::*;
use ghl_runtime::na_reasons::NaReasonTable;
use ghl_runtime::{Blueprint, FittedGlm, Interpreter, Value};
use ghl_syntax::parser::parse;

const N: usize = 1_000_000;
const P_PREDICTORS: usize = 40;

/// Unlike `lib.rs`'s test-only `vector_f64` (fine at N=8,000), `items.iter()` on a
/// `Value::Vector` here would `Deref` through `VectorData` to its lazily-materialized
/// `Vec<Value>` cache -- boxing all 1M cells (160 bytes each) *before* this benchmark's
/// own NEKO fast path even runs, silently reintroducing the exact cost this benchmark
/// exists to measure the absence of. `as_f64_view()` (same fast path NEKO's `bake()` now
/// uses) reads `random_normal`'s already-`Float64`, no-NA output with zero boxing.
fn vector_f64(v: &Value) -> Vec<f64> {
    match v {
        Value::Vector(vd) => vd.as_f64_view().expect("random_normal output is numeric").as_slice().to_vec(),
        other => panic!("Expected Vector, found {other:?}"),
    }
}

fn sigmoid_stable(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

/// Sequential implementation of `assemble_weighted_normal_equations` (`neko.rs`,
/// `pub(crate)`) -- identical algebra (`X^T W X`, `X^T W z`), self-contained here to
/// avoid exposing internal crate helper visibility solely for benchmarking.
fn assemble_seq(n: usize, p: usize, x_data: &[f64], weights: &[f64], target: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut xtwx = vec![0.0; p * p];
    let mut xtwt = vec![0.0; p];
    for i in 0..n {
        let row = &x_data[i * p..(i + 1) * p];
        let w = weights[i];
        let t = target[i];
        for a in 0..p {
            xtwt[a] += w * row[a] * t;
            for b in 0..p {
                xtwx[a * p + b] += w * row[a] * row[b];
            }
        }
    }
    (xtwx, xtwt)
}

/// Same algebra partitioned with rayon: each parallel task accumulates a local
/// p*p/p buffer across a slice of rows (`fold`), followed by a tree reduction (`reduce`),
/// avoiding lock contention across worker threads.
fn assemble_par(n: usize, p: usize, x_data: &[f64], weights: &[f64], target: &[f64]) -> (Vec<f64>, Vec<f64>) {
    (0..n)
        .into_par_iter()
        .fold(
            || (vec![0.0_f64; p * p], vec![0.0_f64; p]),
            |(mut xtwx, mut xtwt), i| {
                let row = &x_data[i * p..(i + 1) * p];
                let w = weights[i];
                let t = target[i];
                for a in 0..p {
                    xtwt[a] += w * row[a] * t;
                    for b in 0..p {
                        xtwx[a * p + b] += w * row[a] * row[b];
                    }
                }
                (xtwx, xtwt)
            },
        )
        .reduce(
            || (vec![0.0_f64; p * p], vec![0.0_f64; p]),
            |(mut xtwx_a, mut xtwt_a), (xtwx_b, xtwt_b)| {
                for k in 0..xtwx_a.len() {
                    xtwx_a[k] += xtwx_b[k];
                }
                for k in 0..xtwt_a.len() {
                    xtwt_a[k] += xtwt_b[k];
                }
                (xtwx_a, xtwt_a)
            },
        )
}

fn main() {
    println!("Threads available for rayon: {}", rayon::current_num_threads());
    println!("N = {N}, P = {P_PREDICTORS} continuous predictors\n");

    // 1. Predictors generated via seeded GHL random_normal through the interpreter.
    let gen_code: String = (1..=P_PREDICTORS)
        .map(|j| format!("let x{j} = random_normal({N}, 0.0, 1.0, {seed});\n", seed = 1000 + j))
        .collect();
    let program = parse(&gen_code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let predictors: Vec<Vec<f64>> = (1..=P_PREDICTORS)
        .map(|j| vector_f64(&interp.env.get(&format!("x{j}")).unwrap()))
        .collect();
    drop(interp);

    // 2. Fixed true beta and Bernoulli response generated from the true model.
    let true_beta: Vec<f64> = (0..=P_PREDICTORS)
        .map(|j| if j == 0 { 0.2 } else { 0.3 * if j % 2 == 0 { 1.0 } else { -1.0 } / (j as f64).sqrt() })
        .collect();

    let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(777);
    let mut y = Vec::with_capacity(N);
    for i in 0..N {
        let mut eta = true_beta[0];
        for j in 0..P_PREDICTORS {
            eta += true_beta[j + 1] * predictors[j][i];
        }
        let p = sigmoid_stable(eta);
        y.push(if rng.random::<f64>() < p { 1.0 } else { 0.0 });
    }

    // 3. Build DataFrame consumed by Blueprint::bake directly without intermediate boxing.
    let mut predictors = predictors;
    let term_names: Vec<String> = (1..=P_PREDICTORS).map(|j| format!("x{j}")).collect();
    let mut columns: Vec<Column> = Vec::with_capacity(P_PREDICTORS + 1);
    columns.push(Float64Chunked::from_vec(PlSmallStr::from_static("y"), y).into_series().into());
    for (j, name) in term_names.iter().enumerate() {
        let col = std::mem::take(&mut predictors[j]);
        columns.push(Float64Chunked::from_vec(PlSmallStr::from_string(name.clone()), col).into_series().into());
    }
    drop(predictors);
    let frame = DataFrame::new_infer_height(columns).expect("well-formed equal-length f64 columns");
    let na_reasons = NaReasonTable::new();

    let blueprint = Blueprint::new("y".to_string(), term_names.clone());

    // 4. Benchmark full-scale IRLS fit.
    let start = Instant::now();
    let fit = FittedGlm::fit_logistic(blueprint, &frame, &na_reasons).expect("IRLS converges");
    let elapsed = start.elapsed();
    drop(frame);

    println!("=== Full Fit (fit_logistic) ===");
    println!("  Total time:       {elapsed:>10.3?}");
    println!("  IRLS iterations:  {}", fit.iterations);
    println!("  n_obs:            {}", fit.n_obs);
    let max_beta_err = fit
        .coefficients
        .iter()
        .zip(&true_beta)
        .map(|(&est, &truth)| (est - truth).abs())
        .fold(0.0_f64, f64::max);
    println!("  max |beta_est - beta_true|: {max_beta_err:.4} (numerical stability)");

    // 5. Benchmark sequential vs parallel assembly with converged weights.
    let p_full = P_PREDICTORS + 1; // +intercept
    let weights: Vec<f64> = fit.fitted_values.iter().map(|&mu| (mu * (1.0 - mu)).max(1e-10)).collect();
    let target = &fit.fitted_values;

    const REPEATS: u32 = 5;
    let mut t_seq = std::time::Duration::MAX;
    let mut t_par = std::time::Duration::MAX;
    let mut xtwx_seq = Vec::new();
    let mut xtwx_par = Vec::new();
    for r in 0..REPEATS {
        if r % 2 == 0 {
            let start = Instant::now();
            let (x, _) = assemble_seq(N, p_full, &fit.x_data, &weights, target);
            t_seq = t_seq.min(start.elapsed());
            xtwx_seq = x;

            let start = Instant::now();
            let (x, _) = assemble_par(N, p_full, &fit.x_data, &weights, target);
            t_par = t_par.min(start.elapsed());
            xtwx_par = x;
        } else {
            let start = Instant::now();
            let (x, _) = assemble_par(N, p_full, &fit.x_data, &weights, target);
            t_par = t_par.min(start.elapsed());
            xtwx_par = x;

            let start = Instant::now();
            let (x, _) = assemble_seq(N, p_full, &fit.x_data, &weights, target);
            t_seq = t_seq.min(start.elapsed());
            xtwx_seq = x;
        }
    }

    let max_diff = xtwx_seq
        .iter()
        .zip(&xtwx_par)
        .map(|(&a, &b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    assert!(max_diff < 1e-6, "seq and par must match, max_diff={max_diff}");

    println!("\n=== Assembly X^T W X / X^T W z (typical IRLS iteration) ===");
    println!("  Sequential:  {t_seq:>10.3?}");
    println!("  Parallel:    {t_par:>10.3?}");
    let speedup = t_seq.as_secs_f64() / t_par.as_secs_f64().max(1e-12);
    println!("  Parallel vs Sequential Speedup: ~{speedup:.2}x");
}
