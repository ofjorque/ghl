//! Comprehensive test suite for the Probability & Inference Subsystem (`std::prob` in pure GHL).
//! RFC 15 / Roadmap 08 Parte E: First-Class Probability Distributions.

mod common;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

#[test]
fn test_normal_distribution_struct_methods() {
    let code = r#"
        let dist = normal(0.0, 1.0);
        let p0 = dist.pdf(0.0);
        let c_crit = dist.cdf(1.95996398454);
        let q_crit = dist.quantile(0.975);
        let sample = dist.sample_seed(200, 42);
        let len_sample = length(sample);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let p0 = interp.env.get("p0").unwrap().as_f64().unwrap();
    let c_crit = interp.env.get("c_crit").unwrap().as_f64().unwrap();
    let q_crit = interp.env.get("q_crit").unwrap().as_f64().unwrap();
    let len_sample = interp.env.get("len_sample").unwrap().as_f64().unwrap();

    assert!((p0 - 0.39894228).abs() < 1e-4);
    assert!((c_crit - 0.975).abs() < 1e-4);
    assert!((q_crit - 1.95996398).abs() < 1e-4);
    assert_eq!(len_sample, 200.0);
}

#[test]
fn test_student_t_and_fisher_f_struct_methods() {
    let code = r#"
        let t = student_t(10.0);
        let t_p0 = t.pdf(0.0);
        let t_cdf0 = t.cdf(0.0);
        let t_q975 = t.quantile(0.975);

        let f = fisher_f(5.0, 10.0);
        let f_q95 = f.quantile(0.95);
        let f_cdf_crit = f.cdf(f_q95);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let t_p0 = interp.env.get("t_p0").unwrap().as_f64().unwrap();
    let t_cdf0 = interp.env.get("t_cdf0").unwrap().as_f64().unwrap();
    let t_q975 = interp.env.get("t_q975").unwrap().as_f64().unwrap();

    assert!(t_p0 > 0.35 && t_p0 < 0.40);
    assert!((t_cdf0 - 0.5).abs() < 1e-7);
    assert!((t_q975 - 2.22813885).abs() < 1e-4);

    let f_cdf_crit = interp.env.get("f_cdf_crit").unwrap().as_f64().unwrap();
    assert!((f_cdf_crit - 0.95).abs() < 1e-5);
}

#[test]
fn test_chisq_gamma_beta_uniform_exponential() {
    let code = r#"
        let cs = chisq(4.0);
        let cs_q95 = cs.quantile(0.95);
        let cs_c = cs.cdf(cs_q95);

        let g = gamma_dist(2.0, 1.0);
        let g_c = g.cdf(2.0);

        let b = beta_dist(2.0, 2.0);
        let b_med = b.quantile(0.5);

        let u = uniform(10.0, 20.0);
        let u_c = u.cdf(15.0);
        let u_q = u.quantile(0.5);

        let e = exponential(2.0);
        let e_c = e.cdf(0.5);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let cs_c = interp.env.get("cs_c").unwrap().as_f64().unwrap();
    assert!((cs_c - 0.95).abs() < 1e-5);

    let b_med = interp.env.get("b_med").unwrap().as_f64().unwrap();
    assert!((b_med - 0.5).abs() < 1e-5);

    let u_c = interp.env.get("u_c").unwrap().as_f64().unwrap();
    let u_q = interp.env.get("u_q").unwrap().as_f64().unwrap();
    assert!((u_c - 0.5).abs() < 1e-6);
    assert!((u_q - 15.0).abs() < 1e-6);

    let e_c = interp.env.get("e_c").unwrap().as_f64().unwrap();
    // For Exp(rate=2) at x=0.5: cdf = 1 - exp(-2 * 0.5) = 1 - e^(-1) ~ 0.63212
    assert!((e_c - 0.6321205588).abs() < 1e-4);
}

#[test]
fn test_discrete_binomial_and_poisson() {
    let code = r#"
        let bin = binomial(10.0, 0.5);
        let bin_pmf5 = bin.pmf(5.0);
        let bin_cdf5 = bin.cdf(5.0);

        let poi = poisson_dist(4.0);
        let poi_pmf4 = poi.pmf(4.0);
        let poi_cdf4 = poi.cdf(4.0);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let bin_pmf5 = interp.env.get("bin_pmf5").unwrap().as_f64().unwrap();
    // Binomial(10, 0.5) at k=5: 252 / 1024 ~ 0.24609375
    assert!((bin_pmf5 - 0.24609375).abs() < 1e-4);

    let poi_pmf4 = interp.env.get("poi_pmf4").unwrap().as_f64().unwrap();
    // Poisson(lambda=4) at k=4: 4^4 * e^(-4) / 24 = 256 / 24 * e^(-4) ~ 0.1953668
    assert!((poi_pmf4 - 0.1953668).abs() < 1e-4);
}

#[test]
fn test_inference_p_value_and_confidence_interval() {
    let code = r#"
        let std_norm = normal(0.0, 1.0);
        let p_two = p_value(1.95996398454, std_norm, "two_sided");
        let p_right = p_value(1.64485362695, std_norm, "greater");
        let p_left = p_value(-1.64485362695, std_norm, "less");

        let ci = conf_int(100.0, 2.0, std_norm, 0.95);
        let low = ci.conf_low;
        let high = ci.conf_high;
        let td = tidy(ci);
        let gl = glance(ci);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let p_two = interp.env.get("p_two").unwrap().as_f64().unwrap();
    let p_right = interp.env.get("p_right").unwrap().as_f64().unwrap();
    let p_left = interp.env.get("p_left").unwrap().as_f64().unwrap();

    assert!((p_two - 0.05).abs() < 1e-4);
    assert!((p_right - 0.05).abs() < 1e-4);
    assert!((p_left - 0.05).abs() < 1e-4);

    let low = interp.env.get("low").unwrap().as_f64().unwrap();
    let high = interp.env.get("high").unwrap().as_f64().unwrap();

    // Estimate = 100, se = 2, crit ~ 1.96 => 100 +- 3.92 => [96.08, 103.92]
    assert!((low - 96.08007).abs() < 1e-3);
    assert!((high - 103.91993).abs() < 1e-3);

    let td_val = interp.env.get("td").unwrap();
    assert!(matches!(td_val, Value::DataFrame { .. }));
    let gl_val = interp.env.get("gl").unwrap();
    assert!(matches!(gl_val, Value::DataFrame { .. }));
}

#[test]
fn test_z_test_and_cor_test() {
    let code = r#"
        let x = [102.0, 98.0, 101.0, 99.0, 100.0];
        let z_res = z_test_one_sample(x, 100.0, 2.0);
        let z_stat = z_res.statistic;
        let z_pval = z_res.p_value;

        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let b = [2.0, 3.9, 6.1, 8.0, 9.9, 12.1];
        let cor_res = cor_test(a, b);
        let r_est = cor_res.estimate;
        let cor_pval = cor_res.p_value;
        let cor_td = tidy(cor_res);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let z_stat = interp.env.get("z_stat").unwrap().as_f64().unwrap();
    let z_pval = interp.env.get("z_pval").unwrap().as_f64().unwrap();

    // Sample mean is exactly 100.0, mu = 100.0 => z_stat = 0.0, p_val = 1.0
    assert!(z_stat.abs() < 1e-6);
    assert!((z_pval - 1.0).abs() < 1e-6);

    let r_est = interp.env.get("r_est").unwrap().as_f64().unwrap();
    let cor_pval = interp.env.get("cor_pval").unwrap().as_f64().unwrap();

    // a and b are almost perfectly linearly correlated: r > 0.999
    assert!(r_est > 0.999, "Expected r > 0.999, got {r_est}");
    assert!(
        cor_pval < 0.0001,
        "Expected highly significant correlation, got {cor_pval}"
    );

    let cor_td_val = interp.env.get("cor_td").unwrap();
    assert!(matches!(cor_td_val, Value::DataFrame { .. }));
}
