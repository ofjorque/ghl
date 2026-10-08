//! Comprehensive test suite for Self-Hosted Statistical Estimators (RFC 15 / Roadmap 08 Parte F.2).
//!
//! Tests:
//! - `anova(formula, df)` in pure GHL (`stats.gh`)
//! - `t_test(x, y)` (Welch two-sample) in pure GHL
//! - `t_test_one_sample(x, mu)` in pure GHL
//! - `chisq_test(observed)` in pure GHL
//! - First-class results with `tidy()`, `glance()`, and `summary()` method dispatch
//! - DataFrame column access (`df.col` and `df["col"]`)

mod common;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

#[test]
fn test_dataframe_column_field_access_and_indexing() {
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0],
            y: [10.0, 20.0, 30.0, 40.0]
        };
        let col_dot = df.x;
        let col_bracket_str = df["y"];
        let col_bracket_idx = df[0];
        let m_x = mean(col_dot);
        let m_y = mean(col_bracket_str);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("m_x").unwrap().as_f64().unwrap(), 2.5);
    assert_eq!(interp.env.get("m_y").unwrap().as_f64().unwrap(), 25.0);
}

#[test]
fn test_anova_self_hosted() {
    let code = r#"
        let df = dataframe {
            y: [2.0, 3.0, 5.0, 4.0, 6.0, 8.0],
            x: [1.0, 1.0, 2.0, 2.0, 3.0, 3.0]
        };
        let res = anova(y ~ x, df);
        let td = tidy(res);
        let gl = glance(res);
        let sm = summary(res);
        let f_stat = res.f_stat;
        let p_val = res.p_value;
        let r2 = res.r2;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let f_stat = interp.env.get("f_stat").unwrap().as_f64().unwrap();
    let p_val = interp.env.get("p_val").unwrap().as_f64().unwrap();
    let r2 = interp.env.get("r2").unwrap().as_f64().unwrap();

    assert!(f_stat > 0.0, "f_stat should be positive, got {f_stat}");
    assert!(
        p_val >= 0.0 && p_val <= 1.0,
        "p_val should be in [0, 1], got {p_val}"
    );
    assert!(r2 > 0.0 && r2 <= 1.0, "r2 should be in (0, 1], got {r2}");

    let td_val = interp.env.get("td").unwrap();
    assert!(
        matches!(td_val, Value::DataFrame { .. }),
        "tidy(res) must return a DataFrame"
    );

    let gl_val = interp.env.get("gl").unwrap();
    assert!(
        matches!(gl_val, Value::DataFrame { .. }),
        "glance(res) must return a DataFrame"
    );
}

#[test]
fn test_welch_t_test_self_hosted() {
    let code = r#"
        let x = [10.0, 12.0, 13.0, 11.0, 14.0];
        let y = [20.0, 22.0, 21.0, 23.0, 24.0];
        let res = t_test(x, y);
        let t_stat = res.statistic;
        let p_val = res.p_value;
        let est = res.estimate;
        let td = tidy(res);
        let gl = glance(res);
        let sm = summary(res);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let t_stat = interp.env.get("t_stat").unwrap().as_f64().unwrap();
    let p_val = interp.env.get("p_val").unwrap().as_f64().unwrap();
    let est = interp.env.get("est").unwrap().as_f64().unwrap();

    // Group 1 mean = 12, Group 2 mean = 22 => estimate = -10
    assert!((est - (-10.0)).abs() < 1e-6);
    // t_stat should be strongly negative (means are 12 vs 22, very separated)
    assert!(
        t_stat < -5.0,
        "Expected strongly negative t-statistic, got {t_stat}"
    );
    assert!(
        p_val < 0.001,
        "Expected highly significant p-value, got {p_val}"
    );

    let td_val = interp.env.get("td").unwrap();
    assert!(
        matches!(td_val, Value::DataFrame { .. }),
        "tidy(res) must return a DataFrame"
    );
}

#[test]
fn test_one_sample_t_test_self_hosted() {
    let code = r#"
        let x = [10.0, 10.5, 9.5, 10.2, 9.8];
        let res = t_test_one_sample(x, 10.0);
        let t_stat = res.statistic;
        let p_val = res.p_value;
        let est = res.estimate;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let t_stat = interp.env.get("t_stat").unwrap().as_f64().unwrap();
    let p_val = interp.env.get("p_val").unwrap().as_f64().unwrap();
    let est = interp.env.get("est").unwrap().as_f64().unwrap();

    // Sample mean = 10.0, mu = 10.0 => diff = 0, t_stat ~ 0, p_val ~ 1
    assert!((est - 10.0).abs() < 1e-6);
    assert!(t_stat.abs() < 0.1);
    assert!(p_val > 0.8);
}

#[test]
fn test_chisq_test_self_hosted() {
    let code = r#"
        let obs = [20.0, 20.0, 20.0];
        let res_uniform = chisq_test(obs);
        let stat_uniform = res_uniform.statistic;
        let p_uniform = res_uniform.p_value;

        let obs_skewed = [10.0, 20.0, 60.0];
        let res_skewed = chisq_test(obs_skewed);
        let stat_skewed = res_skewed.statistic;
        let p_skewed = res_skewed.p_value;

        let td = tidy(res_skewed);
        let gl = glance(res_skewed);
        let sm = summary(res_skewed);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let stat_uniform = interp.env.get("stat_uniform").unwrap().as_f64().unwrap();
    let p_uniform = interp.env.get("p_uniform").unwrap().as_f64().unwrap();

    // For perfectly equal observed counts, chi-sq statistic should be exactly 0
    assert!(stat_uniform.abs() < 1e-6);
    assert!((p_uniform - 1.0).abs() < 1e-6);

    let stat_skewed = interp.env.get("stat_skewed").unwrap().as_f64().unwrap();
    let p_skewed = interp.env.get("p_skewed").unwrap().as_f64().unwrap();

    // For skewed observed counts [10, 20, 60], total = 90, expected = 30 each:
    // terms: (10-30)^2/30 + (20-30)^2/30 + (60-30)^2/30 = 400/30 + 100/30 + 900/30 = 1400/30 = 46.6667
    assert!((stat_skewed - 46.66666667).abs() < 1e-4);
    assert!(p_skewed < 1e-5);

    let td_val = interp.env.get("td").unwrap();
    assert!(
        matches!(td_val, Value::DataFrame { .. }),
        "tidy(res) must return a DataFrame"
    );
}
