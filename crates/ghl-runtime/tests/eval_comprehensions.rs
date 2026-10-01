use ghl_syntax::parser::parse;
use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;

#[test]
fn test_1d_vector_comprehension_with_range() {
    let code = r#"
        let evens = [x * 2 for x in 0..5];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("evens").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 5);
            assert_eq!(vd.value_at(0), Some(Value::I64(0)));
            assert_eq!(vd.value_at(1), Some(Value::I64(2)));
            assert_eq!(vd.value_at(2), Some(Value::I64(4)));
            assert_eq!(vd.value_at(3), Some(Value::I64(6)));
            assert_eq!(vd.value_at(4), Some(Value::I64(8)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_1d_vector_comprehension_with_condition() {
    let code = r#"
        let evens = [x for x in 0..10 if x % 2 == 0];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("evens").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 5);
            assert_eq!(vd.value_at(0), Some(Value::I64(0)));
            assert_eq!(vd.value_at(1), Some(Value::I64(2)));
            assert_eq!(vd.value_at(2), Some(Value::I64(4)));
            assert_eq!(vd.value_at(3), Some(Value::I64(6)));
            assert_eq!(vd.value_at(4), Some(Value::I64(8)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_1d_vector_comprehension_over_vector() {
    let code = r#"
        let original = [10.0, 20.0, 30.0];
        let scaled = [x * 1.5 for x in original];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("scaled").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 3);
            assert_eq!(vd.value_at(0), Some(Value::F64(15.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(30.0)));
            assert_eq!(vd.value_at(2), Some(Value::F64(45.0)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_anti_python_scope_leak() {
    let code = r#"
        let x = 999;
        let result = [x * 2 for x in 0..5];
        let outer_x = x;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    // Outer x must remain 999 -- zero scope leak!
    assert_eq!(interp.env.get("outer_x").unwrap(), Value::I64(999));
}

#[test]
fn test_2d_matrix_comprehension_standard_fila_columna() {
    let code = r#"
        let M = [r * 10.0 + c for r in 0..3, c in 0..4];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("M").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 3);
            assert_eq!(cols, 4);
            // Verify row-major layout: data[r * cols + c]
            assert_eq!(data[0 * 4 + 0], 0.0);
            assert_eq!(data[0 * 4 + 1], 1.0);
            assert_eq!(data[0 * 4 + 2], 2.0);
            assert_eq!(data[0 * 4 + 3], 3.0);
            assert_eq!(data[1 * 4 + 0], 10.0);
            assert_eq!(data[1 * 4 + 1], 11.0);
            assert_eq!(data[2 * 4 + 3], 23.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_2d_matrix_comprehension_for_separated() {
    let code = r#"
        let M = [r * 10.0 + c for r in 0..2 for c in 0..3];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("M").unwrap() {
        Value::Matrix { rows, cols, data } => {
            assert_eq!(rows, 2);
            assert_eq!(cols, 3);
            assert_eq!(data[0 * 3 + 0], 0.0);
            assert_eq!(data[1 * 3 + 2], 12.0);
        }
        other => panic!("Expected Matrix, found {other:?}"),
    }
}

#[test]
fn test_2d_filtered_comprehension_flattens_to_vector() {
    let code = r#"
        let upper = [r * 10 + c for r in 0..3, c in 0..3 if r < c];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    // Because `if` breaks the rectangular grid, it cleanly produces a 1D Vector (like in Julia)
    match interp.env.get("upper").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 3);
            // r=0, c=1 => 1
            // r=0, c=2 => 2
            // r=1, c=2 => 12
            assert_eq!(vd.value_at(0), Some(Value::I64(1)));
            assert_eq!(vd.value_at(1), Some(Value::I64(2)));
            assert_eq!(vd.value_at(2), Some(Value::I64(12)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_2d_dependent_iterators() {
    let code = r#"
        let triangular = [c for r in 1..4, c in 0..r];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    // r=1: c in 0..1 => [0]
    // r=2: c in 0..2 => [0, 1]
    // r=3: c in 0..3 => [0, 1, 2]
    match interp.env.get("triangular").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 6);
            assert_eq!(vd.value_at(0), Some(Value::I64(0)));
            assert_eq!(vd.value_at(1), Some(Value::I64(0)));
            assert_eq!(vd.value_at(2), Some(Value::I64(1)));
            assert_eq!(vd.value_at(3), Some(Value::I64(0)));
            assert_eq!(vd.value_at(4), Some(Value::I64(1)));
            assert_eq!(vd.value_at(5), Some(Value::I64(2)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}

#[test]
fn test_3d_cartesian_product_comprehension() {
    let code = r#"
        let cube = [x + y + z for x in 0..2, y in 0..2, z in 0..2];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    match interp.env.get("cube").unwrap() {
        Value::Vector(vd) => {
            assert_eq!(vd.len(), 8);
            // 0+0+0 = 0
            assert_eq!(vd.value_at(0), Some(Value::I64(0)));
            // 1+1+1 = 3
            assert_eq!(vd.value_at(7), Some(Value::I64(3)));
        }
        other => panic!("Expected Vector, found {other:?}"),
    }
}
