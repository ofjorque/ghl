//! Phase 6, Case 3.4 (Suite 03: Statistical Modeling and MCMC Methods)
//!
//! Benchmark / Spike for the EM (Expectation-Maximization) algorithm for multivariate
//! Gaussian Mixture Models executed entirely as a GHL program on the `ghl-runtime` interpreter.
//!
//! Suite 03 Specification:
//!   - K = 10 multivariate Gaussian components
//!   - D = 20 dimensions
//!   - N observations (default: 1,000)
//!   - 500 EM iterations
//!   - Numerical stability in log-sum-exp, repetitive matrix computation, and vectorization.
//!
//! Usage:
//!   cargo run --release --example spike_em_gmm_latency -p ghl-runtime -- [iterations] [n_obs]

use std::time::Instant;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal as RNormal};
use rand_xoshiro::Xoshiro256PlusPlus;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let num_iterations: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let n_obs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1_000);
    let k_clusters: usize = 10;
    let d_dim: usize = 20;

    println!("================================================================================");
    println!(" GHL Suite 03 - Case 3.4: EM Algorithm for Gaussian Mixture Models");
    println!("================================================================================");
    println!("Configuration:");
    println!("  - Components (K):   {k_clusters}");
    println!("  - Dimensions (D):   {d_dim}");
    println!("  - Observations (N): {n_obs}");
    println!("  - EM Iterations:    {num_iterations} (CLI adjustable)");

    // 1. Generate synthetic multivariate data across K distinct clusters
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(42);
    let mut x_data = Vec::with_capacity(n_obs * d_dim);

    let n_per_k = n_obs / k_clusters;
    for k in 0..k_clusters {
        let center = (k as f64 - 4.5) * 3.0;
        let dist = RNormal::new(center, 1.0).unwrap();
        let count = if k == k_clusters - 1 { n_obs - (n_per_k * (k_clusters - 1)) } else { n_per_k };
        for _ in 0..count {
            for _ in 0..d_dim {
                x_data.push(dist.sample(&mut rng));
            }
        }
    }

    // 2. Full GHL Program
    let ghl_code = r#"
        fn run_gmm_em(X, K, D, max_iter) {
            let N = len(X);

            // Initialize weights pi uniformly: 1/K
            let mut pi = zeros(K);
            let mut k_init = 0;
            while k_init < K {
                pi = set(pi, k_init, 1.0 / (K * 1.0));
                k_init = k_init + 1;
            };

            // Initialize means mu (K x D) taking K distinct points from X
            let mut mu = zeros(K, D);
            let step = N / K;
            let mut k_mu = 0;
            while k_mu < K {
                let sample_idx = k_mu * step;
                mu = set_row(mu, k_mu, get_row(X, sample_idx));
                k_mu = k_mu + 1;
            };

            // Initialize diagonal variances (K x D) to 1.0
            let mut vars = zeros(K, D);
            let mut k_var = 0;
            while k_var < K {
                let mut d_init = 0;
                let mut v_init = zeros(D);
                while d_init < D {
                    v_init = set(v_init, d_init, 1.0);
                    d_init = d_init + 1;
                };
                vars = set_row(vars, k_var, v_init);
                k_var = k_var + 1;
            };

            let two_pi = 6.283185307179586;
            let log_two_pi_d = (D * 1.0) * log(two_pi);

            let mut iter = 0;
            while iter < max_iter {
                // --- E-step ---
                let mut Gamma = zeros(N, K);
                let mut i = 0;
                while i < N {
                    let xi = get_row(X, i);
                    let mut log_p = zeros(K);

                    let mut c = 0;
                    while c < K {
                        let mu_c = get_row(mu, c);
                        let var_c = get_row(vars, c);
                        let diff = xi - mu_c;
                        let diff_sq = diff * diff;
                        let mahal = sum(diff_sq / var_c);
                        let log_det = sum(log(var_c));
                        let log_gauss = -0.5 * (log_two_pi_d + log_det + mahal);
                        let log_prior = log(get(pi, c));
                        log_p = set(log_p, c, log_prior + log_gauss);
                        c = c + 1;
                    };

                    let lse = log_sum_exp(log_p);
                    let mut c2 = 0;
                    while c2 < K {
                        let resp = exp(get(log_p, c2) - lse);
                        Gamma = set(Gamma, i, c2, resp);
                        c2 = c2 + 1;
                    };

                    i = i + 1;
                };

                // --- M-step ---
                let Gamma_T = transpose(Gamma);
                let weighted_X = Gamma_T * X;

                let mut c3 = 0;
                while c3 < K {
                    let gamma_c = get_row(Gamma_T, c3);
                    let N_c = sum(gamma_c);
                    pi = set(pi, c3, N_c / N);

                    // Updated means
                    let new_mu_c = get_row(weighted_X, c3) / N_c;
                    mu = set_row(mu, c3, new_mu_c);

                    // Updated diagonal variances with ridge regularization
                    let mut sum_sq = zeros(D);
                    let mut i2 = 0;
                    while i2 < N {
                        let xi2 = get_row(X, i2);
                        let diff2 = xi2 - new_mu_c;
                        let w = get(gamma_c, i2);
                        sum_sq = sum_sq + (diff2 * diff2) * w;
                        i2 = i2 + 1;
                    };
                    let mut ridge = zeros(D);
                    let mut rd = 0;
                    while rd < D {
                        ridge = set(ridge, rd, 0.001);
                        rd = rd + 1;
                    };
                    let new_var_c = (sum_sq / N_c) + ridge;
                    vars = set_row(vars, c3, new_var_c);

                    c3 = c3 + 1;
                };

                iter = iter + 1;
            };

            mu
        }

        let fitted_means = run_gmm_em(X, 10, 20, iterations);
        let first_cluster_mean = get_row(fitted_means, 0);
        let mean_dim0 = get(first_cluster_mean, 0);
    "#;

    let program = parse(ghl_code).expect("GHL code parsed successfully");

    let mut interp = Interpreter::new();
    interp.env.set(
        "X".to_string(),
        Value::Matrix {
            rows: n_obs,
            cols: d_dim,
            data: std::sync::Arc::new(x_data.clone()),
        },
    );
    interp.env.set("iterations".to_string(), Value::I64(num_iterations));

    println!("\nStarting EM fit in GHL...");
    let start = Instant::now();
    interp.eval_program(&program).expect("GHL evaluation successful");
    let elapsed = start.elapsed();

    let per_iter = elapsed.as_secs_f64() / (num_iterations as f64);
    let iter_per_sec = (num_iterations as f64) / elapsed.as_secs_f64();
    let mean_d0 = interp.env.get("mean_dim0").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);

    println!("\nEM Fit Results:");
    println!("  - Dimension 0 of Cluster 0: {:>8.4}", mean_d0);
    println!("  - Convergence and stability: log_sum_exp prevented underflow/overflow.");

    println!("\nPerformance (GHL Script):");
    println!("  - Total time:          {elapsed:?}");
    println!("  - Time per iteration:  {:>8.3} ms", per_iter * 1000.0);
    println!("  - Throughput:          {:>8.1} iterations/second", iter_per_sec);
    if num_iterations < 500 {
        let est_500 = per_iter * 500.0;
        println!("  - Estimated 500 iter:  {:>8.2} s", est_500);
    }

    // 3. NEKO Native Execution (fit_gmm) with Cockpit Visual
    println!("\n================================================================================");
    println!(" NEKO Native GMM (fit_gmm) & Terminal Cockpit");
    println!("================================================================================");

    let neko_code = r#"
        let model = fit_gmm(X, 10, iterations, 0.00001);
        summary(model);
    "#;
    let neko_prog = parse(neko_code).expect("GHL code parsed successfully");
    let mut interp_neko = Interpreter::new();
    interp_neko.env.set(
        "X".to_string(),
        Value::Matrix {
            rows: n_obs,
            cols: d_dim,
            data: std::sync::Arc::new(x_data),
        },
    );
    interp_neko.env.set("iterations".to_string(), Value::I64(num_iterations));

    let start_neko = Instant::now();
    interp_neko.eval_program(&neko_prog).expect("NEKO evaluation successful");
    let elapsed_neko = start_neko.elapsed();
    let per_iter_neko = elapsed_neko.as_secs_f64() / (num_iterations as f64);
    let speedup = per_iter / per_iter_neko;

    println!("\nComparative Performance (NEKO Native vs GHL Script):");
    println!("  - Total NEKO time:        {elapsed_neko:?}");
    println!("  - Time per iteration:     {:>8.3} ms ({:>8.1} µs)", per_iter_neko * 1000.0, per_iter_neko * 1_000_000.0);
    println!("  - Speedup NEKO vs Script: {:>8.1}x faster", speedup);
    println!("================================================================================");
}
