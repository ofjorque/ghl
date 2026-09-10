//! High-performance runtime engine for GHL (Generalized Hypothesis Language).
//!
//! Provides memory evaluation, Kleene three-valued logic, linear algebra operators,
//! and native statistical functions.

pub mod value;
pub mod matrix;
pub mod env;
pub mod eval;
pub mod neko;
pub mod io;
pub mod na_reasons;
pub mod polars_bridge;
pub mod vector_data;

pub use value::Value;
pub use env::RuntimeEnv;
pub use eval::Interpreter;
pub use neko::{Blueprint, FittedModel, RowDisposition, VcovKind};

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
    use crate::polars_bridge;

    /// Test helper: extract a column's values from a `Value::DataFrame`, panicking with
    /// a clear message if `v` isn't one or the column doesn't exist — keeps assertions
    /// below focused on what they're checking rather than on the polars accessor API.
    fn df_column(v: &Value, col: &str) -> Vec<Value> {
        match v {
            Value::DataFrame { frame, na_reasons } => {
                polars_bridge::pull_column_as_values(frame, na_reasons, col)
                    .unwrap_or_else(|e| panic!("column `{col}` not found: {e:?}"))
            }
            other => panic!("Expected DataFrame, found `{}`", other.type_name()),
        }
    }

    fn df_columns(v: &Value) -> Vec<String> {
        match v {
            Value::DataFrame { frame, .. } => frame.get_column_names().iter().map(|s| s.to_string()).collect(),
            other => panic!("Expected DataFrame, found `{}`", other.type_name()),
        }
    }

    fn df_height(v: &Value) -> usize {
        match v {
            Value::DataFrame { frame, .. } => frame.height(),
            other => panic!("Expected DataFrame, found `{}`", other.type_name()),
        }
    }

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
    fn test_matrix_multiplication_real_product() {
        // TODO.md Fase 3, Caso 1.2: `A * B` on two Matrix values used to fall straight
        // through to a "cannot apply Mul" error -- MatrixOps::mul existed but nothing in
        // the interpreter ever called it. Now wired to faer's `*` operator.
        //   A (2x3)      B (3x2)         A*B (2x2)
        //   [1 2 3]      [ 7  8]         [1*7+2*9+3*11  1*8+2*10+3*12]   [58  64]
        //   [4 5 6]      [ 9 10]    =    [4*7+5*9+6*11  4*8+5*10+6*12] = [139 154]
        //                [11 12]
        let code = r#"
            let a = mat [ 1.0, 2.0, 3.0 ; 4.0, 5.0, 6.0 ];
            let b = mat [ 7.0, 8.0 ; 9.0, 10.0 ; 11.0, 12.0 ];
            let c = a * b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("c").expect("c must be computed") {
            Value::Matrix { rows, cols, data } => {
                assert_eq!((rows, cols), (2, 2));
                let expected = [58.0, 64.0, 139.0, 154.0];
                for (got, want) in data.iter().zip(expected.iter()) {
                    assert!((got - want).abs() < 1e-9, "got {data:?}, expected {expected:?}");
                }
            }
            other => panic!("Expected Matrix, found {other:?}"),
        }
    }

    #[test]
    fn test_matrix_multiplication_rejects_non_conformable_dimensions() {
        let code = r#"
            let a = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
            let b = mat [ 1.0, 2.0, 3.0 ; 4.0, 5.0, 6.0 ; 7.0, 8.0, 9.0 ];
            let c = a * b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("2x2 * 3x3 must fail");
        assert_eq!(err.code, "S0412");
    }

    fn matrix_data(v: &Value) -> Vec<f64> {
        match v {
            Value::Matrix { data, .. } => data.clone(),
            other => panic!("Expected Matrix, found {other:?}"),
        }
    }

    fn vector_f64(v: &Value) -> Vec<f64> {
        match v {
            Value::Vector(items) => items.iter().map(|x| x.as_f64().unwrap()).collect(),
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_qr_decomposition_reconstructs_original_matrix() {
        // TODO.md Fase 3: qr()/qr_q()/qr_r() over faer's thin QR. A (3x2, tall) must
        // reconstruct as Q * R via GHL's own (now-wired) real matrix multiplication.
        let code = r#"
            let a = mat [ 1.0, 2.0 ; 3.0, 4.0 ; 5.0, 6.0 ];
            let decomp = qr(a);
            let q = qr_q(decomp);
            let r = qr_r(decomp);
            let reconstructed = q * r;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let a = matrix_data(&interp.env.get("a").unwrap());
        let reconstructed = matrix_data(&interp.env.get("reconstructed").unwrap());
        for (got, want) in reconstructed.iter().zip(a.iter()) {
            assert!((got - want).abs() < 1e-9, "QR reconstruction mismatch: {reconstructed:?} vs {a:?}");
        }
    }

    #[test]
    fn test_cholesky_reconstructs_symmetric_positive_definite_matrix() {
        let code = r#"
            let a = mat [ 4.0, 2.0 ; 2.0, 3.0 ];
            let l = cholesky(a);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let l = matrix_data(&interp.env.get("l").unwrap());
        let (l11, l12, l21, l22) = (l[0], l[1], l[2], l[3]);
        assert!(l12.abs() < 1e-12, "L must be lower triangular, got {l:?}");
        let reconstructed = [
            l11 * l11 + l12 * l12, l11 * l21 + l12 * l22,
            l21 * l11 + l22 * l12, l21 * l21 + l22 * l22,
        ];
        let expected = [4.0, 2.0, 2.0, 3.0];
        for (got, want) in reconstructed.iter().zip(expected.iter()) {
            assert!((got - want).abs() < 1e-9, "L * Lt mismatch: {reconstructed:?} vs {expected:?}");
        }
    }

    #[test]
    fn test_cholesky_rejects_asymmetric_matrix() {
        let code = r#"
            let a = mat [ 1.0, 2.0 ; 999.0, 3.0 ];
            let l = cholesky(a);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("asymmetric matrix must be rejected");
        assert_eq!(err.code, "S0412");
    }

    #[test]
    fn test_cholesky_rejects_non_positive_definite_matrix() {
        // Symmetric but indefinite (eigenvalues 3 and -1) -- a real Cholesky failure, not
        // the symmetry check above.
        let code = r#"
            let a = mat [ 1.0, 2.0 ; 2.0, 1.0 ];
            let l = cholesky(a);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("indefinite matrix must be rejected");
        assert_eq!(err.code, "S0101");
    }

    #[test]
    fn test_eigen_symmetric_matrix() {
        let code = r#"
            let a = mat [ 2.0, 0.0 ; 0.0, 5.0 ];
            let decomp = eigen(a);
            let values = eigen_values(decomp);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let values = vector_f64(&interp.env.get("values").unwrap());
        assert_eq!(values.len(), 2);
        // faer returns eigenvalues in nondecreasing order.
        assert!((values[0] - 2.0).abs() < 1e-9);
        assert!((values[1] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_eigen_rejects_asymmetric_matrix() {
        let code = r#"
            let a = mat [ 1.0, 2.0 ; 999.0, 3.0 ];
            let decomp = eigen(a);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("asymmetric matrix must be rejected");
        assert_eq!(err.code, "S0412");
    }

    #[test]
    fn test_svd_reconstructs_matrix() {
        let code = r#"
            let a = mat [ 3.0, 0.0 ; 4.0, 5.0 ; 0.0, 0.0 ];
            let decomp = svd(a);
            let u = svd_u(decomp);
            let s = svd_s(decomp);
            let v = svd_v(decomp);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let (u_rows, u_cols, u) = match interp.env.get("u").unwrap() {
            Value::Matrix { rows, cols, data } => (rows, cols, data),
            other => panic!("Expected Matrix, found {other:?}"),
        };
        let s = vector_f64(&interp.env.get("s").unwrap());
        let (v_rows, v_cols, v) = match interp.env.get("v").unwrap() {
            Value::Matrix { rows, cols, data } => (rows, cols, data),
            other => panic!("Expected Matrix, found {other:?}"),
        };
        assert_eq!((u_rows, u_cols), (3, 2));
        assert_eq!((v_rows, v_cols), (2, 2));
        assert_eq!(s.len(), 2);
        assert!(s[0] >= s[1], "singular values must be nonincreasing: {s:?}");

        // Reconstruct A = U * diag(S) * V^T by hand (no transpose()/diag() builtin yet).
        let a = matrix_data(&interp.env.get("a").unwrap());
        let mut reconstructed = vec![0.0; 3 * 2];
        for i in 0..3 {
            for j in 0..2 {
                let mut acc = 0.0;
                for k in 0..2 {
                    acc += u[i * u_cols + k] * s[k] * v[j * v_cols + k];
                }
                reconstructed[i * 2 + j] = acc;
            }
        }
        for (got, want) in reconstructed.iter().zip(a.iter()) {
            assert!((got - want).abs() < 1e-9, "SVD reconstruction mismatch: {reconstructed:?} vs {a:?}");
        }
    }

    #[test]
    fn test_dot_product_real_computation() {
        // TODO.md Fase 3, Track 2, Punto 2 -- [1,2,3] . [4,5,6] = 4+10+18 = 32.
        let code = r#"
            let a = [1.0, 2.0, 3.0];
            let b = [4.0, 5.0, 6.0];
            let d = dot(a, b);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");
        assert_eq!(interp.env.get("d"), Some(Value::F64(32.0)));
    }

    #[test]
    fn test_dot_product_rejects_mismatched_lengths() {
        let code = r#"
            let a = [1.0, 2.0, 3.0];
            let b = [4.0, 5.0];
            let d = dot(a, b);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("mismatched lengths must fail");
        assert_eq!(err.code, "S0412");
    }

    #[test]
    fn test_dot_product_propagates_na_with_reason() {
        let code = r#"
            let a = [1.0, NA:SensorDropout, 3.0];
            let b = [4.0, 5.0, 6.0];
            let d = dot(a, b);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");
        assert_eq!(interp.env.get("d"), Some(Value::NA(Some("SensorDropout".into()))));
    }

    #[test]
    fn test_mean_sum_preserve_na_reason_over_vector() {
        // Regression lock for TODO.md Fase 3 Punto 2's explicit design decision: a
        // standalone mean()/sum() over a Vector propagates the *specific* NA reason (not
        // a generic NA(None)) -- this was true before Punto 2's native-reduce migration
        // too, but was never actually asserted by a test until now.
        let code = r#"
            let v = [1.0, NA:SensorDropout, 3.0];
            let m = mean(v);
            let s = sum(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");
        assert_eq!(interp.env.get("m"), Some(Value::NA(Some("SensorDropout".into()))));
        assert_eq!(interp.env.get("s"), Some(Value::NA(Some("SensorDropout".into()))));
    }

    #[test]
    fn test_map_applies_closure_over_vector() {
        // TODO.md Fase 3, Track 2, Punto 3, Caso 1.4's exact expression (in GHL's real
        // lambda syntax, `\xi -> ...`, not the aspirational `xi => ...` the roadmap text
        // used informally).
        let code = r#"
            let x = [1.0, -2.0, 0.5];
            let y = x |> map(\xi -> log(1.0 + exp(-abs(xi))) + sin(xi));
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let y = vector_f64(&interp.env.get("y").unwrap());
        let expected: Vec<f64> = [1.0f64, -2.0, 0.5]
            .iter()
            .map(|&xi| (1.0 + (-xi.abs()).exp()).ln() + xi.sin())
            .collect();
        assert_eq!(y.len(), 3);
        for (got, want) in y.iter().zip(expected.iter()) {
            assert!((got - want).abs() < 1e-9, "map() mismatch: {y:?} vs {expected:?}");
        }
    }

    #[test]
    fn test_map_with_native_fn() {
        // map()'s second argument doesn't have to be a closure -- an existing builtin
        // works directly, exercising the Value::NativeFn branch (not Value::Closure).
        let code = r#"
            let x = [4.0, 9.0, 16.0];
            let y = map(x, sqrt);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let y = vector_f64(&interp.env.get("y").unwrap());
        assert_eq!(y, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_map_preserves_per_element_na() {
        // sqrt(-1.0) is NaN-safe (gives NA, never panics, per the existing math helpers)
        // -- map() must keep that NA at its own position without disturbing the others.
        let code = r#"
            let x = [4.0, -1.0, 9.0];
            let y = map(x, \xi -> sqrt(xi));
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("y").unwrap() {
            Value::Vector(vd) => {
                assert_eq!(vd.value_at(0), Some(Value::F64(2.0)));
                assert!(matches!(vd.value_at(1), Some(Value::NA(_))), "expected NA at index 1, got {:?}", vd.value_at(1));
                assert_eq!(vd.value_at(2), Some(Value::F64(3.0)));
            }
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_map_rejects_non_vector_first_argument() {
        let code = r#"
            let y = map(5.0, \xi -> xi * 2.0);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("non-Vector first argument must fail");
        assert_eq!(err.code, "C0202");
    }

    #[test]
    fn test_map_rejects_non_callable_second_argument() {
        let code = r#"
            let x = [1.0, 2.0];
            let y = map(x, 5.0);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("non-callable second argument must fail");
        assert_eq!(err.code, "C0203");
    }

    #[test]
    fn test_scalar_vector_arithmetic_is_symmetric() {
        // TODO.md Fase 3, Track 2 follow-up: `scalar op Vector` used to fail while
        // `Vector op scalar` worked -- and non-commutative ops (`-`/`/`) must flip
        // direction, not just accept the reversed order.
        let code = r#"
            let v = [1.0, 2.0, 4.0];
            let a = 10.0 + v;
            let b = v + 10.0;
            let c = 10.0 - v;
            let d = 100.0 / v;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(vector_f64(&interp.env.get("a").unwrap()), vec![11.0, 12.0, 14.0]);
        assert_eq!(vector_f64(&interp.env.get("a").unwrap()), vector_f64(&interp.env.get("b").unwrap()));
        assert_eq!(vector_f64(&interp.env.get("c").unwrap()), vec![9.0, 8.0, 6.0]);
        assert_eq!(vector_f64(&interp.env.get("d").unwrap()), vec![100.0, 50.0, 25.0]);
    }

    #[test]
    fn test_vector_vector_plain_operators_are_elementwise() {
        // `+`/`-`/`*`/`/` between two same-length Vectors now behave like their explicit
        // `.+`/`.-`/`.*`/`./` counterparts -- matching R/NumPy/Julia's convention that `*`
        // between vectors is elementwise, not a dot product (`dot()`, Punto 2, is the
        // dedicated way to ask for that).
        let code = r#"
            let a = [1.0, 2.0, 3.0];
            let b = [10.0, 20.0, 30.0];
            let sum = a + b;
            let diff = b - a;
            let prod = a * b;
            let quot = b / a;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(vector_f64(&interp.env.get("sum").unwrap()), vec![11.0, 22.0, 33.0]);
        assert_eq!(vector_f64(&interp.env.get("diff").unwrap()), vec![9.0, 18.0, 27.0]);
        assert_eq!(vector_f64(&interp.env.get("prod").unwrap()), vec![10.0, 40.0, 90.0]);
        assert_eq!(vector_f64(&interp.env.get("quot").unwrap()), vec![10.0, 10.0, 10.0]);
    }

    #[test]
    fn test_vector_vector_plain_operators_reject_mismatched_lengths() {
        let code = r#"
            let a = [1.0, 2.0, 3.0];
            let b = [10.0, 20.0];
            let sum = a + b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("mismatched lengths must fail");
        assert_eq!(err.code, "S0412");
    }

    #[test]
    fn test_vector_elementwise_parallel_path_matches_sequential_reference() {
        // TODO.md Fase 4, punto (a): 60,000 elements is above PARALLEL_THRESHOLD (50,000,
        // picked from examples/spike_vector_elementwise_parallel_latency.rs's measured
        // crossover), so this exercises the rayon `par_iter()` branch of
        // `vector_elementwise_op`, not just its zero-boxing sequential fast path.
        let code = r#"
            let a = random_uniform(60000);
            let b = random_uniform(60000);
            let sum = a + b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let a = vector_f64(&interp.env.get("a").unwrap());
        let b = vector_f64(&interp.env.get("b").unwrap());
        let sum = vector_f64(&interp.env.get("sum").unwrap());
        assert_eq!(sum.len(), 60_000);
        for i in 0..sum.len() {
            assert!(
                (sum[i] - (a[i] + b[i])).abs() < 1e-9,
                "mismatch at index {i}: sum={} expected={}",
                sum[i],
                a[i] + b[i]
            );
        }
    }

    #[test]
    fn test_vector_elementwise_below_threshold_still_correct() {
        // Below PARALLEL_THRESHOLD: exercises the sequential, zero-boxing numeric fast
        // path (still `as_f64_view()`-based, just single-threaded).
        let code = r#"
            let a = random_uniform(100);
            let b = random_uniform(100);
            let sum = a + b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let a = vector_f64(&interp.env.get("a").unwrap());
        let b = vector_f64(&interp.env.get("b").unwrap());
        let sum = vector_f64(&interp.env.get("sum").unwrap());
        assert_eq!(sum.len(), 100);
        for i in 0..sum.len() {
            assert!((sum[i] - (a[i] + b[i])).abs() < 1e-9);
        }
    }

    #[test]
    fn test_vector_elementwise_na_fallback_unaffected() {
        // A NA anywhere on either side must still fall back to the original per-element
        // boxed loop (not the new numeric fast path) and preserve the Kleene "NA wins at
        // that position only" behavior `test_vector_vector_plain_operators_are_elementwise`
        // already established -- this is the case the fast path deliberately excludes.
        let code = r#"
            let a = [1.0, sqrt(-1.0), 3.0];
            let b = [10.0, 20.0, 30.0];
            let sum = a + b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("sum").unwrap() {
            Value::Vector(vd) => {
                assert_eq!(vd.value_at(0), Some(Value::F64(11.0)));
                assert!(matches!(vd.value_at(1), Some(Value::NA(_))), "expected NA at index 1, got {:?}", vd.value_at(1));
                assert_eq!(vd.value_at(2), Some(Value::F64(33.0)));
            }
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_map_numeric_fn_fast_path_matches_boxed_reference() {
        // Migration of map_numeric_fn's dispatcher (covers log/exp/sqrt/abs/.../pow/
        // clamp/sin/cos) to as_f64_view() -- a large, NA-free vector exercises the fast
        // path, compared against a hand-computed reference.
        let code = r#"
            let v = random_uniform(60000);
            let roots = sqrt(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let v = vector_f64(&interp.env.get("v").unwrap());
        let roots = vector_f64(&interp.env.get("roots").unwrap());
        assert_eq!(roots.len(), 60_000);
        for i in 0..roots.len() {
            assert!((roots[i] - v[i].sqrt()).abs() < 1e-9);
        }
    }

    #[test]
    fn test_pow_fast_path_produces_na_on_nan_without_input_na() {
        // Exercises VectorData::from_f64_opt: pow(-8.0, 0.5) is NaN even though -8.0
        // itself isn't NA -- the fast path must still emit a per-position NA for it.
        let code = r#"
            let v = [4.0, -8.0, 9.0];
            let p = pow(v, 0.5);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("p").unwrap() {
            Value::Vector(vd) => {
                assert_eq!(vd.value_at(0), Some(Value::F64(2.0)));
                assert!(matches!(vd.value_at(1), Some(Value::NA(_))), "expected NA at index 1, got {:?}", vd.value_at(1));
                assert!((vd.value_at(2).unwrap().as_f64().unwrap() - 3.0).abs() < 1e-9);
            }
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_unary_neg_preserves_i64_dtype() {
        // UnaryNeg's fast path only applies to already-Float64 vectors -- an Int64
        // vector must fall through to the existing boxed loop and keep being Int64
        // (reconstructing from as_f64_view() would have silently turned it into F64).
        let code = r#"
            let v = [1, 2, 3];
            let neg = -v;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("neg").unwrap() {
            Value::Vector(vd) => {
                assert_eq!(vd.value_at(0), Some(Value::I64(-1)));
                assert_eq!(vd.value_at(1), Some(Value::I64(-2)));
                assert_eq!(vd.value_at(2), Some(Value::I64(-3)));
            }
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_between_fast_path() {
        let code = r#"
            let v = [1.0, 5.0, 10.0];
            let in_range = between(v, 2.0, 8.0);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("in_range").unwrap() {
            Value::Vector(vd) => {
                assert_eq!(vd.value_at(0), Some(Value::Bool(false)));
                assert_eq!(vd.value_at(1), Some(Value::Bool(true)));
                assert_eq!(vd.value_at(2), Some(Value::Bool(false)));
            }
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_cumsum_fast_path_matches_boxed_reference() {
        let code = r#"
            let v = [1.0, 2.0, 3.0, 4.0];
            let running = cumsum(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");
        assert_eq!(vector_f64(&interp.env.get("running").unwrap()), vec![1.0, 3.0, 6.0, 10.0]);
    }

    #[test]
    fn test_cumsum_na_poisons_rest_unaffected() {
        // The NA-present path is untouched by this migration -- still falls back to
        // cumulative()'s existing "one NA poisons everything after it" behavior.
        let code = r#"
            let v = [1.0, sqrt(-1.0), 3.0];
            let running = cumsum(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("running").unwrap() {
            Value::Vector(vd) => {
                assert_eq!(vd.value_at(0), Some(Value::F64(1.0)));
                assert!(matches!(vd.value_at(1), Some(Value::NA(_))));
                assert!(matches!(vd.value_at(2), Some(Value::NA(_))), "NA must poison everything after it");
            }
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_sort_asc_preserves_i64_dtype() {
        // sort_vector's fast path reorders the *original* Column via `.take()` rather
        // than reconstructing from as_f64_view() -- Int64 in must mean Int64 out.
        let code = r#"
            let v = [3, 1, 2];
            let sorted = sort_asc(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("sorted").unwrap() {
            Value::Vector(vd) => {
                assert_eq!(vd.value_at(0), Some(Value::I64(1)));
                assert_eq!(vd.value_at(1), Some(Value::I64(2)));
                assert_eq!(vd.value_at(2), Some(Value::I64(3)));
            }
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_sort_asc_fast_path_matches_boxed_reference() {
        let code = r#"
            let v = random_uniform(60000);
            let sorted = sort_asc(v);
            let rsorted = sort_desc(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let mut expected = vector_f64(&interp.env.get("v").unwrap());
        expected.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(vector_f64(&interp.env.get("sorted").unwrap()), expected);

        expected.reverse();
        assert_eq!(vector_f64(&interp.env.get("rsorted").unwrap()), expected);
    }

    #[test]
    fn test_rank_fast_path_matches_boxed_reference() {
        let code = r#"
            let v = [10.0, 30.0, 20.0, 30.0];
            let ranks = rank(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");
        // 10 -> rank 1; 20 -> rank 2; the two 30s tie for ranks 3 and 4, averaged to 3.5.
        assert_eq!(vector_f64(&interp.env.get("ranks").unwrap()), vec![1.0, 3.5, 2.0, 3.5]);
    }

    #[test]
    fn test_str_upper_fast_read_matches_reference() {
        let code = r#"
            let v = ["hello", "World"];
            let upper = str_upper(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        match interp.env.get("upper").unwrap() {
            Value::Vector(vd) => {
                assert_eq!(vd.value_at(0), Some(Value::String("HELLO".to_string())));
                assert_eq!(vd.value_at(1), Some(Value::String("WORLD".to_string())));
            }
            other => panic!("Expected Vector, found {other:?}"),
        }
    }

    #[test]
    fn test_n_distinct_counts_different_na_reasons_as_one_missing_value() {
        // Regression for the correctness bug this migration fixed: n_distinct used to
        // Debug-format every Value (including the NA's reason string) into a
        // HashSet<String>, so two NAs with different reasons counted as two distinct
        // values. Column::n_unique() counts null as at most one distinct value.
        let code = r#"
            let empty = random_uniform(0);
            let m = mean(empty);
            let v = [1.0, sqrt(-1.0), m];
            let n = n_distinct(v);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");
        // {1.0, NA} -- two distinct NA reasons ("NaN" and "EmptyVector") must still
        // collapse to a single missing-value bucket, giving 2 total, not 3.
        assert_eq!(interp.env.get("n").unwrap(), Value::I64(2));
    }

    #[test]
    fn test_random_uniform_produces_vector_of_requested_length_in_unit_interval() {
        // TODO.md Fase 3, Track 2, Punto 4: not reproducible across runs yet (that's
        // Fase 5's PRNG::seed job) -- what's checked here is shape and range only.
        let code = r#"
            let v = random_uniform(1000);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let v = vector_f64(&interp.env.get("v").unwrap());
        assert_eq!(v.len(), 1000);
        assert!(v.iter().all(|&x| (0.0..1.0).contains(&x)), "all values must be in [0, 1): {v:?}");
        // Not all-identical -- a real generator, not a stub returning a constant.
        assert!(v.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn test_random_uniform_rejects_negative_length() {
        let code = r#"
            let v = random_uniform(-5);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("negative length must fail");
        assert_eq!(err.code, "C0201");
    }

    #[test]
    fn test_random_uniform_seeded_is_reproducible() {
        // TODO.md Fase 5, Punto 1: same seed + n -> byte-identical Vector, any run.
        let code = r#"
            let a = random_uniform(1000, 42);
            let b = random_uniform(1000, 42);
            let c = random_uniform(1000, 7);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let a = vector_f64(&interp.env.get("a").unwrap());
        let b = vector_f64(&interp.env.get("b").unwrap());
        let c = vector_f64(&interp.env.get("c").unwrap());
        assert_eq!(a, b, "same seed must produce byte-identical output");
        assert_ne!(a, c, "different seeds must produce different output");
    }

    #[test]
    fn test_random_uniform_without_seed_still_unseeded() {
        // Confirms the optional third argument didn't change the existing, already-
        // tested default (thread-local, non-reproducible) behavior.
        let code = r#"
            let a = random_uniform(1000);
            let b = random_uniform(1000);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let a = vector_f64(&interp.env.get("a").unwrap());
        let b = vector_f64(&interp.env.get("b").unwrap());
        assert_ne!(a, b, "unseeded calls must not be reproducible");
    }

    #[test]
    fn test_bootstrap_mean_seeded_is_reproducible() {
        let code = r#"
            let v = random_uniform(1000, 1);
            let a = bootstrap_mean(v, 500, 99);
            let b = bootstrap_mean(v, 500, 99);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(
            vector_f64(&interp.env.get("a").unwrap()),
            vector_f64(&interp.env.get("b").unwrap()),
            "same seed must produce byte-identical bootstrap replicas"
        );
    }

    #[test]
    fn test_bootstrap_mean_seeded_independent_of_thread_count() {
        // The real test of "reproducible bit-a-bit": which replica gets which stream
        // must be fixed by its index (via Xoshiro256PlusPlus::jump()), not by which
        // thread happens to run it -- so 1 thread and rayon's default pool must agree.
        let code = r#"
            let v = random_uniform(1000, 1);
            let means = bootstrap_mean(v, 500, 99);
        "#;
        let program = parse(code).expect("syntax ok");

        let mut interp_default = Interpreter::new();
        interp_default.eval_program(&program).expect("evaluation ok");
        let default_pool_result = vector_f64(&interp_default.env.get("means").unwrap());

        let single_thread_pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        let single_thread_result = single_thread_pool.install(|| {
            let mut interp_single = Interpreter::new();
            interp_single.eval_program(&program).expect("evaluation ok");
            vector_f64(&interp_single.env.get("means").unwrap())
        });

        assert_eq!(
            default_pool_result, single_thread_result,
            "seeded bootstrap_mean must not depend on thread count/scheduling"
        );
    }

    #[test]
    fn test_bootstrap_mean_produces_requested_number_of_replicas() {
        // TODO.md Fase 4, punto (c), Suite 03's Caso 3.3. Base sample [1..5], real mean 3.0.
        // Each replica is itself a mean of values resampled *from* the base, so it must
        // land within the base's own [min, max] range, and averaging many replicas should
        // land close to the true mean (generous tolerance -- this is a statistical
        // assertion, not an exact one, to avoid flakiness).
        let code = r#"
            let v = [1.0, 2.0, 3.0, 4.0, 5.0];
            let reps = bootstrap_mean(v, 500);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let reps = vector_f64(&interp.env.get("reps").unwrap());
        assert_eq!(reps.len(), 500);
        for r in &reps {
            assert!((1.0..=5.0).contains(r), "replica mean {r} outside base sample range");
        }
        let grand_mean: f64 = reps.iter().sum::<f64>() / reps.len() as f64;
        assert!((grand_mean - 3.0).abs() < 0.5, "grand mean {grand_mean} too far from true mean 3.0");
    }

    #[test]
    fn test_bootstrap_mean_propagates_na() {
        let code = r#"
            let v = [1.0, sqrt(-1.0), 3.0];
            let reps = bootstrap_mean(v, 10);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert!(matches!(interp.env.get("reps").unwrap(), Value::NA(_)));
    }

    #[test]
    fn test_bootstrap_mean_rejects_non_vector_first_argument() {
        let code = r#"
            let reps = bootstrap_mean(5.0, 10);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("non-Vector first argument must fail");
        assert_eq!(err.code, "C0202");
    }

    #[test]
    fn test_bootstrap_mean_rejects_empty_vector() {
        let code = r#"
            let empty = random_uniform(0);
            let reps = bootstrap_mean(empty, 10);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let err = interp.eval_program(&program).expect_err("empty base sample must fail");
        assert_eq!(err.code, "S0412");
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
        let ids = df_column(&filtered_val, "id");
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0], Value::I64(2));
        assert_eq!(ids[1], Value::I64(3));
        assert_eq!(ids[2], Value::I64(4));
    }

    #[test]
    fn test_filter_is_na_predicate_and_negation() {
        let code = r#"
            let df = dataframe {
                id: [1, 2, 3, 4],
                category: ["A", NA, "B", NA:Dropout]
            };
            let only_na = df |> filter(is_na(category));
            let without_na = df |> filter(!is_na(category));
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let only_na = interp.env.get("only_na").expect("only_na exists");
        assert_eq!(df_column(&only_na, "id"), vec![Value::I64(2), Value::I64(4)]);

        let without_na = interp.env.get("without_na").expect("without_na exists");
        assert_eq!(df_column(&without_na, "id"), vec![Value::I64(1), Value::I64(3)]);
    }

    #[test]
    fn test_filter_compound_predicate_and_or() {
        let code = r#"
            let df = dataframe {
                id:       [1, 2, 3, 4, 5],
                score:    [80.0, 60.0, 90.0, 40.0, 95.0],
                category: ["A", "B", NA, "A", "B"]
            };
            // Caso 2.3 de benchmarks/suites/02: score > 75.0 && !is_na(category).
            let clean = df |> filter(score > 75.0 && !is_na(category));
            let either = df |> filter(score > 90.0 || category == "B");
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        // Rows with score > 75.0: 1 (80), 3 (90), 5 (95). Row 3 has NA category, so it
        // must be excluded by !is_na(category) -- only rows 1 and 5 survive.
        let clean = interp.env.get("clean").expect("clean exists");
        assert_eq!(df_column(&clean, "id"), vec![Value::I64(1), Value::I64(5)]);

        // score > 90.0: row 5 (95). category == "B": rows 2, 5. Union: rows 2, 5.
        let either = interp.env.get("either").expect("either exists");
        assert_eq!(df_column(&either, "id"), vec![Value::I64(2), Value::I64(5)]);
    }

    #[test]
    fn test_filter_rejects_unrecognized_predicate_instead_of_silently_passing_through() {
        // A `filter()` that doesn't understand its second argument must error, not
        // silently hand back the DataFrame unfiltered -- GHL doesn't do silent state.
        let code = r#"
            let df = dataframe { id: [1, 2, 3] };
            let filtered = df |> filter(42);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let result = interp.eval_program(&program);
        assert!(result.is_err(), "filter() with a nonsensical predicate must error, not no-op");
    }

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
        assert_eq!(df_columns(&tidy_val), vec!["term", "estimate", "std_error", "statistic", "p_value"]);
        let terms = df_column(&tidy_val, "term");
        assert_eq!(terms.len(), 3);
        assert_eq!(terms[0], Value::String("(Intercept)".into()));
        assert_eq!(terms[1], Value::String("x1".into()));
        assert_eq!(terms[2], Value::String("x2".into()));

        // Verify glance table
        let glance_val = interp.env.get("glanced").expect("glance exists");
        assert!(df_columns(&glance_val).contains(&"r_squared".to_string()));
        let r2 = df_column(&glance_val, "r_squared")[0].as_f64().unwrap();
        assert!((r2 - 1.0).abs() < 1e-6, "R2 must be 1.0 for perfect linear fit");

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
    fn test_grammar_of_graphics_pipeline() {
        let code = r#"
            let df = dataframe {
                dose: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                response: [10.5, 20.0, 31.2, 39.8, 51.0, 60.5]
            };

            let p = df |> plot(aes(col("dose"), col("response")))
                       |> geom_point()
                       |> geom_smooth()
                       |> labs("Dose-Response Fit");

            let h = [1.0, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0] |> hist();
            let b = [10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 100.0] |> boxplot();
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let p_val = interp.env.get("p").expect("p exists");
        if let Value::Plot(spec) = p_val {
            assert_eq!(spec.layers.len(), 2);
            assert_eq!(spec.x_data.len(), 6);
            assert_eq!(spec.y_data.len(), 6);
            assert_eq!(spec.labels.title.as_deref(), Some("Dose-Response Fit"));
        } else {
            panic!("Expected Plot for p");
        }

        let h_val = interp.env.get("h").expect("h exists");
        assert!(matches!(h_val, Value::Plot(_)));

        let b_val = interp.env.get("b").expect("b exists");
        assert!(matches!(b_val, Value::Plot(_)));
    }

    #[test]
    fn test_layered_io_and_wrangling_pipeline() {
        let temp_dir = std::env::temp_dir();
        let in_path = temp_dir.join("ghl_test_input.csv");
        let out_path = temp_dir.join("ghl_test_output.csv");
        let in_str = in_path.to_str().unwrap().replace('\\', "/");
        let out_str = out_path.to_str().unwrap().replace('\\', "/");

        let code = format!(
            r#"
            let raw_csv = "id,dose,response,batch\n1,1.0,10.0,A\n2,2.0,20.5,A\n3,3.0,30.2,B\n4,4.0,39.9,B\n5,5.0,50.1,C\n";
            raw_csv |> write_file("{in_str}");

            let df = read_csv("{in_str}");
            let subset = df |> select(["dose", "response"]) |> head(4);
            let model = fit(response ~ dose, subset);
            let tidy_df = tidy(model);

            tidy_df |> write_csv("{out_str}");
            let exists = file_exists("{out_str}");
            let lines = read_lines("{out_str}");
            "#
        );

        let program = parse(&code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let exists_val = interp.env.get("exists").expect("exists val");
        assert_eq!(exists_val, Value::Bool(true));

        let lines_val = interp.env.get("lines").expect("lines val");
        if let Value::Vector(lines) = lines_val {
            assert!(lines.len() >= 3); // Header + Intercept + dose
            assert!(lines[0].to_string().contains("term"));
        } else {
            panic!("Expected Vector for lines");
        }

        let _ = std::fs::remove_file(in_path);
        let _ = std::fs::remove_file(out_path);
    }

    #[test]
    fn test_dataframe_tidyverse_pipeline() {
        // Full Tidyverse-style DataFrame pipeline:
        //   mutate -> arrange -> rename -> drop -> distinct -> nrow/ncol/colnames -> slice
        let code = r#"
            let df = dataframe {
                id:     [1, 2, 3, 4, 5, 2, 3],
                dose:   [1.0, 2.0, 3.0, 4.0, 5.0, 2.0, 3.0],
                group:  ["A", "B", "A", "B", "A", "B", "A"]
            };

            // mutate: add a derived column (dose squared)
            let dose_sq = [1.0, 4.0, 9.0, 16.0, 25.0, 4.0, 9.0];
            let df2 = df |> mutate("dose2", dose_sq);

            // arrange: sort by dose descending
            let df_sorted = df |> arrange(desc(dose));

            // rename: rename group -> cohort
            let df_renamed = df |> rename("group", "cohort");

            // drop: remove id column
            let df_dropped = df |> drop(["id"]);

            // distinct: remove duplicate rows (id 2 and 3 appear twice)
            let df_unique = df |> distinct();

            // nrow / ncol / colnames
            let n_rows     = df |> nrow();
            let n_cols     = df |> ncol();
            let col_names  = df |> colnames();

            // slice: rows 1..3 (0-based, exclusive end)
            let sliced = df |> slice(1, 4);
        "#;

        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        // mutate added a 4th column
        let df2 = interp.env.get("df2").expect("df2");
        assert_eq!(df_columns(&df2).len(), 4);
        assert!(df_columns(&df2).contains(&"dose2".to_string()));
        let d2 = df_column(&df2, "dose2");
        assert_eq!(d2[0], Value::F64(1.0));
        assert_eq!(d2[1], Value::F64(4.0));

        // arrange sorted descending: first dose should be 5.0
        let df_s = interp.env.get("df_sorted").expect("df_sorted");
        let doses = df_column(&df_s, "dose");
        assert_eq!(doses[0], Value::F64(5.0));
        assert_eq!(doses[1], Value::F64(4.0));

        // rename: cohort present, group absent
        let df_r = interp.env.get("df_renamed").expect("df_renamed");
        let renamed_cols = df_columns(&df_r);
        assert!(renamed_cols.contains(&"cohort".to_string()));
        assert!(!renamed_cols.contains(&"group".to_string()));

        // drop: id column removed
        let df_d = interp.env.get("df_dropped").expect("df_dropped");
        let dropped_cols = df_columns(&df_d);
        assert!(!dropped_cols.contains(&"id".to_string()));
        assert!(dropped_cols.contains(&"dose".to_string()));

        // distinct: 7 rows -> 5 unique (rows with id=2 and id=3 have exact duplicates)
        let df_u = interp.env.get("df_unique").expect("df_unique");
        assert_eq!(df_height(&df_u), 5, "Expected 5 distinct rows");

        // nrow = 7, ncol = 3
        assert_eq!(interp.env.get("n_rows"), Some(Value::I64(7)));
        assert_eq!(interp.env.get("n_cols"), Some(Value::I64(3)));

        // colnames = ["id", "dose", "group"]
        let col_names_val = interp.env.get("col_names").expect("col_names");
        if let Value::Vector(names) = col_names_val {
            assert_eq!(names.len(), 3);
            assert_eq!(names[0], Value::String("id".into()));
        } else { panic!("Expected col_names to be Vector"); }

        // slice(1, 4) = rows 1,2,3 — second id should be 2
        let sliced_val = interp.env.get("sliced").expect("sliced");
        let ids = df_column(&sliced_val, "id");
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0], Value::I64(2));
        assert_eq!(ids[2], Value::I64(4));
    }

    #[test]
    fn test_group_by_summarize_unquoted_columns() {
        // Bare column names (no quotes, no `col()`) inside `summarize()`'s named args.
        let code = r#"
            let df = dataframe {
                species: ["a", "b", "a", "b", "a"],
                x:       [1.0, 2.0, 3.0, 4.0, 5.0]
            };

            let summary = df
                |> group_by(species)
                |> summarize(n = count(), mean_x = mean(x), max_x = max(x));
        "#;

        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let summary = interp.env.get("summary").expect("summary");
        assert_eq!(
            df_columns(&summary),
            vec!["species".to_string(), "n".to_string(), "mean_x".to_string(), "max_x".to_string()]
        );
        let species = df_column(&summary, "species");
        let n = df_column(&summary, "n");
        let mean_x = df_column(&summary, "mean_x");
        assert_eq!(species.len(), 2);

        let a_idx = species.iter().position(|v| v == &Value::String("a".into())).unwrap();
        assert_eq!(n[a_idx], Value::I64(3));
        assert_eq!(mean_x[a_idx], Value::F64(3.0)); // (1+3+5)/3

        let b_idx = species.iter().position(|v| v == &Value::String("b".into())).unwrap();
        assert_eq!(n[b_idx], Value::I64(2));
        assert_eq!(mean_x[b_idx], Value::F64(3.0)); // (2+4)/2
    }

    #[test]
    fn test_summarize_propagates_na_kleene_style() {
        // RFC 02 sect2.4: GHL never silently skips missing data in aggregates the way
        // pandas/numpy do. summarize()'s native polars reduce (mean_reduce/max_reduce/
        // etc.) skips nulls by default -- this locks in the explicit null_count() guard
        // in compute_agg (io.rs) that makes any NA in a group taint the whole result,
        // same as the standalone mean(vec)/max(vec) functions already do.
        let code = r#"
            let df = dataframe {
                g: ["a", "b", "a", "b", "a"],
                x: [1.0, 2.0, NA:SensorDropout, 4.0, 5.0]
            };
            let summary = df |> group_by(g) |> summarize(
                mean_x = mean(x), max_x = max(x), min_x = min(x),
                sum_x = sum(x), n_distinct_x = n_distinct(x)
            );
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let summary = interp.env.get("summary").expect("summary");
        let groups = df_column(&summary, "g");
        let mean_x = df_column(&summary, "mean_x");
        let max_x = df_column(&summary, "max_x");
        let sum_x = df_column(&summary, "sum_x");

        // Group "a" (values [1.0, NA:SensorDropout, 5.0]) must come back as plain NA for
        // every value-touching aggregate -- not skip the NA and average [1.0, 5.0] to 3.0.
        let a_idx = groups.iter().position(|v| v == &Value::String("a".into())).unwrap();
        assert_eq!(mean_x[a_idx], Value::NA(None));
        assert_eq!(max_x[a_idx], Value::NA(None));
        assert_eq!(sum_x[a_idx], Value::NA(None));

        // Group "b" (values [2.0, 4.0], no NA) must still compute normally.
        let b_idx = groups.iter().position(|v| v == &Value::String("b".into())).unwrap();
        assert_eq!(mean_x[b_idx], Value::F64(3.0));
        assert_eq!(max_x[b_idx], Value::F64(4.0));
        assert_eq!(sum_x[b_idx], Value::F64(6.0));

        // n_distinct() isn't a Kleene-propagating aggregate (matches the pre-optimization
        // behavior): the NA itself counts as one of the distinct values in the group.
        let n_distinct_x = df_column(&summary, "n_distinct_x");
        assert_eq!(n_distinct_x[a_idx], Value::I64(3));
        assert_eq!(n_distinct_x[b_idx], Value::I64(2));
    }

    #[test]
    fn test_arrange_multi_column_with_desc() {
        let code = r#"
            let df = dataframe {
                group: ["b", "a", "a", "b"],
                x:     [2.0, 1.0, 2.0, 1.0]
            };

            // Ascending by group, then descending by x within each group.
            let sorted = df |> arrange(group, desc(x));
        "#;

        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let sorted = interp.env.get("sorted").expect("sorted");
        let groups = df_column(&sorted, "group");
        let xs = df_column(&sorted, "x");
        assert_eq!(groups, vec![
            Value::String("a".into()), Value::String("a".into()),
            Value::String("b".into()), Value::String("b".into()),
        ]);
        assert_eq!(xs, vec![Value::F64(2.0), Value::F64(1.0), Value::F64(2.0), Value::F64(1.0)]);
    }

    #[test]
    fn test_slice_min_max_and_sample_n() {
        let code = r#"
            let df = dataframe {
                id: [1, 2, 3, 4, 5],
                x:  [30.0, 10.0, 50.0, 20.0, 40.0]
            };

            let smallest = df |> slice_min(x, 2);
            let largest  = df |> slice_max(x, 2);
            let sampled  = df |> sample_n(3);
        "#;

        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        if let Some(smallest) = interp.env.get("smallest") {
            assert_eq!(df_column(&smallest, "x"), vec![Value::F64(10.0), Value::F64(20.0)]);
        } else {
            panic!("Expected smallest to be DataFrame");
        }

        if let Some(largest) = interp.env.get("largest") {
            assert_eq!(df_column(&largest, "x"), vec![Value::F64(50.0), Value::F64(40.0)]);
        } else {
            panic!("Expected largest to be DataFrame");
        }

        if let Some(sampled) = interp.env.get("sampled") {
            assert_eq!(df_column(&sampled, "x").len(), 3);
        } else {
            panic!("Expected sampled to be DataFrame");
        }
    }

    #[test]
    fn test_inner_join_and_left_join() {
        let code = r#"
            let orders = dataframe {
                order_id: [1, 2, 3, 4],
                customer_id: [10, 20, 10, 30]
            };
            let customers = dataframe {
                customer_id: [10, 20],
                name: ["Alice", "Bob"]
            };

            let inner = orders |> inner_join(customers, customer_id);
            let left = orders |> left_join(customers, customer_id);
        "#;

        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let inner = interp.env.get("inner").expect("inner exists");
        assert_eq!(df_height(&inner), 3, "unmatched customer_id=30 row should be dropped");
        assert!(df_columns(&inner).contains(&"name".to_string()));

        let left = interp.env.get("left").expect("left exists");
        assert_eq!(df_height(&left), 4, "every `orders` row should survive a left join");
        let names = df_column(&left, "name");
        let order_ids = df_column(&left, "order_id");
        let unmatched_idx = order_ids.iter().position(|v| v == &Value::I64(4)).unwrap();
        assert_eq!(names[unmatched_idx], Value::NA(None), "unmatched right side should be NA");
    }

    #[test]
    fn test_join_preserves_na_reasons_on_both_sides() {
        // TODO.md Fase 1: na_reasons used to be dropped entirely by inner_join/left_join
        // (the eager join API doesn't expose per-output-row provenance on its own) --
        // df_join now carries a row-index column through the join on each side to recover
        // it. `score` collides between `left` and `right` on purpose, to also cover the
        // rename-on-collision path (`score` -> `score_right`).
        let code = r#"
            let left = dataframe {
                id:    [1, 2, 3],
                score: [10.0, NA:SensorDropout, 30.0]
            };
            let right = dataframe {
                id:    [1, 2],
                score: [NA:Timeout, 200.0]
            };

            let inner = left |> inner_join(right, id);
            let inner_left_reasons = inner |> na_reasons(score);
            let inner_right_reasons = inner |> na_reasons(score_right);

            let outer = left |> left_join(right, id);
            let outer_left_reasons = outer |> na_reasons(score);
            let outer_right_reasons = outer |> na_reasons(score_right);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let na = Value::NA(None);
        let vec_of = |interp: &Interpreter, name: &str| match interp.env.get(name).expect(name) {
            Value::Vector(vals) => vals.iter().cloned().collect::<Vec<Value>>(),
            other => panic!("expected Vector for `{name}`, got {other:?}"),
        };

        // Inner join: id=3 (left-only) is dropped, id=1/2 both matched.
        assert_eq!(vec_of(&interp, "inner_left_reasons"), vec![na.clone(), Value::String("SensorDropout".into())]);
        assert_eq!(vec_of(&interp, "inner_right_reasons"), vec![Value::String("Timeout".into()), na.clone()]);

        // Left join: every left row survives, including the unmatched id=3 -- its
        // `score_right` is a real (reason-less) NA, not a fabricated reason from a right
        // row that never existed.
        assert_eq!(vec_of(&interp, "outer_left_reasons"), vec![na.clone(), Value::String("SensorDropout".into()), na.clone()]);
        assert_eq!(vec_of(&interp, "outer_right_reasons"), vec![Value::String("Timeout".into()), na.clone(), na.clone()]);
    }

    #[test]
    fn test_parquet_round_trip() {
        let path = std::env::temp_dir().join("ghl_test_round_trip.parquet");
        let path_str = path.to_str().unwrap();

        let code = format!(
            r#"
            let df = dataframe {{
                id: [1, 2, 3],
                score: [10.5, 20.5, NA],
                label: ["a", "b", "c"]
            }};
            df |> write_parquet("{path}");
            let roundtripped = read_parquet("{path}");
            "#,
            path = path_str
        );

        let program = parse(&code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");
        let _ = std::fs::remove_file(&path);

        let roundtripped = interp.env.get("roundtripped").expect("roundtripped exists");
        assert_eq!(df_height(&roundtripped), 3);
        assert_eq!(df_column(&roundtripped, "id"), vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
        assert_eq!(
            df_column(&roundtripped, "score"),
            vec![Value::F64(10.5), Value::F64(20.5), Value::NA(None)],
            "plain NA (validity bit) must survive Parquet even though a *reason* can't"
        );
        assert_eq!(
            df_column(&roundtripped, "label"),
            vec![Value::String("a".into()), Value::String("b".into()), Value::String("c".into())]
        );
    }

    #[test]
    fn test_na_reason_accessors_survive_filter() {
        // RFC 02 sect2.5: reasons exist so missing-data-analysis code can consume them
        // via ordinary GHL values, not just internally.
        let code = r#"
            let df = dataframe {
                keep:  [1, 1, 0, 1],
                score: [10.0, NA:SensorDropout, 30.0, NA:LowBattery]
            };

            let plain_reason = na_reason(NA);
            let noted_reason = na_reason(NA:NoResponse);

            let reasons_before = df |> na_reasons(score);
            let filtered = df |> filter(keep > 0);
            let reasons_after = filtered |> na_reasons(score);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(interp.env.get("plain_reason"), Some(Value::NA(None)));
        assert_eq!(interp.env.get("noted_reason"), Some(Value::String("NoResponse".into())));

        let before = interp.env.get("reasons_before").expect("reasons_before");
        if let Value::Vector(vals) = before {
            assert_eq!(*vals, vec![Value::NA(None), Value::String("SensorDropout".into()), Value::NA(None), Value::String("LowBattery".into())]);
        } else {
            panic!("Expected Vector for na_reasons()");
        }

        // Row index 2 (score=30.0, no reason) is the one dropped by `filter(keep > 0)`;
        // the LowBattery reason at the old row 3 must reindex to the new row 2.
        let after = interp.env.get("reasons_after").expect("reasons_after");
        if let Value::Vector(vals) = after {
            assert_eq!(*vals, vec![Value::NA(None), Value::String("SensorDropout".into()), Value::String("LowBattery".into())]);
        } else {
            panic!("Expected Vector for na_reasons()");
        }
    }

    #[test]
    fn test_is_na() {
        let code = r#"
            let scalar_true = is_na(NA);
            let scalar_false = is_na(5.0);
            let vectorized = is_na([1.0, NA, NA:SensorDropout, 4.0]);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(interp.env.get("scalar_true"), Some(Value::Bool(true)));
        assert_eq!(interp.env.get("scalar_false"), Some(Value::Bool(false)));
        assert_eq!(
            interp.env.get("vectorized"),
            Some(Value::Vector(crate::vector_data::VectorData::from_values(vec![Value::Bool(false), Value::Bool(true), Value::Bool(true), Value::Bool(false)])))
        );
    }

    #[test]
    fn test_math_and_string_helpers() {
        let code = r#"
            let rounded = round(3.14159, 2);
            let clamped = clamp(15.0, 0.0, 10.0);
            let root = sqrt(-1.0);
            let shouted = str_upper("hello");
            let padded = str_pad("42", 5, "0");
            let contains = str_contains("hello world", "world");
        "#;

        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(interp.env.get("rounded"), Some(Value::F64(3.14)));
        assert_eq!(interp.env.get("clamped"), Some(Value::F64(10.0)));
        assert!(matches!(interp.env.get("root"), Some(Value::NA(_))));
        assert_eq!(interp.env.get("shouted"), Some(Value::String("HELLO".into())));
        assert_eq!(interp.env.get("padded"), Some(Value::String("00042".into())));
        assert_eq!(interp.env.get("contains"), Some(Value::Bool(true)));
    }
}

