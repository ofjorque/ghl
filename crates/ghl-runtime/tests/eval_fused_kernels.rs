//! Integration tests for fused tensor operations and matrix axis reductions (Pillar 5 / Roadmap Part H).

use ghl_runtime::value::Value;
use ghl_runtime::eval::Interpreter;
use ghl_syntax::parser::parse;

#[test]
fn test_eval_fused_sigmoid_matmul() {
    let code = r#"
        let A = mat [ 1.0, 0.0 ; 0.0, 1.0 ];
        let B = mat [ 0.0, 0.0 ; 0.0, 0.0 ];
        let P = sigmoid_matmul(A, B);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    if let Some(Value::Matrix { rows, cols, data }) = interp.env.get("P") {
        assert_eq!(rows, 2);
        assert_eq!(cols, 2);
        for v in data.as_slice() {
            assert!((v - 0.5).abs() < 1e-6);
        }
    } else {
        panic!("Expected Matrix for P");
    }
}

#[test]
fn test_eval_fused_log_sum_exp_and_softmax() {
    let code = r#"
        let M = mat [ 0.0, 0.0 ; 1.0, 1.0 ];
        let lse_rows = log_sum_exp(M, 1);
        let prob_rows = softmax(M, 1);
        let vec_input = [1000.0, 1000.0];
        let vec_lse = log_sum_exp(vec_input);
        let vec_sm = softmax(vec_input);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    // lse_rows: [ln(2), 1.0 + ln(2)]
    if let Some(Value::Vector(vd)) = interp.env.get("lse_rows") {
        let view = vd.as_f64_view().unwrap();
        let s = view.as_slice();
        assert!((s[0] - 2.0f64.ln()).abs() < 1e-6);
        assert!((s[1] - (1.0 + 2.0f64.ln())).abs() < 1e-6);
    } else {
        panic!("Expected Vector for lse_rows");
    }

    // prob_rows: row sums should be 1.0
    if let Some(Value::Matrix { rows, cols, data }) = interp.env.get("prob_rows") {
        assert_eq!(rows, 2);
        assert_eq!(cols, 2);
        let s = data.as_slice();
        assert!((s[0] - 0.5).abs() < 1e-6);
        assert!((s[1] - 0.5).abs() < 1e-6);
        assert!((s[2] - 0.5).abs() < 1e-6);
        assert!((s[3] - 0.5).abs() < 1e-6);
    } else {
        panic!("Expected Matrix for prob_rows");
    }

    // Numerical stability with huge inputs
    if let Some(Value::F64(lse)) = interp.env.get("vec_lse") {
        assert!((lse - (1000.0 + 2.0f64.ln())).abs() < 1e-6);
    } else {
        panic!("Expected F64 for vec_lse");
    }

    if let Some(Value::Vector(vd)) = interp.env.get("vec_sm") {
        let view = vd.as_f64_view().unwrap();
        let s = view.as_slice();
        assert!((s[0] - 0.5).abs() < 1e-6);
        assert!((s[1] - 0.5).abs() < 1e-6);
    } else {
        panic!("Expected Vector for vec_sm");
    }
}

#[test]
fn test_eval_matrix_axis_reductions() {
    let code = r#"
        let M = mat [ 1.0, 2.0, 3.0 ; 4.0, 5.0, 6.0 ];
        let r_sum = row_sums(M);
        let c_sum = col_sums(M);
        let r_avg = row_means(M);
        let c_avg = col_means(M);
        let r_max = row_maxs(M);
        let c_max = col_maxs(M);
        let r_min = row_mins(M);
        let c_min = col_mins(M);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    if let Some(Value::Vector(vd)) = interp.env.get("r_sum") {
        assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[6.0, 15.0]);
    } else { panic!("r_sum"); }

    if let Some(Value::Vector(vd)) = interp.env.get("c_sum") {
        assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[5.0, 7.0, 9.0]);
    } else { panic!("c_sum"); }

    if let Some(Value::Vector(vd)) = interp.env.get("r_avg") {
        assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[2.0, 5.0]);
    } else { panic!("r_avg"); }

    if let Some(Value::Vector(vd)) = interp.env.get("c_avg") {
        assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[2.5, 3.5, 4.5]);
    } else { panic!("c_avg"); }

    if let Some(Value::Vector(vd)) = interp.env.get("r_max") {
        assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[3.0, 6.0]);
    } else { panic!("r_max"); }

    if let Some(Value::Vector(vd)) = interp.env.get("c_max") {
        assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[4.0, 5.0, 6.0]);
    } else { panic!("c_max"); }

    if let Some(Value::Vector(vd)) = interp.env.get("r_min") {
        assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[1.0, 4.0]);
    } else { panic!("r_min"); }

    if let Some(Value::Vector(vd)) = interp.env.get("c_min") {
        assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[1.0, 2.0, 3.0]);
    } else { panic!("c_min"); }
}

#[test]
fn test_eval_fused_mul_add() {
    let code = r#"
        let A = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
        let B = mat [ 2.0, 2.0 ; 2.0, 2.0 ];
        let C = mat [ 10.0, 20.0 ; 30.0, 40.0 ];
        let R = fused_mul_add(A, B, C);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    if let Some(Value::Matrix { rows, cols, data }) = interp.env.get("R") {
        assert_eq!(rows, 2);
        assert_eq!(cols, 2);
        assert_eq!(data.as_slice(), &[16.0, 26.0, 44.0, 54.0]);
    } else {
        panic!("Expected Matrix for R");
    }
}
