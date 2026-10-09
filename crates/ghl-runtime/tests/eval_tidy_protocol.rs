use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parse;

fn eval_expr(code: &str) -> (Value, Interpreter) {
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let val = interp.eval_program(&program).expect("eval ok");
    (val, interp)
}

fn df_column_names(df_val: &Value) -> Vec<String> {
    match df_val {
        Value::DataFrame { frame, .. } => {
            frame.get_column_names().into_iter().map(|s| s.to_string()).collect()
        }
        _ => panic!("Expected DataFrame, got {:?}", df_val),
    }
}

fn df_row_count(df_val: &Value) -> usize {
    match df_val {
        Value::DataFrame { frame, .. } => frame.height(),
        _ => panic!("Expected DataFrame, got {:?}", df_val),
    }
}

#[test]
fn test_ols_universal_tidy_and_glance() {
    let code = r#"
        let df = dataframe {
            y: [2.0, 4.0, 6.0, 8.0, 10.0],
            x: [1.0, 2.0, 3.0, 4.0, 5.0]
        };
        let m = fit(y ~ x, df);

        let t = tidy(m);
        let g = glance(m);
        let a = augment(m, df);
        let p = plot(m);
    "#;
    let (_, interp) = eval_expr(code);

    let t = interp.env.get("t").expect("tidy df");
    let cols_t = df_column_names(&t);
    assert_eq!(
        cols_t,
        vec!["term", "estimate", "std_error", "statistic", "p_value"]
    );
    assert_eq!(df_row_count(&t), 2);

    let g = interp.env.get("g").expect("glance df");
    assert_eq!(df_row_count(&g), 1);
    let cols_g = df_column_names(&g);
    assert!(cols_g.contains(&"r_squared".to_string()));
    assert!(cols_g.contains(&"adj_r_squared".to_string()));
    assert!(cols_g.contains(&"log_lik".to_string()));
    assert!(cols_g.contains(&"aic".to_string()));
    assert!(cols_g.contains(&"bic".to_string()));
    assert!(cols_g.contains(&"nobs".to_string()));
    assert!(cols_g.contains(&"sigma".to_string()));

    let a = interp.env.get("a").expect("augment df");
    let cols_a = df_column_names(&a);
    assert!(cols_a.contains(&".fitted".to_string()));
    assert!(cols_a.contains(&".residual".to_string()));

    let p = interp.env.get("p").expect("plot");
    assert!(matches!(p, Value::Plot(_)));
}

#[test]
fn test_ols_direct_capability_accessors_and_pipes() {
    let code = r#"
        let df = dataframe {
            y: [10.0, 20.0, 30.0, 40.0, 50.0],
            x: [1.0, 2.0, 3.0, 4.0, 5.0]
        };
        let m = fit(y ~ x, df);

        let v_aic = aic(m);
        let v_bic = bic(m);
        let v_r2 = r_squared(m);
        let v_r2_alias = r2(m);
        let v_ar2 = adj_r_squared(m);
        let v_ll = log_lik(m);
        let v_nobs = nobs(m);
        let v_nobs_alias = n_obs(m);
        let v_fitted = fitted(m);
        let v_res = residuals(m);
        let v_coef = coef(m);

        // Pipe tests
        let pipe_aic = m |> aic();
        let pipe_r2 = m |> r_squared();
        let pipe_nobs = m |> nobs();
    "#;
    let (_, interp) = eval_expr(code);

    let v_aic = interp.env.get("v_aic").unwrap();
    assert!(matches!(v_aic, Value::F64(_)));

    let v_bic = interp.env.get("v_bic").unwrap();
    assert!(matches!(v_bic, Value::F64(_)));

    let v_r2 = interp.env.get("v_r2").unwrap();
    if let Value::F64(r2) = v_r2 {
        assert!((r2 - 1.0).abs() < 1e-6);
    } else {
        panic!("Expected F64 for r2");
    }

    let v_r2_alias = interp.env.get("v_r2_alias").unwrap();
    assert_eq!(&v_r2, &v_r2_alias);

    let v_ar2 = interp.env.get("v_ar2").unwrap();
    assert!(matches!(v_ar2, Value::F64(_)));

    let v_ll = interp.env.get("v_ll").unwrap();
    assert!(matches!(v_ll, Value::F64(_)));

    let v_nobs = interp.env.get("v_nobs").unwrap();
    assert_eq!(v_nobs, Value::I64(5));
    let v_nobs_alias = interp.env.get("v_nobs_alias").unwrap();
    assert_eq!(v_nobs_alias, Value::I64(5));

    let v_fitted = interp.env.get("v_fitted").unwrap();
    assert!(matches!(v_fitted, Value::Vector(_)));

    let v_res = interp.env.get("v_res").unwrap();
    assert!(matches!(v_res, Value::Vector(_)));

    let v_coef = interp.env.get("v_coef").unwrap();
    if let Value::Vector(ref b) = v_coef {
        assert_eq!(b.len(), 2);
    } else {
        panic!("Expected Vector for coef");
    }

    // Check pipe values match
    assert_eq!(interp.env.get("pipe_aic").unwrap(), v_aic);
    assert_eq!(interp.env.get("pipe_r2").unwrap(), v_r2);
    assert_eq!(interp.env.get("pipe_nobs").unwrap(), v_nobs);
}

#[test]
fn test_glm_logistic_capabilities_and_glance() {
    let code = r#"
        let df = dataframe {
            y: [0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0],
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
        };
        let m = logistic(y ~ x, df);

        let g = glance(m);
        let m_aic = aic(m);
        let m_bic = bic(m);
        let m_r2 = r_squared(m);
        let m_nobs = nobs(m);
        let m_fit = fitted(m);
        let m_res = residuals(m);
    "#;
    let (_, interp) = eval_expr(code);

    let g = interp.env.get("g").unwrap();
    assert_eq!(df_row_count(&g), 1);
    let cols_g = df_column_names(&g);
    assert!(cols_g.contains(&"nobs".to_string()));
    assert!(cols_g.contains(&"log_lik".to_string()));
    assert!(cols_g.contains(&"r_squared".to_string()));
    assert!(cols_g.contains(&"pseudo_r_squared".to_string()));
    assert!(cols_g.contains(&"deviance".to_string()));

    assert!(matches!(interp.env.get("m_aic").unwrap(), Value::F64(_)));
    assert!(matches!(interp.env.get("m_bic").unwrap(), Value::F64(_)));
    assert!(matches!(interp.env.get("m_r2").unwrap(), Value::F64(_)));
    assert_eq!(interp.env.get("m_nobs").unwrap(), Value::I64(8));
    assert!(matches!(interp.env.get("m_fit").unwrap(), Value::Vector(_)));
    assert!(matches!(interp.env.get("m_res").unwrap(), Value::Vector(_)));
}

#[test]
fn test_custom_struct_dynamic_introspection() {
    let code = r#"
        struct ArimaResult {
            order_p: i64,
            order_d: i64,
            order_q: i64,
            ar_coeffs: Vector,
            intercept: f64,
            sigma2: f64,
            aic: f64,
            bic: f64,
            n_obs: i64,
            residuals: Vector,
            fitted_values: Vector,
            is_stationary: bool
        }

        let model = ArimaResult {
            order_p: 1,
            order_d: 0,
            order_q: 0,
            ar_coeffs: [0.75],
            intercept: 2.5,
            sigma2: 1.2,
            aic: 142.5,
            bic: 148.1,
            n_obs: 100,
            residuals: [0.1, -0.2, 0.05],
            fitted_values: [10.1, 10.2, 10.0],
            is_stationary: true
        };

        let m_aic = aic(model);
        let m_bic = bic(model);
        let m_nobs = nobs(model);
        let m_coef = coef(model);
        let m_res = residuals(model);
        let m_fit = fitted(model);

        let g = glance(model);
        let t = tidy(model);
    "#;
    let (_, interp) = eval_expr(code);

    assert_eq!(interp.env.get("m_aic").unwrap(), Value::F64(142.5));
    assert_eq!(interp.env.get("m_bic").unwrap(), Value::F64(148.1));
    assert_eq!(interp.env.get("m_nobs").unwrap(), Value::I64(100));

    let m_coef = interp.env.get("m_coef").unwrap();
    if let Value::Vector(ref b) = m_coef {
        assert_eq!(b.len(), 1);
        assert_eq!(b[0], Value::F64(0.75));
    } else {
        panic!("Expected Vector for coef");
    }

    // Dynamic glance harvests all scalar fields!
    let g = interp.env.get("g").unwrap();
    assert_eq!(df_row_count(&g), 1);
    let cols_g = df_column_names(&g);
    assert!(cols_g.contains(&"aic".to_string()));
    assert!(cols_g.contains(&"bic".to_string()));
    assert!(cols_g.contains(&"nobs".to_string()));
    assert!(cols_g.contains(&"sigma".to_string()));
    assert!(cols_g.contains(&"is_stationary".to_string()));
    assert!(cols_g.contains(&"order_p".to_string()));

    // Dynamic tidy harvests vector coefficients
    let t = interp.env.get("t").unwrap();
    assert_eq!(df_row_count(&t), 1);
    let cols_t = df_column_names(&t);
    assert_eq!(
        cols_t,
        vec!["term", "estimate", "std_error", "statistic", "p_value"]
    );
}

#[test]
fn test_single_estimate_did_struct_introspection() {
    let code = r#"
        struct DidResult {
            att: f64,
            std_error: f64,
            t_stat: f64,
            p_value: f64,
            conf_low: f64,
            conf_high: f64,
            n_obs: i64,
            is_significant: bool
        }

        let did = DidResult {
            att: -3.45,
            std_error: 0.82,
            t_stat: -4.207,
            p_value: 0.0001,
            conf_low: -5.05,
            conf_high: -1.84,
            n_obs: 250,
            is_significant: true
        };

        let t = tidy(did);
        let g = glance(did);
        let c = coef(did);
        let n = nobs(did);
    "#;
    let (_, interp) = eval_expr(code);

    let t = interp.env.get("t").unwrap();
    assert_eq!(df_row_count(&t), 1);
    let cols_t = df_column_names(&t);
    assert!(cols_t.contains(&"term".to_string()));
    assert!(cols_t.contains(&"estimate".to_string()));
    assert!(cols_t.contains(&"std_error".to_string()));
    assert!(cols_t.contains(&"statistic".to_string()));
    assert!(cols_t.contains(&"p_value".to_string()));
    assert!(cols_t.contains(&"conf_low".to_string()));
    assert!(cols_t.contains(&"conf_high".to_string()));

    let g = interp.env.get("g").unwrap();
    assert_eq!(df_row_count(&g), 1);
    let cols_g = df_column_names(&g);
    assert!(cols_g.contains(&"nobs".to_string()));

    let c = interp.env.get("c").unwrap();
    if let Value::Vector(ref b) = c {
        assert_eq!(b.len(), 1);
        assert_eq!(b[0], Value::F64(-3.45));
    } else {
        panic!("Expected Vector for coef");
    }

    assert_eq!(interp.env.get("n").unwrap(), Value::I64(250));
}

#[test]
fn test_explicit_method_override_on_struct() {
    let code = r#"
        struct MyModel {
            raw_loss: f64
        }

        impl MyModel {
            fn aic(self) {
                self.raw_loss + 10.0
            }
            fn glance(self) {
                let df = dataframe {
                    loss: [self.raw_loss]
                };
                df
            }
            fn plot(self) {
                let p = plot();
                p
            }
        }

        let m = MyModel { raw_loss: 42.0 };
        let custom_aic = aic(m);
        let custom_glance = glance(m);
        let custom_plot = plot(m);
    "#;
    let (_, interp) = eval_expr(code);

    assert_eq!(interp.env.get("custom_aic").unwrap(), Value::F64(52.0));

    let g = interp.env.get("custom_glance").unwrap();
    assert_eq!(df_column_names(&g), vec!["loss"]);

    let p = interp.env.get("custom_plot").unwrap();
    assert!(matches!(p, Value::Plot(_)));
}

#[test]
fn test_error_diagnostics_e0501_and_e0502() {
    let code_missing_cap = r#"
        struct EmptyModel {}
        let m = EmptyModel {};
        aic(m)
    "#;
    let program = parse(code_missing_cap).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp.eval_program(&program).unwrap_err();
    assert_eq!(err.code, "E0502");
    assert!(err.message.contains("EmptyModel"));
    assert!(err.message.contains("aic"));

    let code_missing_tidy = r#"
        struct EmptyModel {}
        let m = EmptyModel {};
        tidy(m)
    "#;
    let program2 = parse(code_missing_tidy).expect("syntax ok");
    let mut interp2 = Interpreter::new();
    let err2 = interp2.eval_program(&program2).unwrap_err();
    assert_eq!(err2.code, "E0501");
    assert!(err2.message.contains("EmptyModel"));

    let code_missing_glance = r#"
        struct EmptyModel {}
        let m = EmptyModel {};
        glance(m)
    "#;
    let program3 = parse(code_missing_glance).expect("syntax ok");
    let mut interp3 = Interpreter::new();
    let err3 = interp3.eval_program(&program3).unwrap_err();
    assert_eq!(err3.code, "E0501");
    assert!(err3.message.contains("EmptyModel"));

    let code_missing_plot = r#"
        struct EmptyModel {}
        let m = EmptyModel {};
        plot(m)
    "#;
    let program4 = parse(code_missing_plot).expect("syntax ok");
    let mut interp4 = Interpreter::new();
    let err4 = interp4.eval_program(&program4).unwrap_err();
    assert_eq!(err4.code, "E0501");
    assert!(err4.message.contains("EmptyModel"));
}
