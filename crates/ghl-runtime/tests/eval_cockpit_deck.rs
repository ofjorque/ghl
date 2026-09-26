use ghl_runtime::{Interpreter, Value};
use ghl_syntax::parse;

fn eval_source(src: &str) -> Result<Value, String> {
    let program = parse(src).map_err(|e| format!("{:?}", e))?;
    let mut interp = Interpreter::new();
    interp.eval_program(&program).map_err(|e| format!("{:?}", e))
}

#[test]
fn test_cockpit_creation_and_formatting() {
    let src = r#"
        let p = cockpit("Test Diagnostics", "OK")
            |> cockpit_add_kv("Metric A", "123.45")
            |> cockpit_add_kv("Metric B", "0.001")
            |> cockpit_add_divider()
            |> cockpit_add_line("All assumptions verified.");

        let text = format_cockpit(p);
        text
    "#;
    let res = eval_source(src).expect("evaluation should succeed");
    if let Value::String(s) = res {
        assert!(s.contains("Test Diagnostics"), "rendered text should contain title: {}", s);
        assert!(s.contains("Metric A"), "rendered text should contain Metric A: {}", s);
        assert!(s.contains("123.45"), "rendered text should contain value: {}", s);
        assert!(s.contains("All assumptions verified."), "rendered text should contain line: {}", s);
    } else {
        panic!("expected string, got {:?}", res);
    }
}

#[test]
fn test_cockpit_with_badge_and_render() {
    let src = r#"
        let p = cockpit("Audit Report");
        let p2 = cockpit_with_badge(p, "VERIFIED");
        let p3 = cockpit_add_kv(p2, "Status", "Passed");
        show_cockpit(p3);
        p3
    "#;
    let res = eval_source(src).expect("evaluation should succeed");
    assert_eq!(res.type_name(), "CockpitPanel");
}

#[test]
fn test_sparkline_generation() {
    let src = r#"
        let vals = [1.0, 2.0, 5.0, 10.0, 4.0, 1.0];
        let spark = sparkline(vals);
        spark
    "#;
    let res = eval_source(src).expect("evaluation should succeed");
    if let Value::String(s) = res {
        assert!(!s.is_empty(), "sparkline should not be empty");
    } else {
        panic!("expected string, got {:?}", res);
    }
}
