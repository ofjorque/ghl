//! High-performance runtime engine for GHL (Generalized Hypothesis Language).
//!
//! Provides memory evaluation, Kleene three-valued logic, linear algebra operators,
//! and native statistical functions.

pub mod value;
pub mod matrix;
pub mod env;
pub mod eval;

pub use value::Value;
pub use env::RuntimeEnv;
pub use eval::Interpreter;

use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::Program;

/// Evaluates a GHL program and returns the final expression value.
pub fn eval(program: &Program) -> Result<Value, Diagnostic> {
    let mut interpreter = Interpreter::new();
    interpreter.eval_program(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghl_syntax::parser::parse;

    #[test]
    fn test_eval_arithmetic_and_kleene_na() {
        let code = r#"
            let a = 10 + 20;
            let with_na = 10 + NA:SensorDropout;
            let kleene_and_false = false && NA:NoResponse;
            let kleene_and_true = true && NA:NoResponse;
            let kleene_or_true = true || NA:SensorDropout;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(interp.env.get("a"), Some(Value::I64(30)));
        assert_eq!(
            interp.env.get("with_na"),
            Some(Value::NA(Some("SensorDropout".into())))
        );
        // Kleene: false && NA is false
        assert_eq!(interp.env.get("kleene_and_false"), Some(Value::Bool(false)));
        // Kleene: true && NA is NA
        assert_eq!(
            interp.env.get("kleene_and_true"),
            Some(Value::NA(Some("NoResponse".into())))
        );
        // Kleene: true || NA is true
        assert_eq!(interp.env.get("kleene_or_true"), Some(Value::Bool(true)));
    }

    #[test]
    fn test_eval_matrix_solve_gaussian() {
        // Linear system:
        //  1*x + 2*y = 5
        //  3*x + 4*y = 11
        // Solution: x = 1.0, y = 2.0
        let code = r#"
            let A = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
            let b = [5.0, 11.0];
            let x = A \ b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let sol = interp.env.get("x").expect("x must be computed");
        match sol {
            Value::Vector(v) => {
                assert_eq!(v.len(), 2);
                let x_val = v[0].as_f64().unwrap();
                let y_val = v[1].as_f64().unwrap();
                assert!((x_val - 1.0).abs() < 1e-6);
                assert!((y_val - 2.0).abs() < 1e-6);
            }
            _ => panic!("Expected vector solution"),
        }
    }

    #[test]
    fn test_eval_singular_matrix_emits_s0101() {
        // Collinear matrix: rows are multiples (det = 0)
        let code = r#"
            let A = mat [ 1.0, 2.0 ; 2.0, 4.0 ];
            let b = [5.0, 10.0];
            let x = A \ b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let res = interp.eval_program(&program);

        assert!(res.is_err(), "Singular matrix must error");
        let err = res.unwrap_err();
        assert_eq!(err.code, "S0101");
    }

    #[test]
    fn test_eval_pipeline_and_mean() {
        let code = r#"
            let raw = [10.0, 20.0, 30.0];
            let avg = raw |> mean();
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(interp.env.get("avg"), Some(Value::F64(20.0)));
    }

    #[test]
    fn test_eval_match_with_na_reason() {
        let code = r#"
            let s1 = match NA:SensorDropout {
                NA:SensorDropout => "sensor failed",
                NA => "generic missing",
                _ => "valid"
            };

            let s2 = match NA {
                NA:SensorDropout => "sensor failed",
                NA => "generic missing",
                _ => "valid"
            };
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(
            interp.env.get("s1"),
            Some(Value::String("sensor failed".into()))
        );
        assert_eq!(
            interp.env.get("s2"),
            Some(Value::String("generic missing".into()))
        );
    }

    #[test]
    fn test_eval_function_call_and_block() {
        let code = r#"
            fn add_bonus(score, bonus) {
                let total = score + bonus;
                total
            }

            let result = add_bonus(40, 2);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(interp.env.get("result"), Some(Value::I64(42)));
    }

    #[test]
    fn test_eval_dataframe_filter_predicate() {
        let code = r#"
            let df = dataframe {
                id: [1, 2, 3, 4],
                score: [10.0, 20.0, 30.0, 40.0]
            };
            let filtered = df |> filter(col("id") > 1);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let filtered_val = interp.env.get("filtered").expect("filtered exists");
        if let Value::DataFrame { columns: _, data } = filtered_val {
            let ids = data.get("id").expect("id column exists");
            assert_eq!(ids.len(), 3);
            assert_eq!(ids[0], Value::I64(2));
            assert_eq!(ids[1], Value::I64(3));
            assert_eq!(ids[2], Value::I64(4));
        } else {
            panic!("Expected DataFrame result");
        }
    }
}

