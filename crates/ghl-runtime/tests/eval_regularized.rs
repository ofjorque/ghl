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
fn test_lasso_variable_selection() {
    let src = r#"
        // Synthetic dataset:
        // True model: y = 2.0 + 3.0 * x1 + 4.0 * x2 + 0.0 * x3 + 0.0 * x4
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            x2: [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0],
            x3: [0.1, -0.1, 0.05, -0.05, 0.02, -0.02, 0.04, -0.04, 0.01, -0.01], // Uncorrelated noise
            x4: [-0.05, 0.05, -0.02, 0.02, -0.01, 0.01, -0.03, 0.03, -0.01, 0.01], // Uncorrelated noise
            // y = 2.0 + 3.0 * x1 + 4.0 * x2
            y:  [9.0, 8.0, 15.0, 14.0, 21.0, 20.0, 27.0, 26.0, 33.0, 32.0]
        };

        // Fit Lasso with penalty lambda = 0.2
        let model = lasso(y ~ x1 + x2 + x3 + x4, df, 0.2);
        model
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("lasso should succeed");
    if let Value::Struct { name, fields } = val {
        assert_eq!(name, "RegularizedResult");
        let m_type = fields.get("model_type").expect("model_type").as_str().unwrap();
        assert_eq!(m_type, "Lasso");

        let n_sel = fields.get("n_selected").expect("n_selected").as_i64().unwrap();
        let coefs = fields.get("coefficients").expect("coefficients");
        let active = fields.get("active_terms").expect("active_terms");

        if let Value::Vector(c_vec) = coefs {
            // [intercept, x1, x2, x3, x4]
            assert_eq!(c_vec.len(), 5);
            let b_x1 = c_vec[1].as_f64().unwrap();
            let b_x2 = c_vec[2].as_f64().unwrap();
            let b_x3 = c_vec[3].as_f64().unwrap();
            let b_x4 = c_vec[4].as_f64().unwrap();

            // x1 and x2 should be selected with positive coefficients
            assert!(b_x1 > 1.0, "Expected x1 to be selected (>1), got {}", b_x1);
            assert!(b_x2 > 1.0, "Expected x2 to be selected (>1), got {}", b_x2);

            // x3 and x4 should be shrunk towards zero or exactly zero
            assert!(b_x3.abs() < 1e-4, "Expected x3 to be shrunk to 0, got {}", b_x3);
            assert!(b_x4.abs() < 1e-4, "Expected x4 to be shrunk to 0, got {}", b_x4);
        } else {
            panic!("Expected Vector for coefficients");
        }

        if let Value::Vector(a_vec) = active {
            assert_eq!(a_vec.len(), n_sel as usize);
            let active_names: Vec<String> = a_vec.iter().map(|v| v.as_str().unwrap().to_string()).collect();
            assert!(active_names.contains(&"x1".to_string()));
            assert!(active_names.contains(&"x2".to_string()));
            assert!(!active_names.contains(&"x3".to_string()));
            assert!(!active_names.contains(&"x4".to_string()));
        }
    } else {
        panic!("Expected RegularizedResult struct");
    }
}

#[test]
fn test_ridge_shrinkage() {
    let src = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            x2: [2.0, 3.0, 5.0, 6.0, 8.0, 9.0, 11.0, 12.0],
            y:  [5.0, 8.0, 13.0, 16.0, 21.0, 24.0, 29.0, 32.0]
        };

        // Fit Ridge (alpha = 0.0) with lambda = 1.0
        let model = ridge(y ~ x1 + x2, df, 1.0);
        model
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("ridge should succeed");
    if let Value::Struct { name, fields } = val {
        assert_eq!(name, "RegularizedResult");
        let m_type = fields.get("model_type").expect("model_type").as_str().unwrap();
        assert_eq!(m_type, "Ridge");

        let alpha = fields.get("alpha").expect("alpha").as_f64().unwrap();
        assert_eq!(alpha, 0.0);

        let n_sel = fields.get("n_selected").expect("n_selected").as_i64().unwrap();
        // In Ridge, coefficients shrink but generally all features remain non-zero
        assert_eq!(n_sel, 2);

        let r2 = fields.get("r2").expect("r2").as_f64().unwrap();
        assert!(r2 > 0.85, "Expected high R2 (>0.85), got {}", r2);
    } else {
        panic!("Expected RegularizedResult struct");
    }
}

#[test]
fn test_elastic_net_mixture() {
    let src = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            x2: [2.0, 4.0, 5.0, 7.0, 9.0, 11.0],
            y:  [3.0, 6.0, 8.0, 11.0, 14.0, 17.0]
        };

        // Fit ElasticNet with lambda = 0.2 and alpha = 0.5
        let model = elastic_net(y ~ x1 + x2, df, 0.2, 0.5);
        model
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("elastic_net should succeed");
    if let Value::Struct { name, fields } = val {
        assert_eq!(name, "RegularizedResult");
        let m_type = fields.get("model_type").expect("model_type").as_str().unwrap();
        assert_eq!(m_type, "ElasticNet");

        let alpha = fields.get("alpha").expect("alpha").as_f64().unwrap();
        assert_eq!(alpha, 0.5);

        let lam = fields.get("lambda").expect("lambda").as_f64().unwrap();
        assert!((lam - 0.2).abs() < 1e-6);
    } else {
        panic!("Expected RegularizedResult struct");
    }
}

#[test]
fn test_cv_glmnet_auto_tuning() {
    let src = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
            x2: [2.0, 3.0, 5.0, 6.0, 8.0, 9.0, 11.0, 12.0, 14.0, 15.0, 17.0, 18.0],
            y:  [3.0, 5.0, 8.0, 10.0, 13.0, 15.0, 18.0, 20.0, 23.0, 25.0, 28.0, 30.0]
        };

        // Calling cv_glmnet automatically tunes lambda via K-fold CV
        let cv_model = cv_glmnet(y ~ x1 + x2, df, 1.0, 3); // 3 folds
        cv_model
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("cv_glmnet should succeed");
    if let Value::Struct { name, fields } = val {
        assert_eq!(name, "RegularizedResult");

        let lam_min = fields.get("lambda_min").expect("lambda_min");
        let lam_1se = fields.get("lambda_1se").expect("lambda_1se");
        let cv_metrics = fields.get("cv_metrics").expect("cv_metrics");

        assert!(!lam_min.is_na(), "lambda_min should be computed");
        assert!(!lam_1se.is_na(), "lambda_1se should be computed");

        let l_min = lam_min.as_f64().unwrap();
        let l_1se = lam_1se.as_f64().unwrap();
        assert!(l_min > 0.0);
        assert!(l_1se >= l_min, "lambda_1se ({}) should be >= lambda_min ({})", l_1se, l_min);

        if let Value::DataFrame { frame, .. } = cv_metrics {
            assert!(frame.height() > 0, "cv_metrics should have rows");
            assert!(frame.column("lambda").is_ok());
            assert!(frame.column("mean_mse").is_ok());
            assert!(frame.column("se_mse").is_ok());
        } else {
            panic!("Expected DataFrame for cv_metrics");
        }
    } else {
        panic!("Expected RegularizedResult struct");
    }
}

#[test]
fn test_neko_verbs_and_predict() {
    let src = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            x2: [2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
            y:  [4.0, 7.0, 10.0, 13.0, 16.0, 19.0]
        };

        let model = lasso(y ~ x1 + x2, df, 0.1);

        let t = tidy(model);
        let g = glance(model);
        let c = coef(model);
        let r = residuals(model);
        summary(model);

        // Predict on new data
        let new_df = dataframe {
            x1: [7.0, 8.0],
            x2: [8.0, 9.0]
        };
        let preds = predict(model, new_df);

        { tidy: t, glance: g, coef: c, residuals: r, preds: preds }
    "#;

    let (_, res) = run_ghl(src);
    let val = res.expect("neko verbs should succeed");
    if let Value::Record(map) = val {
        let t = map.get("tidy").expect("tidy");
        let g = map.get("glance").expect("glance");
        let c = map.get("coef").expect("coef");
        let r = map.get("residuals").expect("residuals");
        let p = map.get("preds").expect("preds");

        // tidy has 3 rows: (Intercept), x1, x2
        if let Value::DataFrame { frame, .. } = t {
            assert_eq!(frame.height(), 3);
            assert!(frame.column("term").is_ok());
            assert!(frame.column("estimate").is_ok());
            assert!(frame.column("is_selected").is_ok());
        } else {
            panic!("Expected DataFrame for tidy");
        }

        // glance has 1 row
        if let Value::DataFrame { frame, .. } = g {
            assert_eq!(frame.height(), 1);
            assert!(frame.column("model_type").is_ok());
            assert!(frame.column("alpha").is_ok());
            assert!(frame.column("lambda").is_ok());
            assert!(frame.column("r2").is_ok());
        } else {
            panic!("Expected DataFrame for glance");
        }

        // coef has 3 elements
        if let Value::Vector(v) = c {
            assert_eq!(v.len(), 3);
        } else {
            panic!("Expected Vector for coef");
        }

        // residuals has 6 elements
        if let Value::Vector(v) = r {
            assert_eq!(v.len(), 6);
        } else {
            panic!("Expected Vector for residuals");
        }

        // predictions on 2 new rows
        if let Value::Vector(v) = p {
            assert_eq!(v.len(), 2);
            let p0 = v[0].as_f64().unwrap();
            let p1 = v[1].as_f64().unwrap();
            // Since true model is y = 1 + 2*x1 + 1*x2:
            // For row 0: 1 + 2*7 + 8 = 23 (approx)
            // For row 1: 1 + 2*8 + 9 = 26 (approx)
            assert!(p0 > 15.0 && p0 < 30.0, "p0 out of range: {}", p0);
            assert!(p1 > p0, "p1 should be greater than p0");
        } else {
            panic!("Expected Vector for predict");
        }
    } else {
        panic!("Expected Record");
    }
}

#[test]
fn test_regularized_error_handling() {
    // 1. Missing predictors
    let src1 = r#"
        let df = dataframe { y: [1.0, 2.0, 3.0] };
        lasso(y ~ 1, df)
    "#;
    let (_, res1) = run_ghl(src1);
    assert!(res1.is_err(), "Formula without predictors should fail");

    // 2. Insufficient observations
    let src2 = r#"
        let df = dataframe {
            x: [1.0, 2.0],
            y: [3.0, 4.0]
        };
        lasso(y ~ x, df)
    "#;
    let (_, res2) = run_ghl(src2);
    assert!(res2.is_err(), "Insufficient observations should fail");
}
