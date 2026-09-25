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
fn test_feols_single_fixed_effect() {
    let src = r#"
        let df = dataframe {
            entity: [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3],
            x:      [1.0, 2.0, 3.0, 4.0, 2.0, 3.0, 5.0, 6.0, 1.5, 2.5, 3.5, 4.5],
            y:      [12.5, 15.0, 17.5, 20.0, 25.0, 27.5, 32.5, 35.0, 33.75, 36.25, 38.75, 41.25]
        };

        // alpha_1 = 10, alpha_2 = 20, alpha_3 = 30; beta = 2.5
        let res = feols(y ~ x | entity, df);
        res
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("feols evaluation succeeded");
    if let Value::Struct { name, fields } = val {
        assert_eq!(name, "FeolsResult");
        let coefs = fields.get("coefficients").expect("coefficients");
        let r2_w = fields.get("r2_within").expect("r2_within");
        let df_fe = fields.get("df_fe").expect("df_fe");
        let n_obs = fields.get("n_obs").expect("n_obs");

        if let Value::Vector(vd) = coefs {
            let b0 = vd[0].as_f64().unwrap();
            assert!((b0 - 2.5).abs() < 1e-6, "Expected beta ≈ 2.5, got {}", b0);
        } else {
            panic!("Expected Vector for coefficients");
        }

        if let Value::F64(r2) = r2_w {
            assert!((r2 - 1.0).abs() < 1e-6, "Expected within R2 ≈ 1.0, got {}", r2);
        }

        if let Value::I64(dffe) = df_fe {
            assert_eq!(*dffe, 3, "Expected 3 absorbed entity fixed effects");
        }

        if let Value::I64(n) = n_obs {
            assert_eq!(*n, 12, "Expected 12 observations");
        }
    } else {
        panic!("Expected FeolsResult struct, got {:?}", val);
    }
}

#[test]
fn test_feols_two_way_fixed_effects_map() {
    let src = r#"
        // 3 entities x 3 time periods = 9 observations
        // beta_1 = 3.0, beta_2 = -1.5
        // alpha_entity = [10, 20, 30]
        // gamma_time   = [1, 2, 3]
        let df = dataframe {
            entity: [1, 1, 1, 2, 2, 2, 3, 3, 3],
            time:   [1, 2, 3, 1, 2, 3, 1, 2, 3],
            x1:     [1.0, 4.0, 2.0, 5.0, 3.0, 6.0, 2.0, 5.0, 7.0],
            x2:     [2.0, 1.0, 4.0, 3.0, 5.0, 2.0, 4.0, 1.0, 3.0],
            y:      [11.0, 22.5, 13.0, 31.5, 23.5, 38.0, 31.0, 45.5, 49.5]
        };

        // Multi-factor alternating projections: entity + time
        let res = feols(y ~ x1 + x2 | entity + time, df);
        res
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("two-way feols succeeded");
    if let Value::Struct { name, fields } = val {
        assert_eq!(name, "FeolsResult");
        let coefs = fields.get("coefficients").expect("coefficients");
        let r2_w = fields.get("r2_within").expect("r2_within");
        let df_fe = fields.get("df_fe").expect("df_fe");
        let df_resid = fields.get("df_resid").expect("df_resid");

        if let Value::Vector(vd) = coefs {
            let b1 = vd[0].as_f64().unwrap();
            let b2 = vd[1].as_f64().unwrap();
            assert!((b1 - 3.0).abs() < 1e-4, "Expected beta1 ≈ 3.0, got {}", b1);
            assert!((b2 - (-1.5)).abs() < 1e-4, "Expected beta2 ≈ -1.5, got {}", b2);
        }

        if let Value::F64(r2) = r2_w {
            assert!((r2 - 1.0).abs() < 1e-4, "Expected within R2 ≈ 1.0, got {}", r2);
        }

        // df_FE = G_entity + (G_time - 1) = 3 + 2 = 5
        if let Value::I64(dffe) = df_fe {
            assert_eq!(*dffe, 5);
        }

        // df_resid = 9 - 2 - 5 = 2
        if let Value::I64(dfres) = df_resid {
            assert_eq!(*dfres, 2);
        }
    } else {
        panic!("Expected FeolsResult struct");
    }
}

#[test]
fn test_feols_vcov_options() {
    let src = r#"
        let df = dataframe {
            entity: [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3],
            x:      [1.0, 2.0, 3.0, 4.0, 2.0, 3.0, 5.0, 6.0, 1.5, 2.5, 3.5, 4.5],
            y:      [12.5, 15.0, 17.5, 20.0, 25.0, 27.5, 32.5, 35.0, 33.75, 36.25, 38.75, 41.25]
        };

        let res_iid = feols(y ~ x | entity, df, "classical");
        let res_rob = feols(y ~ x | entity, df, "robust");
        let res_clu = feols(y ~ x | entity, df, "cluster");

        [res_iid.vcov_type, res_rob.vcov_type, res_clu.vcov_type]
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("vcov options evaluation succeeded");
    if let Value::Vector(vd) = val {
        let t0 = vd[0].as_str().unwrap();
        let t1 = vd[1].as_str().unwrap();
        let t2 = vd[2].as_str().unwrap();

        assert!(t0.contains("Classical"));
        assert!(t1.contains("Robust"));
        assert!(t2.contains("Clustered"));
    } else {
        panic!("Expected vector of vcov_type strings");
    }
}

#[test]
fn test_feols_invariance_detection_s0101() {
    let src = r#"
        let df = dataframe {
            entity: [1, 1, 1, 1, 2, 2, 2, 2],
            gender: [1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0], // constant within entity!
            y:      [10.0, 11.0, 12.0, 13.0, 20.0, 21.0, 22.0, 23.0]
        };

        feols(y ~ gender | entity, df)
    "#;

    let (_, res) = run_ghl(src);
    assert!(res.is_err(), "Should fail with within-variation invariance error");
    let err = res.err().unwrap();
    assert_eq!(err.code, "S0101");
    assert!(err.message.contains("zero within-variation"));
}

#[test]
fn test_feols_formula_without_fe_s0100() {
    let src = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0],
            y: [2.0, 4.0, 6.0, 8.0]
        };

        feols(y ~ x, df)
    "#;

    let (_, res) = run_ghl(src);
    assert!(res.is_err(), "feols without fixed effects should error");
    let err = res.err().unwrap();
    assert_eq!(err.code, "S0100");
}

#[test]
fn test_feols_neko_verbs() {
    let src = r#"
        let df = dataframe {
            entity: [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3],
            x:      [1.0, 2.0, 3.0, 4.0, 2.0, 3.0, 5.0, 6.0, 1.5, 2.5, 3.5, 4.5],
            y:      [12.5, 15.0, 17.5, 20.0, 25.0, 27.5, 32.5, 35.0, 33.75, 36.25, 38.75, 41.25]
        };

        let m = feols(y ~ x | entity, df);

        let t = tidy(m);
        let g = glance(m);
        let c = coef(m);
        let r = residuals(m);
        let v = vcov(m);

        // Test summary output
        summary(m);

        { tidy: t, glance: g, coef: c, residuals: r, vcov: v }
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("neko verbs evaluation succeeded");
    if let Value::Record(fields) = val {
        assert!(matches!(fields.get("tidy").unwrap(), Value::DataFrame { .. }), "tidy() should return DataFrame");
        assert!(matches!(fields.get("glance").unwrap(), Value::DataFrame { .. }), "glance() should return DataFrame");
        assert!(matches!(fields.get("coef").unwrap(), Value::Vector(_)), "coef() should return Vector");
        assert!(matches!(fields.get("residuals").unwrap(), Value::Vector(_)), "residuals() should return Vector");
        assert!(matches!(fields.get("vcov").unwrap(), Value::Matrix { .. }), "vcov() should return Matrix");
    } else {
        panic!("Expected record of NEKO outputs");
    }
}
