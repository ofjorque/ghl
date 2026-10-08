use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;
use ghl_types::checker::TypeChecker;

#[test]
fn test_vector_scalar_in_place_assignment() {
    let code = r#"
        let mut v = [10.0, 20.0, 30.0];
        v[1] = 99.0;
        let res = v;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 3);
            assert_eq!(vd.value_at(0), Some(Value::F64(10.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(99.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(30.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_vector_slice_assignment() {
    let code = r#"
        let mut v = [1.0, 2.0, 3.0, 4.0, 5.0];
        v[1..4] = [20.0, 30.0, 40.0];
        let res = v;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 5);
            assert_eq!(vd.value_at(0), Some(Value::F64(1.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(20.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(30.0)));
            assert_eq!(vd.value_at(3), Some(Value::F64(40.0)));
            assert_eq!(vd.value_at(4), Some(Value::F64(5.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_vector_slice_scalar_broadcast() {
    let code = r#"
        let mut v = [1.0, 2.0, 3.0, 4.0, 5.0];
        v[1..4] = 0.0;
        let res = v;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 5);
            assert_eq!(vd.value_at(0), Some(Value::F64(1.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(0.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(0.0)));
            assert_eq!(vd.value_at(3), Some(Value::F64(0.0)));
            assert_eq!(vd.value_at(4), Some(Value::F64(5.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_vector_boolean_mask_clamping() {
    let code = r#"
        let mut v = [-2.0, 5.0, -1.0, 8.0];
        v[v < 0.0] = 0.0;
        let res = v;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 4);
            assert_eq!(vd.value_at(0), Some(Value::F64(0.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(5.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(0.0)));
            assert_eq!(vd.value_at(3), Some(Value::F64(8.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_matrix_cell_assignment() {
    let code = r#"
        let mut M = mat [
            1.0, 2.0 ;
            3.0, 4.0
        ];
        M[0, 1] = 42.0;
        let res = M;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 2);
            assert_eq!(cols, 2);
            assert_eq!(data[0 * 2 + 0], 1.0);
            assert_eq!(data[0 * 2 + 1], 42.0);
            assert_eq!(data[1 * 2 + 0], 3.0);
            assert_eq!(data[1 * 2 + 1], 4.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_matrix_row_assignment_vector_and_broadcast() {
    let code = r#"
        let mut M = mat [
            1.0, 2.0, 3.0 ;
            4.0, 5.0, 6.0
        ];
        M[0, ..] = [10.0, 20.0, 30.0];
        M[1, ..] = 99.0;
        let res = M;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 2);
            assert_eq!(cols, 3);
            assert_eq!(data[0 * 3 + 0], 10.0);
            assert_eq!(data[0 * 3 + 1], 20.0);
            assert_eq!(data[0 * 3 + 2], 30.0);
            assert_eq!(data[1 * 3 + 0], 99.0);
            assert_eq!(data[1 * 3 + 1], 99.0);
            assert_eq!(data[1 * 3 + 2], 99.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_matrix_col_assignment_vector_and_broadcast() {
    let code = r#"
        let mut M = mat [
            1.0, 2.0 ;
            3.0, 4.0 ;
            5.0, 6.0
        ];
        M[.., 1] = [20.0, 40.0, 60.0];
        M[.., 0] = 0.0;
        let res = M;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 3);
            assert_eq!(cols, 2);
            assert_eq!(data[0 * 2 + 0], 0.0);
            assert_eq!(data[0 * 2 + 1], 20.0);
            assert_eq!(data[1 * 2 + 0], 0.0);
            assert_eq!(data[1 * 2 + 1], 40.0);
            assert_eq!(data[2 * 2 + 0], 0.0);
            assert_eq!(data[2 * 2 + 1], 60.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_matrix_submatrix_assignment() {
    let code = r#"
        let mut M = mat [
            1.0, 2.0, 3.0 ;
            4.0, 5.0, 6.0 ;
            7.0, 8.0, 9.0
        ];
        let sub = mat [
            10.0, 20.0 ;
            30.0, 40.0
        ];
        M[0..2, 1..3] = sub;
        let res = M;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 3);
            assert_eq!(cols, 3);
            // Row 0: 1.0, 10.0, 20.0
            assert_eq!(data[0 * 3 + 0], 1.0);
            assert_eq!(data[0 * 3 + 1], 10.0);
            assert_eq!(data[0 * 3 + 2], 20.0);
            // Row 1: 4.0, 30.0, 40.0
            assert_eq!(data[1 * 3 + 0], 4.0);
            assert_eq!(data[1 * 3 + 1], 30.0);
            assert_eq!(data[1 * 3 + 2], 40.0);
            // Row 2: 7.0, 8.0, 9.0 (untouched)
            assert_eq!(data[2 * 3 + 0], 7.0);
            assert_eq!(data[2 * 3 + 1], 8.0);
            assert_eq!(data[2 * 3 + 2], 9.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_immutable_indexed_assignment_rejected_by_typechecker() {
    let code = r#"
        let v = [1.0, 2.0, 3.0];
        v[0] = 99.0;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut checker = TypeChecker::new("test.gh".to_string(), code);
    let diags = checker
        .check_program(&program)
        .expect_err("Should reject assigning to immutable variable");
    assert!(
        diags.iter().any(|d| d.code == "C0104"),
        "Expected C0104 error"
    );
}

#[test]
fn test_matrix_copy_on_write_isolation() {
    let code = r#"
        let mut m1 = mat [
            1.0, 2.0 ;
            3.0, 4.0
        ];
        let m2 = m1;
        m1[0, 0] = 999.0;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let m1 = interp.env.get("m1").unwrap();
    let m2 = interp.env.get("m2").unwrap();

    if let (Value::Matrix { data: d1, .. }, Value::Matrix { data: d2, .. }) = (m1, m2) {
        assert_eq!(d1[0], 999.0, "m1 was updated in-place");
        assert_eq!(d2[0], 1.0, "m2 was preserved via Copy-on-Write");
    } else {
        panic!("Expected both to be matrices");
    }
}

#[test]
fn test_struct_field_mutation_and_compound_ops() {
    let code = r#"
        struct Point { x: f64, y: f64 }
        let mut p = Point { x: 10.0, y: 20.0 };
        p.x = 99.0;
        p.y += 5.0;
        p.x -= 9.0;
        p.y *= 2.0;
        let rx = p.x;
        let ry = p.y;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("rx"), Some(Value::F64(90.0)));
    assert_eq!(interp.env.get("ry"), Some(Value::F64(50.0)));
}

#[test]
fn test_record_field_mutation_and_compound_ops() {
    let code = r#"
        let mut r = { a: 10, b: 20.0 };
        r.a = 42;
        r.a += 8;
        r.b *= 2.0;
        let ra = r.a;
        let rb = r.b;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("ra"), Some(Value::I64(50)));
    assert_eq!(interp.env.get("rb"), Some(Value::F64(40.0)));
}

#[test]
fn test_dataframe_column_assignment_and_broadcast() {
    let code = r#"
        let mut df = dataframe {
            id: [1, 2, 3],
            score: [10.0, 20.0, 30.0]
        };
        df["score"] = [100.0, 200.0, 300.0];
        df["bonus"] = 5.0;
        let s = df["score"];
        let b = df["bonus"];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("s").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 3);
            assert_eq!(vd.value_at(0), Some(Value::F64(100.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(200.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(300.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }

    match interp.env.get("b").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 3);
            assert_eq!(vd.value_at(0), Some(Value::F64(5.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(5.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(5.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_dataframe_column_compound_ops() {
    let code = r#"
        let mut df = dataframe {
            score: [10.0, 20.0, 30.0]
        };
        df["score"] += 5.0;
        df.score *= 2.0;
        let res = df["score"];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 3);
            // (10 + 5) * 2 = 30; (20 + 5) * 2 = 50; (30 + 5) * 2 = 70
            assert_eq!(vd.value_at(0), Some(Value::F64(30.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(50.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(70.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_dataframe_conditional_cell_assignment() {
    let code = r#"
        let mut df = dataframe {
            score: [10.0, 40.0, 20.0, 80.0]
        };
        df[df["score"] > 30.0, "score"] = 0.0;
        let res = df["score"];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 4);
            assert_eq!(vd.value_at(0), Some(Value::F64(10.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(0.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(20.0)));
            assert_eq!(vd.value_at(3), Some(Value::F64(0.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_dataframe_cell_compound_ops() {
    let code = r#"
        let mut df = dataframe {
            score: [10.0, 20.0, 30.0]
        };
        df[0, "score"] += 5.0;
        df[1..3, "score"] *= 2.0;
        let res = df["score"];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 3);
            assert_eq!(vd.value_at(0), Some(Value::F64(15.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(40.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(60.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_vector_and_matrix_compound_ops() {
    let code = r#"
        let mut v = [1.0, 2.0, 3.0, 4.0];
        v[0] += 10.0;
        v[1..3] *= 3.0;
        let res_v = v;

        let mut M = mat [
            1.0, 2.0 ;
            3.0, 4.0
        ];
        M[0, 1] += 10.0;
        M[1, ..] *= 2.0;
        let res_m = M;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("res_v").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 4);
            assert_eq!(vd.value_at(0), Some(Value::F64(11.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(6.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(9.0)));
            assert_eq!(vd.value_at(3), Some(Value::F64(4.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }

    match interp.env.get("res_m").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 2);
            assert_eq!(cols, 2);
            assert_eq!(data[0 * 2 + 0], 1.0);
            assert_eq!(data[0 * 2 + 1], 12.0); // 2.0 + 10.0
            assert_eq!(data[1 * 2 + 0], 6.0); // 3.0 * 2.0
            assert_eq!(data[1 * 2 + 1], 8.0); // 4.0 * 2.0
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_immutable_field_and_df_rejected_by_typechecker() {
    let code_struct = r#"
        struct Point { x: f64, y: f64 }
        let p = Point { x: 1.0, y: 2.0 };
        p.x = 99.0;
    "#;
    let program = parse(code_struct).expect("syntax ok");
    let mut checker = TypeChecker::new("test_struct.gh".to_string(), code_struct);
    let diags = checker
        .check_program(&program)
        .expect_err("Should reject immutable struct field assign");
    assert!(
        diags.iter().any(|d| d.code == "C0104"),
        "Expected C0104 for struct"
    );

    let code_df = r#"
        let df = dataframe { score: [1.0, 2.0] };
        df["score"] = [10.0, 20.0];
    "#;
    let program = parse(code_df).expect("syntax ok");
    let mut checker = TypeChecker::new("test_df.gh".to_string(), code_df);
    let diags = checker
        .check_program(&program)
        .expect_err("Should reject immutable dataframe assign");
    assert!(
        diags.iter().any(|d| d.code == "C0104"),
        "Expected C0104 for dataframe"
    );
}
