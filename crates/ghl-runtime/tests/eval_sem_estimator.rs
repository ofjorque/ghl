use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

fn run_ghl(src: &str) -> (Interpreter, Value) {
    let program = parse(src).expect("syntax ok");
    let mut interp = Interpreter::new();
    let res = interp.eval_program(&program).expect("evaluation ok");
    (interp, res)
}

#[test]
fn test_optim_quadratic_bfgs() {
    let src = r#"
        fn quad(par) {
            let x = par[0];
            let y = par[1];
            (x - 3.0) * (x - 3.0) + (y + 2.0) * (y + 2.0)
        }

        let res = optim(quad, [0.0, 0.0], "BFGS");
        res
    "#;

    let (_, res) = run_ghl(src);
    if let Value::Record(fields) = res {
        let par = fields.get("par").expect("par field");
        let conv = fields.get("converged").expect("converged field");
        let val = fields.get("value").expect("value field");

        if let Value::Bool(b) = conv {
            assert!(b, "BFGS should converge");
        }
        if let Value::Vector(vd) = par {
            let p0 = vd[0].as_f64().unwrap();
            let p1 = vd[1].as_f64().unwrap();
            assert!((p0 - 3.0).abs() < 1e-3, "Expected x ≈ 3.0, got {}", p0);
            assert!((p1 - (-2.0)).abs() < 1e-3, "Expected y ≈ -2.0, got {}", p1);
        }
        if let Value::F64(v) = val {
            assert!(v.abs() < 1e-4, "Expected minimum value ≈ 0.0, got {}", v);
        }
    } else {
        panic!("Expected Record from optim()");
    }
}

#[test]
fn test_optim_rosenbrock_nelder_mead() {
    let src = r#"
        fn rosenbrock(par) {
            let x = par[0];
            let y = par[1];
            (1.0 - x) * (1.0 - x) + 100.0 * (y - x * x) * (y - x * x)
        }

        let res = optim(rosenbrock, [-1.0, 1.0], "Nelder-Mead", 2000, 0.000001);
        res
    "#;

    let (_, res) = run_ghl(src);
    if let Value::Record(fields) = res {
        let par = fields.get("par").expect("par field");
        if let Value::Vector(vd) = par {
            let p0 = vd[0].as_f64().unwrap();
            let p1 = vd[1].as_f64().unwrap();
            assert!((p0 - 1.0).abs() < 0.05, "Expected x ≈ 1.0, got {}", p0);
            assert!((p1 - 1.0).abs() < 0.05, "Expected y ≈ 1.0, got {}", p1);
        }
    } else {
        panic!("Expected Record from optim()");
    }
}

#[test]
fn test_sample_covariance() {
    let src = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0],
            x2: [2.0, 3.0, 5.0, 7.0, 11.0]
        };

        sample_cov(df, ["x1", "x2"])
    "#;

    let (_, res) = run_ghl(src);
    if let Value::Matrix { rows, cols, data } = res {
        assert_eq!(rows, 2);
        assert_eq!(cols, 2);
        // Var(x1) = 2.5
        assert!(
            (data[0] - 2.5).abs() < 1e-6,
            "Expected Var(x1) = 2.5, got {}",
            data[0]
        );
        // Cov(x1, x2) = 5.5
        assert!(
            (data[1] - 5.5).abs() < 1e-6,
            "Expected Cov(x1, x2) = 5.5, got {}",
            data[1]
        );
        // Var(x2) = 12.8
        assert!(
            (data[3] - 12.8).abs() < 1e-6,
            "Expected Var(x2) = 12.8, got {}",
            data[3]
        );
    } else {
        panic!("Expected Matrix from sample_cov()");
    }
}

#[test]
fn test_sem_one_factor_cfa_end_to_end() {
    // Generate synthetic 1-factor model data:
    // xi ~ N(0, 1)
    // x1 = 1.0 * xi + e1 (var ≈ 0.36)
    // x2 = 0.8 * xi + e2 (var ≈ 0.49)
    // x3 = 1.2 * xi + e3 (var ≈ 0.25)
    let mut x1_vals = Vec::new();
    let mut x2_vals = Vec::new();
    let mut x3_vals = Vec::new();

    let n = 200;
    for i in 0..n {
        let t = i as f64 / 10.0;
        let xi = (t * 1.5).sin() + (t * 0.7).cos();
        let e1 = (t * 2.3).sin() * 0.5;
        let e2 = (t * 3.1).cos() * 0.6;
        let e3 = (t * 4.7).sin() * 0.4;

        x1_vals.push(1.0 * xi + e1);
        x2_vals.push(0.8 * xi + e2);
        x3_vals.push(1.2 * xi + e3);
    }

    let x1_str = format!("{:?}", x1_vals);
    let x2_str = format!("{:?}", x2_vals);
    let x3_str = format!("{:?}", x3_vals);

    let src = format!(
        r#"
        let df = dataframe {{
            x1: {},
            x2: {},
            x3: {}
        }};

        let spec = sem_spec {{
            f1 =~ x1 + x2 + x3;
        }};

        let fit = sem(spec, df);
        summary(fit);

        let t = tidy(fit);
        let g = glance(fit);
        let v = vcov(fit);
    "#,
        x1_str, x2_str, x3_str
    );

    let (interp, _) = run_ghl(&src);

    let t = interp.env.get("t").expect("t exists in environment");
    let g = interp.env.get("g").expect("g exists in environment");
    let v = interp.env.get("v").expect("v exists in environment");

    // 1. Check tidy() DataFrame
    if let Value::DataFrame { frame, .. } = t {
        assert!(
            frame.height() >= 5,
            "Expected at least 5 estimated parameters"
        );
        assert!(frame.column("term").is_ok());
        assert!(frame.column("estimate").is_ok());
        assert!(frame.column("std_error").is_ok());
        assert!(frame.column("statistic").is_ok());
        assert!(frame.column("p_value").is_ok());
        assert!(frame.column("conf_low").is_ok());
        assert!(frame.column("conf_high").is_ok());

        // Check marker variable fixed to 1.0
        let terms = frame.column("term").unwrap().str().unwrap();
        let ests = frame.column("estimate").unwrap().f64().unwrap();
        let mut found_marker = false;
        for i in 0..terms.len() {
            if terms.get(i) == Some("f1 =~ x1") {
                assert_eq!(ests.get(i), Some(1.0));
                found_marker = true;
            }
        }
        assert!(found_marker, "Marker variable f1 =~ x1 should be present");
    } else {
        panic!("Expected DataFrame from tidy(), found {:?}", t);
    }

    // 2. Check glance() DataFrame
    if let Value::DataFrame { frame, .. } = g {
        assert_eq!(frame.height(), 1);
        let cfi = frame.column("cfi").unwrap().f64().unwrap().get(0).unwrap();
        let srmr = frame.column("srmr").unwrap().f64().unwrap().get(0).unwrap();
        let rmsea = frame
            .column("rmsea")
            .unwrap()
            .f64()
            .unwrap()
            .get(0)
            .unwrap();

        assert!(cfi >= 0.85, "Expected high CFI (>= 0.85), got {}", cfi);
        assert!(srmr <= 0.15, "Expected low SRMR (<= 0.15), got {}", srmr);
        assert!(rmsea >= 0.0, "Expected non-negative RMSEA, got {}", rmsea);
    } else {
        panic!("Expected DataFrame from glance(), found {:?}", g);
    }

    // 3. Check vcov() Matrix
    if let Value::Matrix { rows, cols, data } = v {
        assert_eq!(rows, 3);
        assert_eq!(cols, 3);
        assert_eq!(data.len(), 9);
        // Diagonal entries (variances) must be positive
        assert!(data[0] > 0.0);
        assert!(data[4] > 0.0);
        assert!(data[8] > 0.0);
    } else {
        panic!("Expected Matrix from vcov(), found {:?}", v);
    }
}

#[test]
fn test_optim_bounded_lbfgs_b() {
    let src = r#"
        fn quad(par) {
            let x = par[0];
            let y = par[1];
            (x - 5.0) * (x - 5.0) + (y + 5.0) * (y + 5.0)
        }

        // Unconstrained min is (5.0, -5.0).
        // With bounds lower = [0.0, -3.0], upper = [2.0, 0.0],
        // the constrained min must be exactly (2.0, -3.0).
        let res = optim(quad, [0.0, 0.0], "L-BFGS-B", 1000, 1e-6, [0.0, -3.0], [2.0, 0.0]);
        res
    "#;

    let (_, res) = run_ghl(src);
    if let Value::Record(fields) = res {
        let par = fields.get("par").expect("par field");
        let conv = fields.get("converged").expect("converged field");
        let val = fields.get("value").expect("value field");

        if let Value::Bool(b) = conv {
            assert!(b, "L-BFGS-B should converge");
        }
        if let Value::Vector(vd) = par {
            let p0 = vd[0].as_f64().unwrap();
            let p1 = vd[1].as_f64().unwrap();
            assert!(
                (p0 - 2.0).abs() < 1e-3,
                "Expected bounded x ≈ 2.0, got {}",
                p0
            );
            assert!(
                (p1 - (-3.0)).abs() < 1e-3,
                "Expected bounded y ≈ -3.0, got {}",
                p1
            );
        }
        if let Value::F64(v) = val {
            assert!(
                (v - 13.0).abs() < 1e-2,
                "Expected f(2, -3) ≈ 13.0, got {}",
                v
            );
        }
    } else {
        panic!("Expected Record from optim()");
    }
}

#[test]
fn test_nls_levenberg_marquardt() {
    let src = r#"
        fn michaelis_residuals(par) {
            let vmax = par[0];
            let km = par[1];

            let x = [1.0, 2.0, 5.0, 10.0, 20.0];
            let y = [3.333333, 5.0, 7.142857, 8.333333, 9.090909];

            let mut r = [];
            for i in 0..5 {
                let xi = x[i];
                let yi = y[i];
                let pred = (vmax * xi) / (km + xi);
                r = append(r, yi - pred);
            }
            r
        }

        let res = nls(michaelis_residuals, [2.0, 1.0], 500, 1e-6);
        res
    "#;

    let (_, res) = run_ghl(src);
    if let Value::Record(fields) = res {
        let par = fields.get("par").expect("par field");
        let conv = fields.get("converged").expect("converged field");

        if let Value::Bool(b) = conv {
            assert!(b, "NLS Levenberg-Marquardt should converge");
        }
        if let Value::Vector(vd) = par {
            let vmax = vd[0].as_f64().unwrap();
            let km = vd[1].as_f64().unwrap();
            assert!(
                (vmax - 10.0).abs() < 0.1,
                "Expected Vmax ≈ 10.0, got {}",
                vmax
            );
            assert!((km - 2.0).abs() < 0.1, "Expected Km ≈ 2.0, got {}", km);
        }
    } else {
        panic!("Expected Record from nls()");
    }
}
