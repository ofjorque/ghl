//! NEKO statistical models: OLS/logistic fit, predict, vcov estimators, GLM generic verbs.

mod common;
use common::*;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_runtime::vector_data::VectorData;
use ghl_syntax::parser::parse;
use rand::{RngExt, SeedableRng};

#[test]
fn test_neko_ols_fit_and_projections() {
    // True model: y = 5.0 + 2.0*x1 - 1.0*x2
    // Observations:
    // x1 = [1, 2, 3, 4, 5, 6]
    // x2 = [2, 1, 4, 3, 6, 5]
    // y  = [5 + 2*1 - 2 = 5,
    //       5 + 2*2 - 1 = 8,
    //       5 + 2*3 - 4 = 7,
    //       5 + 2*4 - 3 = 10,
    //       5 + 2*5 - 6 = 9,
    //       5 + 2*6 - 5 = 12]
    let code = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            x2: [2.0, 1.0, 4.0, 3.0, 6.0, 5.0],
            y:  [5.0, 8.0, 7.0, 10.0, 9.0, 12.0]
        };

        let model = fit(y ~ x1 + x2, df);
        let tidied = tidy(model);
        let glanced = glance(model);
        let coefficients = coef(model);
        let pred = predict(model, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    // Verify coefficients
    let coef_val = interp.env.get("coefficients").expect("coef exists");
    if let Value::Vector(coefs) = coef_val {
        assert_eq!(coefs.len(), 3);
        let b0 = coefs[0].as_f64().unwrap();
        let b1 = coefs[1].as_f64().unwrap();
        let b2 = coefs[2].as_f64().unwrap();
        assert!((b0 - 5.0).abs() < 1e-6, "b0 should be 5.0, got {}", b0);
        assert!((b1 - 2.0).abs() < 1e-6, "b1 should be 2.0, got {}", b1);
        assert!((b2 - -1.0).abs() < 1e-6, "b2 should be -1.0, got {}", b2);
    } else {
        panic!("Expected Vector for coef");
    }

    // Verify tidy table
    let tidy_val = interp.env.get("tidied").expect("tidy exists");
    assert_eq!(
        df_columns(&tidy_val),
        vec!["term", "estimate", "std_error", "statistic", "p_value"]
    );
    let terms = df_column(&tidy_val, "term");
    assert_eq!(terms.len(), 3);
    assert_eq!(terms[0], Value::String("(Intercept)".into()));
    assert_eq!(terms[1], Value::String("x1".into()));
    assert_eq!(terms[2], Value::String("x2".into()));

    // Verify glance table
    let glance_val = interp.env.get("glanced").expect("glance exists");
    assert!(df_columns(&glance_val).contains(&"r_squared".to_string()));
    let r2 = df_column(&glance_val, "r_squared")[0].as_f64().unwrap();
    assert!(
        (r2 - 1.0).abs() < 1e-6,
        "R2 must be 1.0 for perfect linear fit"
    );

    // Verify predictions
    let pred_val = interp.env.get("pred").expect("pred exists");
    if let Value::Vector(preds) = pred_val {
        assert_eq!(preds.len(), 6);
        assert!((preds[0].as_f64().unwrap() - 5.0).abs() < 1e-6);
        assert!((preds[1].as_f64().unwrap() - 8.0).abs() < 1e-6);
    } else {
        panic!("Expected Vector for predict");
    }
}

/// Regression test: `predict()` used to map a categorical value never seen
/// while fitting to level 0 (the baseline category) via `unwrap_or(0)`,
/// silently returning a confidently wrong prediction instead of flagging
/// the row. It must now come back as NA with a reason, not a fabricated
/// number.
#[test]
fn test_predict_unseen_categorical_level_is_na_not_baseline() {
    let code = r#"
        let train = dataframe {
            region: ["A", "B", "A", "B", "A", "B"],
            y: [10.0, 20.0, 11.0, 21.0, 9.0, 19.0]
        };
        let model = ols(y ~ region, train);

        let newdata = dataframe { region: ["B", "C"], y: [0.0, 0.0] };
        let preds = predict(model, newdata);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let preds = interp.env.get("preds").expect("preds exists");
    if let Value::Vector(preds) = preds {
        assert_eq!(preds.len(), 2, "output must have one entry per input row");
        let known = preds[0]
            .as_f64()
            .expect("known level `B` must predict a real number");
        assert!(
            (known - 20.0).abs() < 1e-6,
            "region=B must predict ~20.0, got {known}"
        );
        assert!(
            preds[1].is_na(),
            "region=C (never seen while fitting) must be NA, not a fabricated prediction"
        );
    } else {
        panic!("Expected Vector for predict");
    }
}

/// Regression test: `predict()` used to silently drop rows with missing
/// predictor data from its output instead of keeping the vector aligned
/// with the input - a 3-row `newdata` with 1 NA row used to come back as
/// a 2-element vector with no indication which input row was missing.
#[test]
fn test_predict_preserves_row_alignment_with_na_input() {
    let code = r#"
        let train = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            y: [2.0, 4.0, 6.0, 8.0, 10.0, 12.0]
        };
        let model = ols(y ~ x, train);

        let newdata = dataframe { x: [1.0, NA, 3.0], y: [0.0, 0.0, 0.0] };
        let preds = predict(model, newdata);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let preds = interp.env.get("preds").expect("preds exists");
    if let Value::Vector(preds) = preds {
        assert_eq!(
            preds.len(),
            3,
            "output must stay aligned with the 3 input rows"
        );
        assert!((preds[0].as_f64().unwrap() - 2.0).abs() < 1e-6);
        assert!(
            preds[1].is_na(),
            "the row with missing `x` must be NA, not silently omitted"
        );
        assert!((preds[2].as_f64().unwrap() - 6.0).abs() < 1e-6);
    } else {
        panic!("Expected Vector for predict");
    }
}

#[test]
fn test_fit_ols_with_integer_predictor_column() {
    // `Blueprint::bake`'s fast path (post NEKO data-representation fix) checks
    // `column.dtype() == Float64` and casts otherwise -- unlike the old boxed
    // `Value::as_f64()` path, which coerced I64/F64 alike without a dtype branch.
    // `x1`/`x2` here are integer literals (`[1, 2, ...]`, no `.0`), which GHL infers
    // as an `Int64` column (`value_column_to_polars`'s dtype-widening policy) -- this
    // must still fit correctly via the cast-and-rechunk fallback in
    // `column_as_f64_view`, not silently truncate or zero out.
    let code = r#"
        let df = dataframe {
            x1: [1, 2, 3, 4, 5, 6],
            x2: [2, 1, 4, 3, 6, 5],
            y:  [5.0, 8.0, 7.0, 10.0, 9.0, 12.0]
        };
        let model = fit(y ~ x1 + x2, df);
        let coefficients = coef(model);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let coefs = vector_f64(&interp.env.get("coefficients").unwrap());
    assert_eq!(coefs.len(), 3);
    assert!(
        (coefs[0] - 5.0).abs() < 1e-6,
        "b0 should be 5.0, got {}",
        coefs[0]
    );
    assert!(
        (coefs[1] - 2.0).abs() < 1e-6,
        "b1 should be 2.0, got {}",
        coefs[1]
    );
    assert!(
        (coefs[2] - -1.0).abs() < 1e-6,
        "b2 should be -1.0, got {}",
        coefs[2]
    );
}

#[test]
fn test_neko_na_disposition_in_augment() {
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, NA:SensorDropout, 4.0, 5.0, 6.0],
            y: [2.0, 4.0, 6.0, 8.0, 10.0, 12.0]
        };

        let model = fit(y ~ x, df);
        let eval_df = augment(model, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let eval_val = interp.env.get("eval_df").expect("eval_df exists");
    let columns = df_columns(&eval_val);
    assert!(columns.contains(&".fitted".to_string()));
    assert!(columns.contains(&".used_in_fit".to_string()));
    assert!(columns.contains(&".na_reason".to_string()));

    let used = df_column(&eval_val, ".used_in_fit");
    let reasons = df_column(&eval_val, ".na_reason");

    // Row 0 (valid)
    assert_eq!(used[0], Value::Bool(true));
    assert_eq!(reasons[0], Value::String("none".into()));

    // Row 2 (NA:SensorDropout)
    assert_eq!(used[2], Value::Bool(false));
    assert_eq!(reasons[2], Value::String("SensorDropout".into()));
}

#[test]
fn test_neko_singular_matrix_emits_s0101() {
    // Collinear predictors: x2 = 2 * x1
    let code = r#"
        let df = dataframe {
            x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            x2: [2.0, 4.0, 6.0, 8.0, 10.0, 12.0],
            y:  [3.0, 5.0, 7.0, 9.0, 11.0, 13.0]
        };

        let model = fit(y ~ x1 + x2, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let res = interp.eval_program(&program);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code, "S0101");
}

#[test]
fn test_neko_vcov_hc3() {
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            y: [2.1, 3.9, 6.2, 7.8, 10.1, 12.0]
        };

        let model = fit(y ~ x, df);
        let v_hc3 = vcov(model, "HC3");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let vcov_val = interp.env.get("v_hc3").expect("v_hc3 exists");
    if let Value::Matrix { rows, cols, data } = vcov_val {
        assert_eq!(rows, 2);
        assert_eq!(cols, 2);
        assert!(data[0] > 0.0); // Var(Intercept) > 0
        assert!(data[3] > 0.0); // Var(x) > 0
    } else {
        panic!("Expected Matrix for vcov");
    }
}

#[test]
fn test_vcov_hc0_differs_from_classical_under_heteroskedasticity() {
    // Before the sandwich_vcov fix, HC0 was numerically identical to Classical
    // (both were just s2 * inv_xtx) -- with residual variance that clearly grows
    // with x, the real HC0 sandwich estimator must disagree with Classical.
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            y: [2.1, 3.9, 6.3, 7.7, 10.5, 11.5, 15.0, 13.0, 22.0, 2.0]
        };
        let model = fit(y ~ x, df);
        let v_classical = vcov(model, "Classical");
        let v_hc0 = vcov(model, "HC0");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let var_x_classical = matrix_data(&interp.env.get("v_classical").unwrap())[3];
    let var_x_hc0 = matrix_data(&interp.env.get("v_hc0").unwrap())[3];
    assert!(
        (var_x_classical - var_x_hc0).abs() > 1e-6 * var_x_classical.max(1.0),
        "HC0 ({var_x_hc0}) must differ from Classical ({var_x_classical}) under heteroskedasticity"
    );
}

#[test]
fn test_vcov_hc2_hc3_differ_via_leverage() {
    // A far outlier in x gives that row very high leverage -- before the fix,
    // HC0/HC2/HC3 were all identical (none of them used leverage at all). With a
    // real high-leverage point, HC2 (divides by 1-h_ii) and HC3 (divides by
    // (1-h_ii)^2) must diverge from HC0 and from each other.
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 100.0],
            y: [2.5, 3.5, 7.0, 7.5, 11.0, 150.0]
        };
        let model = fit(y ~ x, df);
        let v_hc0 = vcov(model, "HC0");
        let v_hc2 = vcov(model, "HC2");
        let v_hc3 = vcov(model, "HC3");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let var_x_hc0 = matrix_data(&interp.env.get("v_hc0").unwrap())[3];
    let var_x_hc2 = matrix_data(&interp.env.get("v_hc2").unwrap())[3];
    let var_x_hc3 = matrix_data(&interp.env.get("v_hc3").unwrap())[3];
    assert!(
        (var_x_hc0 - var_x_hc2).abs() > 1e-6 * var_x_hc0.max(1.0),
        "HC2 must differ from HC0 via leverage"
    );
    assert!(
        (var_x_hc2 - var_x_hc3).abs() > 1e-6 * var_x_hc2.max(1.0),
        "HC3 must differ from HC2"
    );
}

#[test]
fn test_fit_ols_parallel_assembly_matches_sequential_reference() {
    // N=60,000 is above PARALLEL_THRESHOLD (50,000, Fase 4), so this exercises
    // assemble_weighted_normal_equations' rayon fold+reduce path, not just the
    // sequential one every other OLS test uses. y is an exact linear function of
    // x1/x2 with zero noise, so the recovered coefficients must match the true
    // ones tightly -- not just "close", a real correctness check of the parallel
    // accumulation, not merely "it runs".
    let code = r#"
        let x1 = random_uniform(60000, 1);
        let x2 = random_uniform(60000, 2);
        let y = 5.0 + x1 * 2.0 - x2;
        let df = dataframe { x1: x1, x2: x2, y: y };
        let model = fit(y ~ x1 + x2, df);
        let coefficients = coef(model);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let coefs = vector_f64(&interp.env.get("coefficients").unwrap());
    assert_eq!(coefs.len(), 3);
    assert!(
        (coefs[0] - 5.0).abs() < 1e-6,
        "b0 should be 5.0, got {}",
        coefs[0]
    );
    assert!(
        (coefs[1] - 2.0).abs() < 1e-6,
        "b1 should be 2.0, got {}",
        coefs[1]
    );
    assert!(
        (coefs[2] - -1.0).abs() < 1e-6,
        "b2 should be -1.0, got {}",
        coefs[2]
    );
}

#[test]
fn test_fit_logistic_recovers_known_coefficients() {
    // GHL has no elementwise Vector comparison operator yet (`u < p` errors with
    // "Inequality comparison requires numeric operands" -- confirmed by reading
    // eval_binary_op's Lt/LtEq/Gt/GtEq arm, which only accepts scalar `as_f64()`
    // operands), so the Bernoulli response can't be simulated in GHL script alone.
    // Predictors are generated via GHL's own seeded random_normal (reusing Fase 5's
    // reproducibility machinery); the response is drawn in Rust from the true
    // model, then injected back into the same Interpreter's env before fitting --
    // the fit itself still goes through the real fit_logistic()/coef() native call
    // path, not a direct Rust call into FittedGlm.
    let true_b0 = -0.5_f64;
    let true_b1 = 1.5_f64;
    let true_b2 = -0.8_f64;
    let n = 8000;

    let gen_code = format!(
        r#"
        let x1 = random_normal({n}, 0.0, 1.0, 10);
        let x2 = random_normal({n}, 0.0, 1.0, 11);
    "#
    );
    let program = parse(&gen_code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let x1 = vector_f64(&interp.env.get("x1").unwrap());
    let x2 = vector_f64(&interp.env.get("x2").unwrap());

    let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(12345);
    let y: Vec<f64> = (0..n)
        .map(|i| {
            let eta = true_b0 + true_b1 * x1[i] + true_b2 * x2[i];
            let p = 1.0 / (1.0 + (-eta).exp());
            if rng.random::<f64>() < p { 1.0 } else { 0.0 }
        })
        .collect();
    interp
        .env
        .set("y".to_string(), Value::Vector(VectorData::from_f64(y)));

    let fit_code = r#"
        let df = dataframe { x1: x1, x2: x2, y: y };
        let model = fit_logistic(y ~ x1 + x2, df);
        let coefficients = coef(model);
        let its = glance(model);
    "#;
    let program = parse(fit_code).expect("syntax ok");
    interp.eval_program(&program).expect("evaluation ok");

    let coefs = vector_f64(&interp.env.get("coefficients").unwrap());
    assert_eq!(coefs.len(), 3);
    assert!(
        (coefs[0] - true_b0).abs() < 0.15,
        "b0 {} too far from {}",
        coefs[0],
        true_b0
    );
    assert!(
        (coefs[1] - true_b1).abs() < 0.15,
        "b1 {} too far from {}",
        coefs[1],
        true_b1
    );
    assert!(
        (coefs[2] - true_b2).abs() < 0.15,
        "b2 {} too far from {}",
        coefs[2],
        true_b2
    );
}

#[test]
fn test_fit_logistic_rejects_non_binary_response() {
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            y: [0.0, 1.0, 2.0, 0.0, 1.0, 0.0]
        };
        let model = fit_logistic(y ~ x, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("non-binary response must fail");
    assert_eq!(err.code, "S0204");
}

#[test]
fn test_fit_logistic_rejects_insufficient_df() {
    let code = r#"
        let df = dataframe {
            x1: [1.0, 2.0],
            x2: [1.0, 2.0],
            y: [0.0, 1.0]
        };
        let model = fit_logistic(y ~ x1 + x2, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("insufficient df must fail");
    assert_eq!(err.code, "S0201");
}

#[test]
fn test_fit_logistic_na_disposition_matches_ols_pattern() {
    // y deliberately doesn't separate cleanly by x (overlapping classes) -- a
    // cleanly-separating toy dataset makes the MLE not exist at all (coefficients
    // diverge), which is a real non-convergence, not a bug, but isn't what this
    // test is checking (the NA-disposition/augment() mechanics, not IRLS itself).
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, NA, 4.0, 5.0, 6.0, 7.0, 8.0],
            y: [0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0]
        };
        let model = fit_logistic(y ~ x, df);
        let aug = augment(model, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let aug = interp.env.get("aug").expect("aug exists");
    let used = df_column(&aug, ".used_in_fit");
    assert_eq!(
        used[2],
        Value::Bool(false),
        "the NA row must be marked unused"
    );
    assert_eq!(used[0], Value::Bool(true));
}

#[test]
fn test_glm_generic_verbs_dispatch_on_either_model_type() {
    let code = r#"
        let df1 = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            y: [5.0, 8.0, 7.0, 10.0, 9.0, 12.0]
        };
        let ols_model = fit(y ~ x, df1);
        let ols_coefs = coef(ols_model);

        let df2 = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            y: [0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0]
        };
        let glm_model = fit_logistic(y ~ x, df2);
        let glm_coefs = coef(glm_model);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(vector_f64(&interp.env.get("ols_coefs").unwrap()).len(), 2);
    assert_eq!(vector_f64(&interp.env.get("glm_coefs").unwrap()).len(), 2);
}

#[test]
fn test_glm_vcov_hc0_differs_from_classical() {
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            y: [0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0]
        };
        let model = fit_logistic(y ~ x, df);
        let v_classical = vcov(model, "Classical");
        let v_hc0 = vcov(model, "HC0");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let var_x_classical = matrix_data(&interp.env.get("v_classical").unwrap())[3];
    let var_x_hc0 = matrix_data(&interp.env.get("v_hc0").unwrap())[3];
    assert!(
        (var_x_classical - var_x_hc0).abs() > 1e-9 * var_x_classical.max(1.0),
        "GLM HC0 ({var_x_hc0}) must differ from Classical ({var_x_classical})"
    );
}

#[test]
fn test_fit_poisson_recovers_trend_and_verbs() {
    let code = r#"
        let df = dataframe {
            x: [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
            y: [1.0, 2.0, 4.0, 7.0, 12.0, 20.0, 33.0, 55.0, 90.0, 150.0]
        };
        let model = poisson(y ~ x, df);
        let coefs = coef(model);
        let td = tidy(model);
        let gl = glance(model);
        let sm = summary(model);
        let aug = augment(model, df);

        let new_df = dataframe { x: [2.5, 4.5] };
        let preds = predict(model, new_df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let coefs = vector_f64(&interp.env.get("coefs").unwrap());
    assert_eq!(coefs.len(), 2);
    // Intercept b0 ~ 0, slope b1 ~ 0.55 (since y grows exponentially with x)
    assert!(
        coefs[1] > 0.4 && coefs[1] < 0.7,
        "Expected positive slope for Poisson growth, got {}",
        coefs[1]
    );

    let td_val = interp.env.get("td").unwrap();
    assert!(matches!(td_val, Value::DataFrame { .. }));
    let gl_val = interp.env.get("gl").unwrap();
    assert!(matches!(gl_val, Value::DataFrame { .. }));
    let aug_val = interp.env.get("aug").unwrap();
    assert!(matches!(aug_val, Value::DataFrame { .. }));

    let preds = vector_f64(&interp.env.get("preds").unwrap());
    assert_eq!(preds.len(), 2);
    assert!(
        preds[1] > preds[0],
        "Predictions must be monotonically increasing"
    );
}

#[test]
fn test_fit_poisson_rejects_negative_response() {
    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0],
            y: [2.0, -1.0, 4.0]
        };
        let model = poisson(y ~ x, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("should reject negative count");
    assert_eq!(err.code, "S0204");
}
