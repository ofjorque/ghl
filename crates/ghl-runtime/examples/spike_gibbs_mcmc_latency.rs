//! Fase 6, Caso 3.1 (Suite 03: Modelado Estadístico y Métodos MCMC)
//!
//! Benchmark / Spike del Muestreador MCMC Personalizado (Gibbs Sampler Jerárquico)
//! ejecutado íntegramente como programa GHL sobre el intérprete de `ghl-runtime`.
//!
//! Modelo:
//!   y_ij ~ Normal(mu_j, 1 / tau)
//!   mu_j ~ Normal(0, 1) [prior estándar]
//!   tau ~ Gamma(1, 1)   [prior estándar]
//!
//! En cada iteración:
//!   1. Actualización de cada mu_j condicional a los datos del grupo y tau:
//!      mu_j | y_j, tau ~ Normal( (sum(y_j)*tau)/(n_j*tau + 1), 1/sqrt(n_j*tau + 1) )
//!   2. Actualización de la precisión global tau condicional a los residuos:
//!      diff = y - get(mu_vec, groups)
//!      ssq = dot(diff, diff)
//!      tau | y, mu ~ Gamma(1 + N/2, 1 + ssq/2)
//!
//! Uso:
//!   cargo run --release --example spike_gibbs_mcmc_latency -p ghl-runtime -- [iteraciones] [n_por_grupo]

use std::time::Instant;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal as RNormal};
use rand_xoshiro::Xoshiro256PlusPlus;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_runtime::vector_data::VectorData;
use ghl_syntax::parser::parse;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let num_iterations: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(500);
    let n_per_group: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);

    println!("================================================================================");
    println!(" GHL Suite 03 - Caso 3.1: Hierarchical Gibbs Sampler");
    println!("================================================================================");
    println!("Configuración:");
    println!("  - Iteraciones MCMC: {num_iterations}");
    println!("  - N por grupo:      {n_per_group} (3 grupos, N_total = {})", n_per_group * 3);
    println!("  - Parámetros reales: mu = [3.0, 8.0, -4.0], sigma = 0.8 (tau ~ 1.5625)");

    // 1. Generar datos sintéticos en Rust
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

    // 2. Definir el programa completo en GHL
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

    let program = parse(ghl_code).expect("Código GHL parseado con éxito");

    let mut interp = Interpreter::new();
    interp.env.set("y".to_string(), Value::Vector(VectorData::from_f64(y)));
    interp.env.set(
        "groups".to_string(),
        Value::Vector(VectorData::from_values(groups.into_iter().map(Value::I64).collect())),
    );
    interp.env.set("iterations".to_string(), Value::I64(num_iterations));

    println!("\nIniciando muestreo Gibbs en GHL...");
    let start = Instant::now();
    interp.eval_program(&program).expect("Evaluación GHL exitosa");
    let elapsed = start.elapsed();

    let m0 = interp.env.get("mean0").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
    let m1 = interp.env.get("mean1").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
    let m2 = interp.env.get("mean2").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);

    let per_iter = elapsed.as_secs_f64() / (num_iterations as f64);
    let iter_per_sec = (num_iterations as f64) / elapsed.as_secs_f64();

    println!("\nResultados del muestreador:");
    println!("  - Media posterior mu[0]: {:>8.4} (esperada: ~3.0000, error: {:>+.4})", m0, m0 - 3.0);
    println!("  - Media posterior mu[1]: {:>8.4} (esperada: ~8.0000, error: {:>+.4})", m1, m1 - 8.0);
    println!("  - Media posterior mu[2]: {:>8.4} (esperada: ~-4.0000, error: {:>+.4})", m2, m2 - (-4.0));

    println!("\nRendimiento:");
    println!("  - Tiempo total:          {elapsed:?}");
    println!("  - Tiempo por iteración:  {:>8.3} µs", per_iter * 1_000_000.0);
    println!("  - Rendimiento:           {:>8.1} iteraciones/segundo", iter_per_sec);
    println!("================================================================================");
}
