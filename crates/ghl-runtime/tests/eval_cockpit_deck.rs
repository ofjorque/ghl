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

#[test]
fn test_cockpit_panel_progress_and_circle() {
    let src = r#"
        let p = cockpit("Solver Diagnostics", "CONVERGED")
            |> cockpit_add_kv("Iterations", "150")
            |> cockpit_add_progress(150, 200, "Budget", "emerald")
            |> cockpit_add_circle(100, 100, "Warmup", "haru")
            |> cockpit_add_circle(45, 100, "Sampling", "cyan");

        let text = format_cockpit(p);
        text
    "#;
    let res = eval_source(src).expect("evaluation should succeed");
    if let Value::String(s) = res {
        assert!(s.contains("Solver Diagnostics"), "text should have title: {}", s);
        assert!(s.contains("Budget"), "text should contain Budget progress label: {}", s);
        assert!(s.contains("75.0%"), "text should contain 75.0%: {}", s);
        assert!(s.contains("Warmup"), "text should contain Warmup: {}", s);
        assert!(s.contains("Sampling"), "text should contain Sampling: {}", s);
    } else {
        panic!("expected string, got {:?}", res);
    }
}

#[test]
fn test_progress_bar_and_done_runtime() {
    let src = r#"
        for i in 1..=5 {
            progress_bar(i, 5, "MCMC Sampling", "step: " + string(i), "gradient");
        }
        progress_done("MCMC Sampling converged.");
    "#;
    let res = eval_source(src).expect("progress bar loop should evaluate successfully");
    assert_eq!(res, Value::Unit);
}

#[test]
fn test_progress_spinner_and_done_runtime() {
    let src = r#"
        for i in 1..=3 {
            progress_spinner("Streaming Query", "rows: " + string(i * 100), "dots", "cyan");
        }
        progress_done("Streaming complete.");
    "#;
    let res = eval_source(src).expect("spinner loop should evaluate successfully");
    assert_eq!(res, Value::Unit);
}

#[test]
fn test_sleep_and_progress_delay_runtime() {
    let src = r#"
        set_progress_delay(5);
        sleep(10);
        sleep_ms(5);
        for i in 1..=3 {
            progress_bar(i, 3, "Delayed Bar", "step: " + string(i), "emerald");
        }
        progress_done("Finished delayed bar");
        set_progress_delay(0);
    "#;
    let res = eval_source(src).expect("sleep and delayed progress should evaluate successfully");
    assert_eq!(res, Value::Unit);
}
