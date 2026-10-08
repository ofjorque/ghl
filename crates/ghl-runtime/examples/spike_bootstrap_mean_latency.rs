//! Phase 4, point (c) (Case 3.3, Suite 03) — benchmarks sequential `bootstrap_mean` (a
//! simple `for` loop instead of `into_par_iter()`, identical computation) against the parallel
//! rayon implementation. Each replica maintains a single `f64` accumulator over the shared
//! `&[f64]` slice without materializing O(n) buffers per replica — total memory is ~O(n + n_replicas).
//!
//! Case 3.3 specifies 20,000 replicas x N=100,000 (2x10^9 draws). Default benchmark size is
//! 2,000 x 100,000 with linear extrapolation to estimate the full workload.
//!
//! Usage: `cargo run --release --example spike_bootstrap_mean_latency -p ghl-runtime -- <n> <n_replicas>`

use rand::RngExt;
use rayon::prelude::*;
use std::time::Instant;

fn sequential_bootstrap_means(base: &[f64], n_replicas: usize) -> Vec<f64> {
    let n = base.len();
    (0..n_replicas)
        .map(|_| {
            let mut rng = rand::rng();
            let mut acc = 0.0;
            for _ in 0..n {
                let idx: usize = rng.random_range(0..n);
                acc += base[idx];
            }
            acc / n as f64
        })
        .collect()
}

fn parallel_bootstrap_means(base: &[f64], n_replicas: usize) -> Vec<f64> {
    let n = base.len();
    (0..n_replicas)
        .into_par_iter()
        .map(|_| {
            let mut rng = rand::rng();
            let mut acc = 0.0;
            for _ in 0..n {
                let idx: usize = rng.random_range(0..n);
                acc += base[idx];
            }
            acc / n as f64
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let n_replicas: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2_000);

    println!("N (base sample size) = {n}");
    println!("n_replicas = {n_replicas}");
    println!(
        "Threads available for rayon: {}",
        rayon::current_num_threads()
    );

    let base: Vec<f64> = (0..n).map(|i| i as f64).collect();

    let start = Instant::now();
    let seq = sequential_bootstrap_means(&base, n_replicas);
    let t_seq = start.elapsed();
    println!("Sequential: {t_seq:>10.3?}");

    let start = Instant::now();
    let par = parallel_bootstrap_means(&base, n_replicas);
    let t_par = start.elapsed();
    println!("Parallel:   {t_par:>10.3?}");

    assert_eq!(seq.len(), n_replicas);
    assert_eq!(par.len(), n_replicas);
    let true_mean = (n - 1) as f64 / 2.0;
    let seq_grand_mean: f64 = seq.iter().sum::<f64>() / seq.len() as f64;
    let par_grand_mean: f64 = par.iter().sum::<f64>() / par.len() as f64;
    println!(
        "Base true mean: {true_mean:.3} | Sequential grand mean: {seq_grand_mean:.3} | Parallel grand mean: {par_grand_mean:.3}"
    );

    let speedup = t_seq.as_secs_f64() / t_par.as_secs_f64().max(1e-12);
    println!("\nParallel vs Sequential Speedup: ~{speedup:.2}x");

    let full_scale_replicas = 20_000usize;
    let full_scale_n = 100_000usize;
    let scale_factor =
        (full_scale_replicas as f64 / n_replicas as f64) * (full_scale_n as f64 / n as f64);
    let est_seq_full = t_seq.as_secs_f64() * scale_factor;
    let est_par_full = t_par.as_secs_f64() * scale_factor;
    println!("\nLinear extrapolation to full scale Case 3.3 (20,000 replicas x N=100,000):");
    println!(
        "  Estimated sequential: ~{est_seq_full:.1}s | Estimated parallel: ~{est_par_full:.1}s"
    );
}
