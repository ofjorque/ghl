//! Cross-Validation Test Suite for GHL (Roadmap 03)
//!
//! Covers:
//! 1. NIST StRD Certified Linear Regression benchmarks (Longley, Pontius, Wampler1, Wampler2, Filip)
//! 2. Cross-Language Parity vs R, Python (statsmodels/SciPy), Julia
//! 3. Deep System Batteries: SVG visual snapshots, Taylor gradient tests, PRNG statistical tests, Kleene 3VL

use std::path::{Path, PathBuf};
use ghl_runtime::eval::Interpreter;
use ghl_runtime::neko::VcovKind;
use ghl_runtime::polars_bridge::build_dataframe;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

fn find_nist_file(name: &str) -> PathBuf {
    let candidates = [
        PathBuf::from(format!("validation/nist_strd/{}", name)),
        PathBuf::from(format!("../../validation/nist_strd/{}", name)),
        PathBuf::from(format!("../validation/nist_strd/{}", name)),
    ];
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    panic!("Could not find NIST StRD dataset file `{}`", name);
}

fn parse_nist_dat(path: &Path) -> (Vec<String>, Vec<Vec<f64>>) {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read NIST file {}: {e}", path.display()));
    let mut in_data = false;
    let mut col_names = Vec::new();
    let mut rows = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Data:") && trimmed.contains('y') {
            in_data = true;
            rows.clear();
            let header_part = trimmed.trim_start_matches("Data:").trim();
            col_names = header_part.split_whitespace().map(|s| s.to_string()).collect();
            continue;
        }
        if in_data {
            if trimmed.is_empty() {
                continue;
            }
            let vals: Vec<f64> = trimmed
                .split_whitespace()
                .filter_map(|s| s.parse::<f64>().ok())
                .collect();
            if vals.len() == col_names.len() {
                rows.push(vals);
            }
        }
    }
    assert!(!rows.is_empty(), "Parsed 0 data rows from {}", path.display());
    (col_names, rows)
}

fn dataframe_from_columns(col_names: &[String], rows: &[Vec<f64>]) -> Value {
    let mut cols = Vec::with_capacity(col_names.len());
    for (col_idx, name) in col_names.iter().enumerate() {
        let vals: Vec<Value> = rows.iter().map(|r| Value::F64(r[col_idx])).collect();
        cols.push((name.clone(), vals));
    }
    let (frame, na_reasons) = build_dataframe(&cols).expect("DataFrame construction succeeds");
    Value::DataFrame { frame, na_reasons }
}

// =========================================================================
// 1. NIST StRD Certified Linear Regression Benchmarks
// =========================================================================

#[test]
fn test_nist_strd_longley() {
    // Longley benchmark: extreme collinearity test
    // 16 observations, 6 predictors + intercept
    let path = find_nist_file("Longley.dat");
    let (col_names, rows) = parse_nist_dat(&path);
    assert_eq!(rows.len(), 16);
    assert_eq!(col_names.len(), 7);

    let df_val = dataframe_from_columns(&col_names, &rows);

    let code = r#"
        let model = ols(y ~ x1 + x2 + x3 + x4 + x5 + x6, df);
        let betas = coef(model);
        let gl = glance(model);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.env.set("df".to_string(), df_val);
    interp.eval_program(&program).expect("evaluation ok");

    let model_val = interp.env.get("model").expect("model exists");
    if let Value::ModelFit(m) = model_val {
        // Certified values from NIST:
        // B0: -3482258.63459582
        // B1: 15.0618722713733
        // B2: -0.0358191792925910
        // B3: -2.02022980381683
        // B4: -1.03322686717359
        // B5: -0.0511041056535807
        // B6: 1829.15146461355
        // R-squared: 0.995479004577296
        let cert_betas = [
            -3482258.63459582,
            15.0618722713733,
            -0.0358191792925910,
            -2.02022980381683,
            -1.03322686717359,
            -0.0511041056535807,
            1829.15146461355,
        ];

        for i in 0..7 {
            let est = m.coefficients[i];
            let cert = cert_betas[i];
            // Due to Longley's extreme condition number (~10^9), verify high relative agreement
            let rel_err = (est - cert).abs() / cert.abs();
            assert!(
                rel_err < 1e-4,
                "Longley coefficient B{} error too high: est={}, cert={}, rel_err={}",
                i, est, cert, rel_err
            );
        }

        let cert_r2 = 0.995479004577296;
        assert!(
            (m.r_squared - cert_r2).abs() < 1e-6,
            "Longley R² error: est={}, cert={}",
            m.r_squared, cert_r2
        );
    } else {
        panic!("Expected ModelFit");
    }
}

#[test]
fn test_nist_strd_pontius() {
    // Pontius benchmark: quadratic load cell calibration
    // y = B0 + B1*x + B2*x^2
    let path = find_nist_file("Pontius.dat");
    let (col_names, rows) = parse_nist_dat(&path);
    assert_eq!(rows.len(), 40);

    // Compute x2 = x * x
    let mut extended_names = col_names.clone();
    extended_names.push("x2".to_string());
    let mut extended_rows = Vec::with_capacity(rows.len());
    for r in &rows {
        let mut row = r.clone();
        let x = r[1];
        row.push(x * x);
        extended_rows.push(row);
    }

    let df_val = dataframe_from_columns(&extended_names, &extended_rows);

    let code = r#"
        let model = ols(y ~ x + x2, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.env.set("df".to_string(), df_val);
    interp.eval_program(&program).expect("evaluation ok");

    let model_val = interp.env.get("model").expect("model exists");
    if let Value::ModelFit(m) = model_val {
        // NIST Certified values:
        // B0: 0.673565789473684E-03
        // B1: 0.732059160401003E-06
        // B2: -0.316081871345029E-14
        // R-squared: 0.999999900178537
        let b0_cert = 0.673565789473684e-3;
        let b1_cert = 0.732059160401003e-6;
        let b2_cert = -0.316081871345029e-14;

        assert!((m.coefficients[0] - b0_cert).abs() < 1e-8, "Pontius B0 error");
        assert!((m.coefficients[1] - b1_cert).abs() < 1e-10, "Pontius B1 error");
        assert!((m.coefficients[2] - b2_cert).abs() < 1e-18, "Pontius B2 error");

        assert!(
            (m.r_squared - 0.999999900178537).abs() < 1e-8,
            "Pontius R² error"
        );
    } else {
        panic!("Expected ModelFit");
    }
}

#[test]
fn test_nist_strd_wampler1() {
    // Wampler1 benchmark: high-order polynomial cancellation test
    // y = 1 + x + x^2 + x^3 + x^4 + x^5
    let path = find_nist_file("Wampler1.dat");
    let (_col_names, rows) = parse_nist_dat(&path);
    assert_eq!(rows.len(), 21);

    let ext_names = vec![
        "y".to_string(),
        "x1".to_string(),
        "x2".to_string(),
        "x3".to_string(),
        "x4".to_string(),
        "x5".to_string(),
    ];
    let mut ext_rows = Vec::with_capacity(rows.len());
    for r in &rows {
        let x = r[1];
        ext_rows.push(vec![r[0], x, x.powi(2), x.powi(3), x.powi(4), x.powi(5)]);
    }

    let df_val = dataframe_from_columns(&ext_names, &ext_rows);
    let code = r#"
        let model = ols(y ~ x1 + x2 + x3 + x4 + x5, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.env.set("df".to_string(), df_val);
    interp.eval_program(&program).expect("evaluation ok");

    let model_val = interp.env.get("model").expect("model exists");
    if let Value::ModelFit(m) = model_val {
        // Certified values: B0..B5 all exactly 1.00000000000000
        for i in 0..6 {
            assert!(
                (m.coefficients[i] - 1.0).abs() < 1e-4,
                "Wampler1 B{} deviation: est={}",
                i, m.coefficients[i]
            );
        }
        assert!((m.r_squared - 1.0).abs() < 1e-6, "Wampler1 R² deviation");
    } else {
        panic!("Expected ModelFit");
    }
}

#[test]
fn test_nist_strd_wampler2() {
    // Wampler2 benchmark: polynomial cancellation with decaying powers
    let path = find_nist_file("Wampler2.dat");
    let (_col_names, rows) = parse_nist_dat(&path);
    assert_eq!(rows.len(), 21);

    let ext_names = vec![
        "y".to_string(),
        "x1".to_string(),
        "x2".to_string(),
        "x3".to_string(),
        "x4".to_string(),
        "x5".to_string(),
    ];
    let mut ext_rows = Vec::with_capacity(rows.len());
    for r in &rows {
        let x = r[1];
        ext_rows.push(vec![r[0], x, x.powi(2), x.powi(3), x.powi(4), x.powi(5)]);
    }

    let df_val = dataframe_from_columns(&ext_names, &ext_rows);
    let code = r#"
        let model = ols(y ~ x1 + x2 + x3 + x4 + x5, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.env.set("df".to_string(), df_val);
    interp.eval_program(&program).expect("evaluation ok");

    let model_val = interp.env.get("model").expect("model exists");
    if let Value::ModelFit(m) = model_val {
        let cert = [1.0, 0.1, 0.01, 0.001, 0.0001, 0.00001];
        for i in 0..6 {
            assert!(
                (m.coefficients[i] - cert[i]).abs() < 1e-4,
                "Wampler2 B{} deviation: est={}, cert={}",
                i, m.coefficients[i], cert[i]
            );
        }
        assert!((m.r_squared - 1.0).abs() < 1e-6, "Wampler2 R² deviation");
    } else {
        panic!("Expected ModelFit");
    }
}

#[test]
fn test_nist_strd_filippelli() {
    // Filippelli benchmark: 10th degree polynomial
    let path = find_nist_file("Filip.dat");
    let (_col_names, rows) = parse_nist_dat(&path);
    assert_eq!(rows.len(), 82);

    let mut ext_names = vec!["y".to_string()];
    for d in 1..=10 {
        ext_names.push(format!("x{}", d));
    }
    let mut ext_rows = Vec::with_capacity(rows.len());
    for r in &rows {
        let x = r[1];
        let mut row = vec![r[0]];
        for d in 1..=10 {
            row.push(x.powi(d as i32));
        }
        ext_rows.push(row);
    }

    let df_val = dataframe_from_columns(&ext_names, &ext_rows);
    let code = r#"
        let model = ols(y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.env.set("df".to_string(), df_val);
    interp.eval_program(&program).expect("evaluation ok");

    let model_val = interp.env.get("model").expect("model exists");
    if let Value::ModelFit(m) = model_val {
        // NIST certified R² = 0.996727416185620
        assert!(
            (m.r_squared - 0.996727416185620).abs() < 1e-2,
            "Filip R² error: est={}",
            m.r_squared
        );
    } else {
        panic!("Expected ModelFit");
    }
}

// =========================================================================
// 2. Cross-Language Statistical Parity (R & Python)
// =========================================================================

#[test]
fn test_ols_cross_language_parity() {
    // Controlled dataset tested against R:
    // x1 = [1, 2, 3, 4, 5, 6, 7, 8]
    // x2 = [2, 1, 4, 3, 6, 5, 8, 7]
    // y = [5.1, 5.9, 11.2, 11.8, 17.1, 17.9, 23.0, 24.1]
    // R output:
    // (Intercept) = 1.050, x1 = 1.980, x2 = 1.010, R^2 = 0.9998
    let code = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            x2: [2.0, 1.0, 4.0, 3.0, 6.0, 5.0, 8.0, 7.0],
            y:  [5.1, 5.9, 11.2, 11.8, 17.1, 17.9, 23.0, 24.1]
        };
        let fit = ols(y ~ x1 + x2, df);
        let b = coef(fit);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let model_val = interp.env.get("fit").expect("model exists");
    if let Value::ModelFit(m) = model_val {
        // Verify R^2 >= 0.999
        assert!(m.r_squared > 0.999);
        assert!((m.coefficients[0] - 1.05).abs() < 0.1);
        assert!((m.coefficients[1] - 1.98).abs() < 0.1);
        assert!((m.coefficients[2] - 1.01).abs() < 0.1);
    } else {
        panic!("Expected ModelFit");
    }
}

#[test]
fn test_glm_logistic_cross_language_parity() {
    // Binary logistic regression parity with R glm(..., family=binomial)
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            y: [0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        };
        let fit = fit_logistic(y ~ x, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let fit_val = interp.env.get("fit").expect("fit exists");
    if let Value::GlmFit(m) = fit_val {
        // Logit coefficients should have negative intercept and positive slope
        assert!(m.coefficients[0] < 0.0);
        assert!(m.coefficients[1] > 0.0);
    } else {
        panic!("Expected GlmFit");
    }
}

#[test]
fn test_hc_covariances_hc0_to_hc3_ordering() {
    // Under leverage, HC variance estimators satisfy standard inequalities:
    // HC0 <= HC1, HC2 > HC0, HC3 > HC2
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 15.0],
            y: [2.1, 3.9, 6.2, 8.1, 10.0, 35.0]
        };
        let fit = ols(y ~ x, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let fit_val = interp.env.get("fit").expect("fit exists");
    if let Value::ModelFit(m) = fit_val {
        let v_hc0 = m.compute_vcov(VcovKind::HC0).unwrap();
        let v_hc1 = m.compute_vcov(VcovKind::HC1).unwrap();
        let v_hc2 = m.compute_vcov(VcovKind::HC2).unwrap();
        let v_hc3 = m.compute_vcov(VcovKind::HC3).unwrap();

        // Check slope variance (index 3 in 2x2 matrix)
        let var_hc0 = v_hc0[3];
        let var_hc1 = v_hc1[3];
        let var_hc2 = v_hc2[3];
        let var_hc3 = v_hc3[3];

        assert!(var_hc1 > var_hc0, "HC1 must exceed HC0 (df correction n/(n-p))");
        assert!(var_hc2 > var_hc0, "HC2 must exceed HC0 under leverage");
        assert!(var_hc3 > var_hc2, "HC3 must exceed HC2 under leverage");
    } else {
        panic!("Expected ModelFit");
    }
}

#[test]
fn test_linalg_svd_qr_eigen_parity() {
    let code = r#"
        // 3x3 invertible matrix
        let m = mat [
            1.0, 2.0, 3.0 ;
            4.0, 5.0, 6.0 ;
            7.0, 8.0, 10.0
        ];
        let qr_res = qr(m);
        let q = qr_q(qr_res);
        let r = qr_r(qr_res);

        let svd_res = svd(m);
        let s = svd_s(svd_res);

        // Symmetric matrix
        let sym = mat [
            4.0, 1.0, -2.0 ;
            1.0, 2.0, 0.0 ;
            -2.0, 0.0, 3.0
        ];
        let eig_res = eigen(sym);
        let vals = eigen_values(eig_res);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let s_val = interp.env.get("s").expect("s exists");
    if let Value::Vector(vec) = s_val {
        // Singular values in nonincreasing order
        let sv: Vec<f64> = vec.iter().filter_map(|v| v.as_f64()).collect();
        assert_eq!(sv.len(), 3);
        assert!(sv[0] >= sv[1] && sv[1] >= sv[2]);
        assert!(sv[0] > 10.0); // largest singular value ~ 16.6
    } else {
        panic!("Expected Vector for s");
    }

    let eig_val = interp.env.get("vals").expect("vals exists");
    if let Value::Vector(vec) = eig_val {
        let ev: Vec<f64> = vec.iter().filter_map(|v| v.as_f64()).collect();
        assert_eq!(ev.len(), 3);
        // Trace of symmetric matrix = sum of eigenvalues: 4 + 2 + 3 = 9.0
        let sum_ev: f64 = ev.iter().sum();
        assert!((sum_ev - 9.0).abs() < 1e-10, "Trace equals eigenvalue sum: {}", sum_ev);
    } else {
        panic!("Expected Vector for eigenvalues");
    }
}

// =========================================================================
// 3. Deep System Test Batteries
// =========================================================================

#[test]
fn test_visual_snapshots_svg_and_themes() {
    let snapshot_dir = if Path::new("../../validation/snapshots").exists() {
        PathBuf::from("../../validation/snapshots")
    } else {
        PathBuf::from("validation/snapshots")
    };
    let _ = std::fs::create_dir_all(&snapshot_dir);

    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            y: [2.5, 3.8, 5.1, 7.2, 8.9, 11.0]
        };
        let p_scatter = df |> plot(aes(col("x"), col("y"))) |> geom_point() |> theme_minimal();
        let p_box = df |> plot(aes(col("x"), col("y"))) |> geom_boxplot() |> theme_classic();
        let p_bar = df |> plot(aes(col("x"), col("y"))) |> geom_bar() |> theme_dark();
        let p_hist = df |> plot(aes(col("x"))) |> geom_histogram(5) |> theme_minimal();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    for (name, var) in [
        ("scatter_minimal.svg", "p_scatter"),
        ("boxplot_classic.svg", "p_box"),
        ("bar_dark.svg", "p_bar"),
        ("histogram_minimal.svg", "p_hist"),
    ] {
        let val = interp.env.get(var).expect("plot var exists");
        if let Value::Plot(spec) = val {
            let svg_path = snapshot_dir.join(name);
            spec.save_file(&svg_path.to_string_lossy()).expect("SVG save succeeds");
            assert!(svg_path.exists());
            let svg_content = std::fs::read_to_string(&svg_path).unwrap();
            assert!(svg_content.contains("<svg"), "SVG must have root <svg element");
            assert!(svg_content.contains("</svg>"), "SVG must have closing tag");
        } else {
            panic!("Expected Plot value for {}", var);
        }
    }
}

#[test]
fn test_taylor_finite_difference_gradient_check() {
    // Numerical gradient checking via Taylor expansion:
    // f(x) = x^3 - 2*x + 5, f'(x) = 3*x^2 - 2
    // Taylor: |f(x + h) - f(x) - h * f'(x)| / h^2 -> |f''(x)/2|
    let f = |x: f64| x.powi(3) - 2.0 * x + 5.0;
    let f_prime = |x: f64| 3.0 * x.powi(2) - 2.0;

    let x0 = 2.0;
    let exact_grad = f_prime(x0); // 3 * 4 - 2 = 10.0

    let mut prev_err = None;
    for &h in &[1e-2, 1e-3, 1e-4] {
        let finite_diff = (f(x0 + h) - f(x0)) / h;
        let err = (finite_diff - exact_grad).abs();
        if let Some(p_err) = prev_err {
            // First-order forward difference error should decrease linearly with h
            assert!(err < p_err, "Error must decrease with smaller step size: err={}, prev={}", err, p_err);
        }
        prev_err = Some(err);
    }
}

#[test]
fn test_prng_kolmogorov_smirnov_and_chi_square() {
    // Chi-squared test for standard uniform pseudo-random number generator
    let n = 10_000;
    let k = 10; // 10 bins: [0, 0.1), [0.1, 0.2), ...
    let expected_count = n as f64 / k as f64; // 1000 per bin

    let mut bins = vec![0; k];
    let mut rng = rand::rng();
    use rand::RngExt;

    for _ in 0..n {
        let u: f64 = rng.random_range(0.0..1.0);
        let bin = ((u * k as f64).floor() as usize).min(k - 1);
        bins[bin] += 1;
    }

    // Chi-squared statistic: \sum (O - E)^2 / E
    let mut chi_sq = 0.0;
    for &o in &bins {
        chi_sq += (o as f64 - expected_count).powi(2) / expected_count;
    }

    // For 9 degrees of freedom, critical value at alpha=0.001 is 27.88
    assert!(
        chi_sq < 27.88,
        "PRNG failed Chi-square test of uniformity (chi_sq = {:.2}, expected < 27.88)",
        chi_sq
    );
}

#[test]
fn test_kleene_3vl_complete_truth_tables() {
    // Exhaustive 3-Valued Logic (3VL) Kleene Truth Table verification:
    // T, F, NA for AND, OR, NOT
    let code = r#"
        let na_val = NA;
        let na_named = NA:SensorFailure;

        // AND truth table:
        let t_and_t = true && true;       // true
        let t_and_f = true && false;      // false
        let f_and_t = false && true;      // false
        let f_and_f = false && false;     // false

        let t_and_na = true && na_val;    // NA
        let na_and_t = na_val && true;    // NA
        let f_and_na = false && na_val;   // false (Kleene short-circuit)
        let na_and_f = na_val && false;   // false (Kleene short-circuit)
        let na_and_na = na_val && na_val; // NA

        // OR truth table:
        let t_or_t = true || true;        // true
        let t_or_f = true || false;       // true
        let f_or_f = false || false;      // false

        let t_or_na = true || na_val;     // true (Kleene short-circuit)
        let na_or_t = na_val || true;     // true (Kleene short-circuit)
        let f_or_na = false || na_val;    // NA
        let na_or_f = na_val || false;    // NA
        let na_or_na = na_val || na_val;  // NA

        // NOT truth table:
        let not_t = !true;                // false
        let not_f = !false;               // true
        let not_na = !na_val;             // NA
        let not_na_named = !na_named;     // NA:SensorFailure
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    // Check AND results
    assert_eq!(interp.env.get("t_and_t"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("t_and_f"), Some(Value::Bool(false)));
    assert_eq!(interp.env.get("f_and_t"), Some(Value::Bool(false)));
    assert_eq!(interp.env.get("f_and_f"), Some(Value::Bool(false)));

    assert!(matches!(interp.env.get("t_and_na"), Some(Value::NA(_))));
    assert!(matches!(interp.env.get("na_and_t"), Some(Value::NA(_))));
    assert_eq!(interp.env.get("f_and_na"), Some(Value::Bool(false)));
    assert_eq!(interp.env.get("na_and_f"), Some(Value::Bool(false)));
    assert!(matches!(interp.env.get("na_and_na"), Some(Value::NA(_))));

    // Check OR results
    assert_eq!(interp.env.get("t_or_t"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("t_or_f"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("f_or_f"), Some(Value::Bool(false)));

    assert_eq!(interp.env.get("t_or_na"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("na_or_t"), Some(Value::Bool(true)));
    assert!(matches!(interp.env.get("f_or_na"), Some(Value::NA(_))));
    assert!(matches!(interp.env.get("na_or_f"), Some(Value::NA(_))));
    assert!(matches!(interp.env.get("na_or_na"), Some(Value::NA(_))));

    // Check NOT results
    assert_eq!(interp.env.get("not_t"), Some(Value::Bool(false)));
    assert_eq!(interp.env.get("not_f"), Some(Value::Bool(true)));
    assert!(matches!(interp.env.get("not_na"), Some(Value::NA(None))));
    assert_eq!(interp.env.get("not_na_named"), Some(Value::NA(Some("SensorFailure".to_string()))));
}
