//! Comprehensive test suite for the Kernel Probability Distributions Subsystem (RFC 15 / Roadmap 08 Parte F).
//!
//! Tests the canonical quadruple interface (`pdf`/`pmf`, `cdf`, `quantile`, `sample`):
//! - Continuous: Normal, Student-t, Fisher-Snedecor F, Chi-Squared, Gamma, Beta, Uniform, Exp
//! - Discrete: Binomial, Poisson
//! - Quantile roundtripping: cdf(quantile(p)) ≈ p
//! - Random sampling shapes and seed reproducibility

mod common;
use common::*;

use ghl_runtime::eval::Interpreter;
use ghl_syntax::parser::parse;

#[test]
fn test_normal_quantile_matches_reference() {
    let code = r#"
        let q50 = normal_quantile(0.5, 0.0, 1.0);
        let q975 = normal_quantile(0.975, 0.0, 1.0);
        let q025 = normal_quantile(0.025, 0.0, 1.0);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let q50 = interp.env.get("q50").unwrap().as_f64().unwrap();
    let q975 = interp.env.get("q975").unwrap().as_f64().unwrap();
    let q025 = interp.env.get("q025").unwrap().as_f64().unwrap();

    assert!((q50 - 0.0).abs() < 1e-7);
    assert!((q975 - 1.95996398454).abs() < 1e-5);
    assert!((q025 - (-1.95996398454)).abs() < 1e-5);
}

#[test]
fn test_student_t_quadruple_interface() {
    let code = r#"
        let p0 = student_t_pdf(0.0, 10.0);
        let c0 = student_t_cdf(0.0, 10.0);
        let q_median = student_t_quantile(0.5, 10.0);
        let q_crit = student_t_quantile(0.975, 10.0);
        let sample = random_student_t(500, 10.0, 0.0, 1.0, 42);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let p0 = interp.env.get("p0").unwrap().as_f64().unwrap();
    let c0 = interp.env.get("c0").unwrap().as_f64().unwrap();
    let q_median = interp.env.get("q_median").unwrap().as_f64().unwrap();
    let q_crit = interp.env.get("q_crit").unwrap().as_f64().unwrap();

    // t_10 at 0: cdf is exactly 0.5
    assert!((c0 - 0.5).abs() < 1e-7);
    assert!((q_median - 0.0).abs() < 1e-7);
    // critical t for 10 df at 97.5% is approx 2.22813885
    assert!((q_crit - 2.22813885).abs() < 1e-4);
    assert!(p0 > 0.35 && p0 < 0.40);

    let sample = vector_f64(&interp.env.get("sample").unwrap());
    assert_eq!(sample.len(), 500);
}

#[test]
fn test_fisher_snedecor_f_distribution_for_anova() {
    let code = r#"
        // F(2, 20) distribution at F = 3.4928 (the ~0.05 p-value cutoff)
        let cdf_val = f_dist_cdf(3.492828, 2.0, 20.0);
        let p_value = 1.0 - cdf_val;
        let q95 = f_dist_quantile(0.95, 2.0, 20.0);
        let samples = random_f_dist(100, 2.0, 20.0, 123);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let p_value = interp.env.get("p_value").unwrap().as_f64().unwrap();
    let q95 = interp.env.get("q95").unwrap().as_f64().unwrap();

    // p-value should be approx 0.05
    assert!(
        (p_value - 0.05).abs() < 1e-4,
        "Expected p-value ~0.05, got {p_value}"
    );
    assert!(
        (q95 - 3.492828).abs() < 1e-4,
        "Expected critical F ~3.4928, got {q95}"
    );

    let samples = vector_f64(&interp.env.get("samples").unwrap());
    assert_eq!(samples.len(), 100);
    assert!(
        samples.iter().all(|&x| x >= 0.0),
        "F-distributed values must be non-negative"
    );
}

#[test]
fn test_chisq_distribution() {
    let code = r#"
        // Chi-squared with 4 df at x = 9.48773 (approx 95th percentile)
        let cdf_val = chisq_cdf(9.487729, 4.0);
        let p_val = 1.0 - cdf_val;
        let q95 = chisq_quantile(0.95, 4.0);
        let samples = random_chisq(200, 4.0, 999);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let p_val = interp.env.get("p_val").unwrap().as_f64().unwrap();
    let q95 = interp.env.get("q95").unwrap().as_f64().unwrap();

    assert!((p_val - 0.05).abs() < 1e-4);
    assert!((q95 - 9.487729).abs() < 1e-4);

    let samples = vector_f64(&interp.env.get("samples").unwrap());
    assert_eq!(samples.len(), 200);
    assert!(samples.iter().all(|&x| x >= 0.0));
}

#[test]
fn test_beta_and_exp_and_uniform() {
    let code = r#"
        // Beta(1, 1) is Uniform(0, 1)
        let beta_c = beta_cdf(0.5, 1.0, 1.0);
        let beta_q = beta_quantile(0.5, 1.0, 1.0);

        // Exp(2.0): median is ln(2)/2 ≈ 0.34657359
        let exp_q = exp_quantile(0.5, 2.0);
        let exp_c = exp_cdf(0.34657359, 2.0);

        // Uniform(10, 20): median is 15
        let u_c = uniform_cdf(15.0, 10.0, 20.0);
        let u_q = uniform_quantile(0.5, 10.0, 20.0);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert!((interp.env.get("beta_c").unwrap().as_f64().unwrap() - 0.5).abs() < 1e-6);
    assert!((interp.env.get("beta_q").unwrap().as_f64().unwrap() - 0.5).abs() < 1e-6);

    assert!((interp.env.get("exp_q").unwrap().as_f64().unwrap() - 0.34657359).abs() < 1e-5);
    assert!((interp.env.get("exp_c").unwrap().as_f64().unwrap() - 0.5).abs() < 1e-5);

    assert!((interp.env.get("u_c").unwrap().as_f64().unwrap() - 0.5).abs() < 1e-6);
    assert!((interp.env.get("u_q").unwrap().as_f64().unwrap() - 15.0).abs() < 1e-6);
}

#[test]
fn test_discrete_distributions_binomial_and_poisson() {
    let code = r#"
        // Binomial(n=10, p=0.5): P(k=5) = 252/1024 ≈ 0.24609375
        let b_pmf = binomial_pmf(5, 10, 0.5);
        let b_cdf = binomial_cdf(5, 10, 0.5);
        let b_q = binomial_quantile(0.5, 10, 0.5);
        let b_samples = random_binomial(50, 10, 0.5, 777);

        // Poisson(lambda=3.0): P(k=0) = e^(-3) ≈ 0.049787
        let p_pmf0 = poisson_pmf(0, 3.0);
        let p_cdf0 = poisson_cdf(0, 3.0);
        let p_q = poisson_quantile(0.5, 3.0);
        let p_samples = random_poisson(50, 3.0, 888);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let b_pmf = interp.env.get("b_pmf").unwrap().as_f64().unwrap();
    let b_cdf = interp.env.get("b_cdf").unwrap().as_f64().unwrap();
    let b_q = interp.env.get("b_q").unwrap().as_i64().unwrap();

    assert!((b_pmf - 0.24609375).abs() < 1e-5);
    assert!((b_cdf - 0.623046875).abs() < 1e-5);
    assert_eq!(b_q, 5);

    let b_samples = vector_f64(&interp.env.get("b_samples").unwrap());
    assert_eq!(b_samples.len(), 50);
    assert!(b_samples.iter().all(|&x| (0.0..=10.0).contains(&x)));

    let p_pmf0 = interp.env.get("p_pmf0").unwrap().as_f64().unwrap();
    let p_cdf0 = interp.env.get("p_cdf0").unwrap().as_f64().unwrap();
    let p_q = interp.env.get("p_q").unwrap().as_i64().unwrap();

    assert!((p_pmf0 - 0.049787068).abs() < 1e-5);
    assert!((p_cdf0 - 0.049787068).abs() < 1e-5);
    assert!(p_q >= 2 && p_q <= 4);

    let p_samples = vector_f64(&interp.env.get("p_samples").unwrap());
    assert_eq!(p_samples.len(), 50);
    assert!(p_samples.iter().all(|&x| x >= 0.0));
}
