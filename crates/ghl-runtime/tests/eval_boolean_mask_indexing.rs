use ghl_syntax::parser::parse;
use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;

#[test]
fn test_vector_boolean_mask_filter() {
    let code = r#"
        let v = [10.0, 25.0, 5.0, 40.0];
        let filtered = v[v > 20.0];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("filtered").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 2);
            assert_eq!(vd.value_at(0), Some(Value::F64(25.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(40.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_matrix_row_filtering_with_boolean_mask() {
    let code = r#"
        let M = mat [
            1.0, 2.0 ;
            3.0, 4.0 ;
            5.0, 6.0
        ];
        let mask = [true, false, true];
        let sub = M[mask, ..];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("sub").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 2);
            assert_eq!(cols, 2);
            // Row 0 of M (1.0, 2.0)
            assert_eq!(data[0 * 2 + 0], 1.0);
            assert_eq!(data[0 * 2 + 1], 2.0);
            // Row 2 of M (5.0, 6.0)
            assert_eq!(data[1 * 2 + 0], 5.0);
            assert_eq!(data[1 * 2 + 1], 6.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_matrix_col_filtering_with_boolean_mask() {
    let code = r#"
        let M = mat [
            1.0, 2.0, 3.0 ;
            4.0, 5.0, 6.0
        ];
        let mask = [false, true, true];
        let sub = M[.., mask];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("sub").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 2);
            assert_eq!(cols, 2);
            // Col 1 and 2
            assert_eq!(data[0 * 2 + 0], 2.0);
            assert_eq!(data[0 * 2 + 1], 3.0);
            assert_eq!(data[1 * 2 + 0], 5.0);
            assert_eq!(data[1 * 2 + 1], 6.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_matrix_row_mask_with_scalar_col() {
    let code = r#"
        let M = mat [
            10.0, 20.0 ;
            30.0, 40.0 ;
            50.0, 60.0
        ];
        let mask = [true, false, true];
        let col1 = M[mask, 1];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("col1").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 2);
            assert_eq!(vd.value_at(0), Some(Value::F64(20.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(60.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_matrix_integer_index_vectors() {
    let code = r#"
        let M = mat [
            10.0, 20.0 ;
            30.0, 40.0 ;
            50.0, 60.0
        ];
        let sub = M[[2, 0], ..];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("sub").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 2);
            assert_eq!(cols, 2);
            // First selected row is index 2: [50.0, 60.0]
            assert_eq!(data[0 * 2 + 0], 50.0);
            assert_eq!(data[0 * 2 + 1], 60.0);
            // Second selected row is index 0: [10.0, 20.0]
            assert_eq!(data[1 * 2 + 0], 10.0);
            assert_eq!(data[1 * 2 + 1], 20.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_matrix_mask_length_mismatch_error() {
    let code = r#"
        let M = mat [
            1.0, 2.0 ;
            3.0, 4.0 ;
            5.0, 6.0
        ];
        let bad_mask = [true, false];
        let sub = M[bad_mask, ..];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let res = interp.eval_program(&program);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code, "C0202");
    assert!(err.message.contains("Boolean mask length (2) must match matrix row count (3)"));
}
