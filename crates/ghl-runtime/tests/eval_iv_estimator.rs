use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

fn run_ghl(src: &str) -> (Interpreter, Result<Value, ghl_diagnostics::Diagnostic>) {
    let program = parse(src).expect("syntax ok");
    let mut interp = Interpreter::new();
    let res = interp.eval_program(&program);
    (interp, res)
}

#[test]
fn test_iv_just_identified() {
    let src = r#"
        // Synthetic IV data:
        // Instrument z: strong correlation with endogenous x
        // True model: y = 2.0 + 3.0 * x
        let df = dataframe {
            z: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            x: [3.5, 6.0, 8.5, 11.0, 13.5, 16.0, 18.5, 21.0, 23.5, 26.0], // x = 1.0 + 2.5 * z
            y: [12.5, 20.0, 27.5, 35.0, 42.5, 50.0, 57.5, 65.0, 72.5, 80.0] // y = 2.0 + 3.0 * x
        };

        let res = iv_regress(y ~ x | z, df);
        res
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("iv_regress should succeed");
    if let Value::Struct { name, fields } = val {
        assert_eq!(name, "IvResult");
        let coefs = fields.get("coefficients").expect("coefficients");
        let terms = fields.get("terms").expect("terms");
        let endog = fields.get("endogenous").expect("endogenous");
        let f_stat = fields.get("first_stage_f").expect("first_stage_f");
        let weak = fields.get("weak_instruments").expect("weak_instruments");
        let sargan = fields.get("sargan_stat").expect("sargan_stat");

        if let Value::Vector(vd) = coefs {
            let b0 = vd[0].as_f64().unwrap();
            let b1 = vd[1].as_f64().unwrap();
            assert!((b0 - 2.0).abs() < 1e-4, "Expected intercept ≈ 2.0, got {}", b0);
            assert!((b1 - 3.0).abs() < 1e-4, "Expected slope ≈ 3.0, got {}", b1);
        } else {
            panic!("Expected Vector for coefficients");
        }

        if let Value::Vector(t_vec) = terms {
            assert_eq!(t_vec.len(), 2);
        }

        if let Value::Vector(f_vec) = f_stat {
            assert_eq!(f_vec.len(), 1);
            let f = f_vec[0].as_f64().unwrap();
            assert!(f > 10.0, "Expected F-statistic > 10, got {}", f);
        }

        if let Value::Vector(vd) = endog {
            assert_eq!(vd.len(), 1);
            assert_eq!(vd[0].as_str().unwrap(), "x");
        }

        if let Value::Bool(is_weak) = weak {
            assert!(!is_weak, "Strong instrument should not be weak");
        }

        // Just-identified models cannot compute Sargan overidentification test
        assert!(sargan.is_na(), "Just-identified model should have NA Sargan stat");
    } else {
        panic!("Expected IvResult struct");
    }
}

#[test]
fn test_iv_overidentified_with_exogenous() {
    let src = r#"
        // 1 exogenous (w), 1 endogenous (x), 2 excluded instruments (z1, z2)
        // True model: y = 5.0 + 2.0 * w + 4.0 * x
        let df = dataframe {
            w:  [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            z1: [2.0, 3.0, 5.0, 7.0, 8.0, 10.0, 11.0, 13.0, 14.0, 16.0],
            z2: [1.5, 2.5, 4.0, 5.5, 6.5, 8.0, 9.5, 11.0, 12.0, 14.0],
            // x = 1.0 + 0.5 * w + 1.2 * z1 + 0.8 * z2
            x:  [5.1, 7.6, 11.7, 15.8, 18.3, 22.4, 24.9, 29.0, 31.5, 35.6],
            // y = 5.0 + 2.0 * w + 4.0 * x
            y:  [27.4, 39.4, 57.8, 76.2, 88.2, 106.6, 118.6, 137.0, 149.0, 167.4]
        };

        let res = iv_regress(y ~ w + x | w + z1 + z2, df);
        res
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("overidentified iv_regress should succeed");
    if let Value::Struct { name, fields } = val {
        assert_eq!(name, "IvResult");
        let coefs = fields.get("coefficients").expect("coefficients");
        let sargan_stat = fields.get("sargan_stat").expect("sargan_stat");
        let sargan_df = fields.get("sargan_df").expect("sargan_df");
        let sargan_p = fields.get("sargan_p").expect("sargan_p");

        if let Value::Vector(vd) = coefs {
            let b0 = vd[0].as_f64().unwrap();
            let b_w = vd[1].as_f64().unwrap();
            let b_x = vd[2].as_f64().unwrap();
            assert!((b0 - 5.0).abs() < 1e-3, "Expected intercept ≈ 5.0, got {}", b0);
            assert!((b_w - 2.0).abs() < 1e-3, "Expected w coeff ≈ 2.0, got {}", b_w);
            assert!((b_x - 4.0).abs() < 1e-3, "Expected x coeff ≈ 4.0, got {}", b_x);
        }

        // Overidentified: df_sargan = 2 - 1 = 1
        if let Value::F64(s) = sargan_stat {
            assert!(*s >= 0.0, "Expected non-negative Sargan statistic");
        }
        if let Value::I64(df) = sargan_df {
            assert_eq!(*df, 1, "Expected Sargan df = 1");
        }
        if let Value::F64(p) = sargan_p {
            assert!(*p >= 0.0 && *p <= 1.0, "Expected valid p-value");
        }
    } else {
        panic!("Expected IvResult struct");
    }
}

#[test]
fn test_iv_underidentified_order_condition_s0101() {
    let src = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0],
            x2: [2.0, 3.0, 4.0, 5.0, 6.0],
            z1: [1.5, 2.5, 3.5, 4.5, 5.5],
            y:  [10.0, 12.0, 14.0, 16.0, 18.0]
        };

        // 2 endogenous variables (x1, x2), only 1 instrument (z1) -> underidentified!
        iv_regress(y ~ x1 + x2 | z1, df)
    "#;

    let (_, res) = run_ghl(src);
    assert!(res.is_err(), "Underidentified model should fail order condition");
    let err = res.err().unwrap();
    assert_eq!(err.code, "S0101");
    assert!(err.message.contains("Order condition failed"));
}

#[test]
fn test_iv_formula_missing_pipe_s0100() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0],
            y: [2.0, 4.0, 6.0, 8.0]
        };

        iv_regress(y ~ x, df)
    "#;

    let (_, res) = run_ghl(src);
    assert!(res.is_err(), "iv_regress without | should fail");
    let err = res.err().unwrap();
    assert_eq!(err.code, "S0100");
}

#[test]
fn test_iv_neko_verbs_and_summary() {
    let src = r#"
        let df = dataframe {
            z: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            x: [3.5, 6.0, 8.5, 11.0, 13.5, 16.0, 18.5, 21.0],
            y: [12.5, 20.0, 27.5, 35.0, 42.5, 50.0, 57.5, 65.0]
        };

        let m = iv_regress(y ~ x | z, df);

        let t = tidy(m);
        let g = glance(m);
        let c = coef(m);
        let r = residuals(m);
        let v = vcov(m);

        // Test summary console printing
        summary(m);

        { tidy: t, glance: g, coef: c, residuals: r, vcov: v }
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("neko verbs should succeed on IvResult");
    if let Value::Record(fields) = val {
        assert!(matches!(fields.get("tidy").unwrap(), Value::DataFrame { .. }), "tidy() should return DataFrame");
        assert!(matches!(fields.get("glance").unwrap(), Value::DataFrame { .. }), "glance() should return DataFrame");
        assert!(matches!(fields.get("coef").unwrap(), Value::Vector(_)), "coef() should return Vector");
        assert!(matches!(fields.get("residuals").unwrap(), Value::Vector(_)), "residuals() should return Vector");
        assert!(matches!(fields.get("vcov").unwrap(), Value::Matrix { .. }), "vcov() should return Matrix");
    } else {
        panic!("Expected Record of NEKO outputs");
    }
}

#[test]
fn test_model_matrix_primitive() {
    let src = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0],
            x2: [10.0, 20.0, 30.0, 40.0],
            y:  [5.0, 6.0, 7.0, 8.0]
        };

        model_matrix(y ~ x1 + x2, df)
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("model_matrix should succeed");
    if let Value::Record(fields) = val {
        assert!(matches!(fields.get("x").unwrap(), Value::Matrix { rows: 4, cols: 3, .. }));
        assert!(matches!(fields.get("y").unwrap(), Value::Vector(_)));
        assert_eq!(fields.get("n_obs").unwrap().as_i64().unwrap(), 4);
        assert_eq!(fields.get("p_cols").unwrap().as_i64().unwrap(), 3);
    } else {
        panic!("Expected Record from model_matrix");
    }
}
