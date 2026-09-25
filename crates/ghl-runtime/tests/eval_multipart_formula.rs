use ghl_runtime::{Interpreter, Value};
use ghl_syntax::ast::FormulaOp;
use ghl_syntax::parser::parse;

#[test]
fn test_multipart_formula_eval_two_parts() {
    let code = r#"
        let f = y ~ x1 + x2 | entity + time;
    "#;
    let program = parse(code).expect("parse ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    let val = interp.env.get("f").expect("f exists");
    match &val {
        Value::Formula { op, response, terms, parts } => {
            assert_eq!(*op, FormulaOp::Regression);
            assert_eq!(response, "y");
            assert_eq!(terms, &vec!["x1".to_string(), "x2".to_string()]);
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0], vec!["x1".to_string(), "x2".to_string()]);
            assert_eq!(parts[1], vec!["entity".to_string(), "time".to_string()]);
        }
        other => panic!("Expected Formula, got {other:?}"),
    }

    // Test Rust FormulaParts API
    let fp = val.formula_parts().expect("should have formula parts");
    assert_eq!(fp.response, "y");
    assert_eq!(fp.terms, vec!["x1", "x2"]);
    assert_eq!(fp.absorbed, vec!["entity", "time"]);
    assert!(fp.instruments.is_empty());
    assert_eq!(fp.parts_count(), 2);
    assert!(fp.has_fixed_effects());
    assert!(!fp.has_instruments());
    assert_eq!(fp.fixed_effects(), Some(&["entity".to_string(), "time".to_string()][..]));
    assert_eq!(fp.instruments(), Some(&["entity".to_string(), "time".to_string()][..]));
}

#[test]
fn test_multipart_formula_eval_three_parts() {
    let code = r#"
        let f = y ~ x_exog + x_endog | entity + time | z1 + z2;
    "#;
    let program = parse(code).expect("parse ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    let val = interp.env.get("f").expect("f exists");
    match &val {
        Value::Formula { op, response, terms, parts } => {
            assert_eq!(*op, FormulaOp::Regression);
            assert_eq!(response, "y");
            assert_eq!(terms, &vec!["x_exog".to_string(), "x_endog".to_string()]);
            assert_eq!(parts.len(), 3);
            assert_eq!(parts[0], vec!["x_exog".to_string(), "x_endog".to_string()]);
            assert_eq!(parts[1], vec!["entity".to_string(), "time".to_string()]);
            assert_eq!(parts[2], vec!["z1".to_string(), "z2".to_string()]);
        }
        other => panic!("Expected Formula, got {other:?}"),
    }

    let fp = val.formula_parts().expect("should have formula parts");
    assert_eq!(fp.parts_count(), 3);
    assert!(fp.has_fixed_effects());
    assert!(fp.has_instruments());
    assert_eq!(fp.fixed_effects(), Some(&["entity".to_string(), "time".to_string()][..]));
    assert_eq!(fp.instruments(), Some(&["z1".to_string(), "z2".to_string()][..]));
}

#[test]
fn test_multipart_formula_ghl_field_access() {
    let code = r#"
        let f = y ~ x1 + x2 | entity + time | z_instr;
        let resp = f.response;
        let t = f.terms;
        let p = f.parts;
        let fe = f.absorbed;
        let iv = f.instruments;
        let cnt = f.parts_count;
        let has_fe = f.has_fixed_effects;
        let has_iv = f.has_instruments;
    "#;
    let program = parse(code).expect("parse ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    assert_eq!(interp.env.get("resp").unwrap(), Value::String("y".into()));
    assert_eq!(interp.env.get("cnt").unwrap(), Value::I64(3));
    assert_eq!(interp.env.get("has_fe").unwrap(), Value::Bool(true));
    assert_eq!(interp.env.get("has_iv").unwrap(), Value::Bool(true));

    if let Value::Vector(vd) = interp.env.get("t").unwrap() {
        let items: Vec<String> = vd.iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(items, vec!["x1", "x2"]);
    } else {
        panic!("Expected Vector for f.terms");
    }

    if let Value::Vector(vd) = interp.env.get("fe").unwrap() {
        let items: Vec<String> = vd.iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(items, vec!["entity", "time"]);
    } else {
        panic!("Expected Vector for f.absorbed");
    }

    if let Value::Vector(vd) = interp.env.get("iv").unwrap() {
        let items: Vec<String> = vd.iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(items, vec!["z_instr"]);
    } else {
        panic!("Expected Vector for f.instruments");
    }
}

#[test]
fn test_formula_parts_native_verb() {
    let code = r#"
        let f = log_wage ~ educ + exper | industry + state | near_college;
        let fp = formula_parts(f);
        let resp = fp.response;
        let fe = fp.absorbed;
        let cnt = fp.parts_count;
    "#;
    let program = parse(code).expect("parse ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    assert_eq!(interp.env.get("resp").unwrap(), Value::String("log_wage".into()));
    assert_eq!(interp.env.get("cnt").unwrap(), Value::I64(3));
    if let Value::Vector(vd) = interp.env.get("fe").unwrap() {
        let items: Vec<String> = vd.iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(items, vec!["industry", "state"]);
    } else {
        panic!("Expected Vector for fp.absorbed");
    }
}

#[test]
fn test_single_part_backward_compatibility() {
    let code = r#"
        let f = y ~ x1 + x2;
        let cnt = f.parts_count;
        let has_fe = f.has_fixed_effects;
        let fe = f.absorbed;
    "#;
    let program = parse(code).expect("parse ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    assert_eq!(interp.env.get("cnt").unwrap(), Value::I64(1));
    assert_eq!(interp.env.get("has_fe").unwrap(), Value::Bool(false));
    if let Value::Vector(vd) = interp.env.get("fe").unwrap() {
        assert_eq!(vd.len(), 0);
    } else {
        panic!("Expected empty Vector for f.absorbed in 1-part formula");
    }
}
