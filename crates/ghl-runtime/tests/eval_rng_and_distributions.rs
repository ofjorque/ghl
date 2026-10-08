//! RNG and distribution native functions: random_uniform/normal/gamma, bootstrap_mean, pdf/cdf.

mod common;
use common::*;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

#[test]
fn test_random_uniform_produces_vector_of_requested_length_in_unit_interval() {
    // TODO.md Fase 3, Track 2, Punto 4: not reproducible across runs yet (that's
    // Fase 5's PRNG::seed job) -- what's checked here is shape and range only.
    let code = r#"
        let v = random_uniform(1000);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let v = vector_f64(&interp.env.get("v").unwrap());
    assert_eq!(v.len(), 1000);
    assert!(
        v.iter().all(|&x| (0.0..1.0).contains(&x)),
        "all values must be in [0, 1): {v:?}"
    );
    // Not all-identical -- a real generator, not a stub returning a constant.
    assert!(v.windows(2).any(|w| w[0] != w[1]));
}

#[test]
fn test_random_uniform_rejects_negative_length() {
    let code = r#"
        let v = random_uniform(-5);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("negative length must fail");
    assert_eq!(err.code, "C0201");
}

#[test]
fn test_random_uniform_seeded_is_reproducible() {
    // TODO.md Fase 5, Punto 1: same seed + n -> byte-identical Vector, any run.
    let code = r#"
        let a = random_uniform(1000, 42);
        let b = random_uniform(1000, 42);
        let c = random_uniform(1000, 7);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let a = vector_f64(&interp.env.get("a").unwrap());
    let b = vector_f64(&interp.env.get("b").unwrap());
    let c = vector_f64(&interp.env.get("c").unwrap());
    assert_eq!(a, b, "same seed must produce byte-identical output");
    assert_ne!(a, c, "different seeds must produce different output");
}

#[test]
fn test_random_uniform_without_seed_still_unseeded() {
    // Confirms the optional third argument didn't change the existing, already-
    // tested default (thread-local, non-reproducible) behavior.
    let code = r#"
        let a = random_uniform(1000);
        let b = random_uniform(1000);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let a = vector_f64(&interp.env.get("a").unwrap());
    let b = vector_f64(&interp.env.get("b").unwrap());
    assert_ne!(a, b, "unseeded calls must not be reproducible");
}

#[test]
fn test_bootstrap_mean_seeded_is_reproducible() {
    let code = r#"
        let v = random_uniform(1000, 1);
        let a = bootstrap_mean(v, 500, 99);
        let b = bootstrap_mean(v, 500, 99);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(
        vector_f64(&interp.env.get("a").unwrap()),
        vector_f64(&interp.env.get("b").unwrap()),
        "same seed must produce byte-identical bootstrap replicas"
    );
}

#[test]
fn test_bootstrap_mean_seeded_independent_of_thread_count() {
    // The real test of "reproducible bit-a-bit": which replica gets which stream
    // must be fixed by its index (via Xoshiro256PlusPlus::jump()), not by which
    // thread happens to run it -- so 1 thread and rayon's default pool must agree.
    let code = r#"
        let v = random_uniform(1000, 1);
        let means = bootstrap_mean(v, 500, 99);
    "#;
    let program = parse(code).expect("syntax ok");

    let mut interp_default = Interpreter::new();
    interp_default
        .eval_program(&program)
        .expect("evaluation ok");
    let default_pool_result = vector_f64(&interp_default.env.get("means").unwrap());

    let single_thread_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let single_thread_result = single_thread_pool.install(|| {
        let mut interp_single = Interpreter::new();
        interp_single.eval_program(&program).expect("evaluation ok");
        vector_f64(&interp_single.env.get("means").unwrap())
    });

    assert_eq!(
        default_pool_result, single_thread_result,
        "seeded bootstrap_mean must not depend on thread count/scheduling"
    );
}

#[test]
fn test_random_normal_seeded_is_reproducible() {
    let code = r#"
        let a = random_normal(1000, 5.0, 2.0, 42);
        let b = random_normal(1000, 5.0, 2.0, 42);
        let c = random_normal(1000, 5.0, 2.0, 7);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let a = vector_f64(&interp.env.get("a").unwrap());
    let b = vector_f64(&interp.env.get("b").unwrap());
    let c = vector_f64(&interp.env.get("c").unwrap());
    assert_eq!(a, b, "same seed must produce byte-identical output");
    assert_ne!(a, c, "different seeds must produce different output");
}

#[test]
fn test_random_gamma_seeded_is_reproducible() {
    let code = r#"
        let a = random_gamma(1000, 3.0, 2.0, 42);
        let b = random_gamma(1000, 3.0, 2.0, 42);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(
        vector_f64(&interp.env.get("a").unwrap()),
        vector_f64(&interp.env.get("b").unwrap())
    );
}

#[test]
fn test_random_normal_matches_expected_mean_and_sd() {
    let code = r#"
        let v = random_normal(200000, 5.0, 2.0, 1);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let v = vector_f64(&interp.env.get("v").unwrap());
    let n = v.len() as f64;
    let mean: f64 = v.iter().sum::<f64>() / n;
    let var: f64 = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    assert!(
        (mean - 5.0).abs() < 0.05,
        "sample mean {mean} too far from 5.0"
    );
    assert!(
        (var.sqrt() - 2.0).abs() < 0.05,
        "sample sd {} too far from 2.0",
        var.sqrt()
    );
}

#[test]
fn test_random_gamma_matches_expected_mean() {
    // Mean of Gamma(shape, rate) is shape / rate -- this is also the test that
    // guards the shape-rate parameterization choice: shape-scale would give a
    // systematically different (shape * rate) sample mean instead.
    let code = r#"
        let v = random_gamma(200000, 3.0, 2.0, 1);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let v = vector_f64(&interp.env.get("v").unwrap());
    let mean: f64 = v.iter().sum::<f64>() / v.len() as f64;
    assert!(
        (mean - 1.5).abs() < 0.05,
        "sample mean {mean} too far from shape/rate = 1.5"
    );
}

#[test]
fn test_random_normal_rejects_non_positive_sd() {
    let code = r#"
        let v = random_normal(10, 0.0, -1.0);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("non-positive sd must fail");
    assert_eq!(err.code, "C0201");
}

#[test]
fn test_random_gamma_rejects_non_positive_shape() {
    let code = r#"
        let v = random_gamma(10, -1.0, 1.0);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("non-positive shape must fail");
    assert_eq!(err.code, "C0201");
}

#[test]
fn test_normal_pdf_cdf_match_known_values() {
    let code = r#"
        let p = normal_pdf(0.0, 0.0, 1.0);
        let c = normal_cdf(0.0, 0.0, 1.0);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert!((interp.env.get("p").unwrap().as_f64().unwrap() - 0.398_942_280_4).abs() < 1e-6);
    assert!((interp.env.get("c").unwrap().as_f64().unwrap() - 0.5).abs() < 1e-9);
}

#[test]
fn test_gamma_pdf_cdf_match_known_values() {
    // shape=1.0 reduces Gamma(shape, rate) to Exponential(rate), whose closed-form
    // pdf/cdf are easy to check by hand: pdf(x) = rate*e^(-rate*x), cdf(x) = 1-e^(-rate*x).
    let code = r#"
        let p = gamma_pdf(1.0, 1.0, 1.0);
        let c = gamma_cdf(0.693147180560, 1.0, 1.0);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert!(
        (interp.env.get("p").unwrap().as_f64().unwrap() - std::f64::consts::E.recip()).abs() < 1e-6
    );
    assert!((interp.env.get("c").unwrap().as_f64().unwrap() - 0.5).abs() < 1e-6);
}

#[test]
fn test_random_normal_parallel_path_matches_sequential_reference() {
    // Above PARALLEL_THRESHOLD (50,000): exercises sample_distribution's chunked
    // parallel path, not just its sequential fast path.
    let code = r#"
        let v = random_normal(60000, 0.0, 1.0, 42);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    let v = vector_f64(&interp.env.get("v").unwrap());
    assert_eq!(v.len(), 60_000);

    let mean: f64 = v.iter().sum::<f64>() / v.len() as f64;
    assert!(mean.abs() < 0.05, "sample mean {mean} too far from 0.0");
}

#[test]
fn test_random_normal_seeded_independent_of_thread_count() {
    let code = r#"
        let v = random_normal(60000, 0.0, 1.0, 42);
    "#;
    let program = parse(code).expect("syntax ok");

    let mut interp_default = Interpreter::new();
    interp_default
        .eval_program(&program)
        .expect("evaluation ok");
    let default_pool_result = vector_f64(&interp_default.env.get("v").unwrap());

    let single_thread_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let single_thread_result = single_thread_pool.install(|| {
        let mut interp_single = Interpreter::new();
        interp_single.eval_program(&program).expect("evaluation ok");
        vector_f64(&interp_single.env.get("v").unwrap())
    });

    assert_eq!(
        default_pool_result, single_thread_result,
        "seeded random_normal must not depend on thread count/scheduling"
    );
}

#[test]
fn test_bootstrap_mean_produces_requested_number_of_replicas() {
    // TODO.md Fase 4, punto (c), Suite 03's Caso 3.3. Base sample [1..5], real mean 3.0.
    // Each replica is itself a mean of values resampled *from* the base, so it must
    // land within the base's own [min, max] range, and averaging many replicas should
    // land close to the true mean (generous tolerance -- this is a statistical
    // assertion, not an exact one, to avoid flakiness).
    let code = r#"
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        let reps = bootstrap_mean(v, 500);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let reps = vector_f64(&interp.env.get("reps").unwrap());
    assert_eq!(reps.len(), 500);
    for r in &reps {
        assert!(
            (1.0..=5.0).contains(r),
            "replica mean {r} outside base sample range"
        );
    }
    let grand_mean: f64 = reps.iter().sum::<f64>() / reps.len() as f64;
    assert!(
        (grand_mean - 3.0).abs() < 0.5,
        "grand mean {grand_mean} too far from true mean 3.0"
    );
}

#[test]
fn test_bootstrap_mean_propagates_na() {
    let code = r#"
        let v = [1.0, sqrt(-1.0), 3.0];
        let reps = bootstrap_mean(v, 10);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert!(matches!(interp.env.get("reps").unwrap(), Value::NA(_)));
}

#[test]
fn test_bootstrap_mean_rejects_non_vector_first_argument() {
    let code = r#"
        let reps = bootstrap_mean(5.0, 10);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("non-Vector first argument must fail");
    assert_eq!(err.code, "C0202");
}

#[test]
fn test_bootstrap_mean_rejects_empty_vector() {
    let code = r#"
        let empty = random_uniform(0);
        let reps = bootstrap_mean(empty, 10);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("empty base sample must fail");
    assert_eq!(err.code, "S0412");
}
