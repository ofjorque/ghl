//! Core eval semantics (arithmetic, Kleene NA), matrix/linalg (QR, Cholesky, eigen, SVD), dot product, map(), and vector-op fast paths.

mod common;
use common::*;

use ghl_runtime::value::Value;
use ghl_runtime::eval::Interpreter;
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

