//! SEM/CFA syntax plumbing: `=~` (measurement), `~~` (covariance), and `sem_spec { ... }`.
//! The `sem()` estimator itself isn't implemented yet (TODO.md Parte A) -- these tests
//! only cover parsing/evaluating the specification into `Value::Formula`/`Value::SemSpec`,
//! and that `fit()`/`fit_logistic()` correctly reject non-regression formulas.

use ghl_runtime::value::Value;
use ghl_runtime::eval::Interpreter;
use ghl_syntax::ast::FormulaOp;
use ghl_syntax::parser::parse;

#[test]
fn test_measurement_and_covariance_operators_evaluate_to_formula() {
    let code = r#"
        let m = f1 =~ x1 + x2 + x3;
        let c = x1 ~~ x2;
        let r = y ~ x1 + x2;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("m").expect("m exists") {
        Value::Formula { op, response, terms, .. } => {
            assert_eq!(op, FormulaOp::Measurement);
            assert_eq!(response, "f1");
            assert_eq!(terms, vec!["x1".to_string(), "x2".to_string(), "x3".to_string()]);
        }
        other => panic!("Expected Formula, got {other:?}"),
    }

    match interp.env.get("c").expect("c exists") {
        Value::Formula { op, response, terms, .. } => {
            assert_eq!(op, FormulaOp::Covariance);
            assert_eq!(response, "x1");
            assert_eq!(terms, vec!["x2".to_string()]);
        }
        other => panic!("Expected Formula, got {other:?}"),
    }

    match interp.env.get("r").expect("r exists") {
        Value::Formula { op, .. } => assert_eq!(op, FormulaOp::Regression),
        other => panic!("Expected Formula, got {other:?}"),
    }
}

#[test]
fn test_sem_spec_block_evaluates_to_ordered_formula_list() {
    let code = r#"
        let spec = sem_spec {
            f1 =~ x1 + x2 + x3;
            f2 =~ y1 + y2 + y3;
            f1 ~~ f2;
            f2 ~ f1;
        };
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("spec").expect("spec exists") {
        Value::SemSpec(equations) => {
            assert_eq!(equations.len(), 4);
            let ops: Vec<FormulaOp> = equations
                .iter()
                .map(|e| match e {
                    Value::Formula { op, .. } => *op,
                    other => panic!("Expected Formula equation, got {other:?}"),
                })
                .collect();
            assert_eq!(
                ops,
                vec![
                    FormulaOp::Measurement,
                    FormulaOp::Measurement,
                    FormulaOp::Covariance,
                    FormulaOp::Regression,
                ]
            );
        }
        other => panic!("Expected SemSpec, got {other:?}"),
    }
}

#[test]
fn test_fit_rejects_measurement_formula() {
    let code = r#"
        let df = dataframe { x1: [1.0, 2.0, 3.0], y: [1.0, 2.0, 3.0] };
        let model = fit(y =~ x1, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp.eval_program(&program).expect_err("fit() must reject a =~ formula");
    assert_eq!(err.code, "S0200");
}

#[test]
fn test_fit_logistic_rejects_covariance_formula() {
    let code = r#"
        let df = dataframe { x1: [1.0, 2.0, 3.0], y: [0, 1, 0] };
        let model = fit_logistic(y ~~ x1, df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("fit_logistic() must reject a ~~ formula");
    assert_eq!(err.code, "S0200");
}

#[test]
fn test_irt_spec_with_constraints_evaluates_correctly() {
    let code = r#"
        let spec = irt_spec {
            Math =~ m1 + m2 + m3 + m4;
            Verbal =~ v1 + v2 + v3 + v4;
            Math ~~ Verbal;
            m1.a == m2.a;
            m3.a == 1.0;
            m4.c == 0.20;
        };
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("spec").expect("spec exists") {
        Value::SemSpec(eqs) => {
            assert_eq!(eqs.len(), 6);
            match &eqs[0] {
                Value::Formula { op, response, terms, .. } => {
                    assert_eq!(*op, FormulaOp::Measurement);
                    assert_eq!(response, "Math");
                    assert_eq!(terms, &vec!["m1", "m2", "m3", "m4"]);
                }
                other => panic!("Expected Measurement formula, got {other:?}"),
            }
            match &eqs[1] {
                Value::Formula { op, response, terms, .. } => {
                    assert_eq!(*op, FormulaOp::Measurement);
                    assert_eq!(response, "Verbal");
                    assert_eq!(terms, &vec!["v1", "v2", "v3", "v4"]);
                }
                other => panic!("Expected Measurement formula, got {other:?}"),
            }
            match &eqs[2] {
                Value::Formula { op, response, terms, .. } => {
                    assert_eq!(*op, FormulaOp::Covariance);
                    assert_eq!(response, "Math");
                    assert_eq!(terms, &vec!["Verbal"]);
                }
                other => panic!("Expected Covariance formula, got {other:?}"),
            }
            match &eqs[3] {
                Value::Formula { op, response, terms, .. } => {
                    assert_eq!(*op, FormulaOp::Constraint);
                    assert_eq!(response, "m1.a");
                    assert_eq!(terms, &vec!["m2.a"]);
                }
                other => panic!("Expected Constraint formula, got {other:?}"),
            }
            match &eqs[4] {
                Value::Formula { op, response, terms, .. } => {
                    assert_eq!(*op, FormulaOp::Constraint);
                    assert_eq!(response, "m3.a");
                    assert_eq!(terms, &vec!["1"]);
                }
                other => panic!("Expected Constraint formula, got {other:?}"),
            }
            match &eqs[5] {
                Value::Formula { op, response, terms, .. } => {
                    assert_eq!(*op, FormulaOp::Constraint);
                    assert_eq!(response, "m4.c");
                    assert_eq!(terms, &vec!["0.2"]);
                }
                other => panic!("Expected Constraint formula, got {other:?}"),
            }
        }
        other => panic!("Expected SemSpec, got {other:?}"),
    }
}

