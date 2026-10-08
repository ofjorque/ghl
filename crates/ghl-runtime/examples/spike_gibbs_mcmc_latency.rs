//! Phase 6, Case 3.1 (Suite 03: Statistical Modeling and MCMC Methods)
//!
//! Benchmark / Spike for the Custom MCMC Sampler (Hierarchical Gibbs Sampler)
//! executed entirely as a GHL program on the `ghl-runtime` interpreter.
//!
//! Model:
//!   y_ij ~ Normal(mu_j, 1 / tau)
//!   mu_j ~ Normal(0, 1) [standard prior]
//!   tau ~ Gamma(1, 1)   [standard prior]
//!
//! In each iteration:
//!   1. Update each mu_j conditional on group data and tau:
//!      mu_j | y_j, tau ~ Normal( (sum(y_j)*tau)/(n_j*tau + 1), 1/sqrt(n_j*tau + 1) )
//!   2. Update global precision tau conditional on residuals:
//!      diff = y - get(mu_vec, groups)
//!      ssq = dot(diff, diff)
//!      tau | y, mu ~ Gamma(1 + N/2, 1 + ssq/2)
//!
//! Usage:
//!   cargo run --release --example spike_gibbs_mcmc_latency -p ghl-runtime -- [iterations] [n_per_group]

use rand::SeedableRng;
use rand_distr::{Distribution, Normal as RNormal};
use rand_xoshiro::Xoshiro256PlusPlus;
use std::time::Instant;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_runtime::vector_data::VectorData;
use ghl_syntax::parser::parse;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let num_iterations: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(500);
    let n_per_group: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);

    println!("================================================================================");
    println!(" GHL Suite 03 - Case 3.1: Hierarchical Gibbs Sampler");
    println!("================================================================================");
    println!("Configuration:");
    println!("  - MCMC Iterations: {num_iterations}");
    println!(
        "  - N per group:     {n_per_group} (3 groups, N_total = {})",
        n_per_group * 3
    );
    println!("  - True parameters: mu = [3.0, 8.0, -4.0], sigma = 0.8 (tau ~ 1.5625)");

    // 1. Generate synthetic data in Rust
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(42);
    let true_means = [3.0, 8.0, -4.0];
    let mut y = Vec::with_capacity(n_per_group * 3);
    let mut groups = Vec::with_capacity(n_per_group * 3);

    for (g, &m) in true_means.iter().enumerate() {
        let dist = RNormal::new(m, 0.8).unwrap();
        for _ in 0..n_per_group {
            y.push(dist.sample(&mut rng));
            groups.push(g as i64);
        }
    }

    // 2. Define the full GHL program
    let ghl_code = r#"
        fn run_gibbs(y, groups, num_iterations) {
            let num_groups = max(groups) + 1;
            let mut trace = zeros(num_iterations, num_groups);
            let mut tau = 1.0;
            let mut mu_vec = zeros(num_groups);
            let n_total = len(y);

            let mut iter = 0;
            while iter < num_iterations {
                let mut j = 0;
                while j < num_groups {
                    let y_j = filter(y, groups == j);
                    let n_j = len(y_j);
                    let post_mean = (sum(y_j) * tau) / (n_j * tau + 1.0);
                    let post_sd = 1.0 / sqrt(n_j * tau + 1.0);
                    let draw = first(random_normal(1, post_mean, post_sd));
                    mu_vec = set(mu_vec, j, draw);
                    j = j + 1;
                };

                let diff = y - get(mu_vec, groups);
                let ssq = dot(diff, diff);
                let alpha_post = 1.0 + n_total / 2.0;
                let beta_post = 1.0 + ssq / 2.0;
                tau = first(random_gamma(1, alpha_post, beta_post));

                trace = set_row(trace, iter, mu_vec);
                iter = iter + 1;
            };

            trace
        }

        let trace = run_gibbs(y, groups, iterations);
        let col0 = get_col(trace, 0);
        let col1 = get_col(trace, 1);
        let col2 = get_col(trace, 2);
        let mean0 = mean(col0);
        let mean1 = mean(col1);
        let mean2 = mean(col2);
    "#;

    let program = parse(ghl_code).expect("GHL code parsed successfully");

    let mut interp = Interpreter::new();
    interp
        .env
        .set("y".to_string(), Value::Vector(VectorData::from_f64(y)));
    interp.env.set(
        "groups".to_string(),
        Value::Vector(VectorData::from_values(
            groups.into_iter().map(Value::I64).collect(),
        )),
    );
    interp
        .env
        .set("iterations".to_string(), Value::I64(num_iterations));

    println!("\nStarting Gibbs sampling in GHL...");
    let start = Instant::now();
    interp
        .eval_program(&program)
        .expect("GHL evaluation successful");
    let elapsed = start.elapsed();

    let m0 = interp
        .env
        .get("mean0")
        .and_then(|v| v.as_f64())
        .unwrap_or(f64::NAN);
    let m1 = interp
        .env
        .get("mean1")
        .and_then(|v| v.as_f64())
        .unwrap_or(f64::NAN);
    let m2 = interp
        .env
        .get("mean2")
        .and_then(|v| v.as_f64())
        .unwrap_or(f64::NAN);

    let per_iter = elapsed.as_secs_f64() / (num_iterations as f64);
    let iter_per_sec = (num_iterations as f64) / elapsed.as_secs_f64();

    println!("\nSampler Results:");
    println!(
        "  - Posterior mean mu[0]: {:>8.4} (expected: ~3.0000, error: {:>+.4})",
        m0,
        m0 - 3.0
    );
    println!(
        "  - Posterior mean mu[1]: {:>8.4} (expected: ~8.0000, error: {:>+.4})",
        m1,
        m1 - 8.0
    );
    println!(
        "  - Posterior mean mu[2]: {:>8.4} (expected: ~-4.0000, error: {:>+.4})",
        m2,
        m2 - (-4.0)
    );

    println!("\nPerformance:");
    println!("  - Total time:          {elapsed:?}");
    println!(
        "  - Time per iteration:  {:>8.3} µs",
        per_iter * 1_000_000.0
    );
    println!(
        "  - Throughput:          {:>8.1} iterations/second",
        iter_per_sec
    );
    println!("================================================================================");
}
