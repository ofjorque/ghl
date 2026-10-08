//! Interpreter control flow (match/return/tail recursion/while/for) and basic DataFrame filter predicates.

mod common;
use common::*;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

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
fn test_return_short_circuits_if_branch() {
    // The exact script that exposed the bug: return was a silent no-op -- both
    // early(5) and early(-5) used to give 42.
    let code = r#"
        fn early(x) {
            if x > 0 {
                return 999;
            };
            42
        }
        let a = early(5);
        let b = early(-5);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("a"), Some(Value::I64(999)));
    assert_eq!(interp.env.get("b"), Some(Value::I64(42)));
}

#[test]
fn test_return_short_circuits_nested_block() {
    // return nested two levels deep (inside an if, inside another if) must still
    // cut all the way up to the function boundary, not just its immediate block.
    let code = r#"
        fn classify(x) {
            if x > 100 {
                if x > 1000 {
                    return "huge";
                };
                return "big";
            };
            "small"
        }
        let a = classify(5000);
        let b = classify(500);
        let c = classify(5);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("a"), Some(Value::String("huge".to_string())));
    assert_eq!(interp.env.get("b"), Some(Value::String("big".to_string())));
    assert_eq!(
        interp.env.get("c"),
        Some(Value::String("small".to_string()))
    );
}

#[test]
fn test_return_at_top_level_stops_program() {
    let code = r#"
        let a = 1;
        return 0;
        let b = 2;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("a"), Some(Value::I64(1)));
    assert_eq!(
        interp.env.get("b"),
        None,
        "statement after a top-level return must not run"
    );
}

#[test]
fn test_return_does_not_leak_into_caller() {
    // A callee's internal return must not short-circuit the caller's own code
    // that runs after the call returns -- pending_return has to be consumed at
    // the function-call boundary, not left set.
    let code = r#"
        fn inner(x) {
            if x > 0 {
                return 1;
            };
            0
        }
        fn outer() {
            let a = inner(5);
            let b = 100;
            b
        }
        let result = outer();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("result"), Some(Value::I64(100)));
}

#[test]
fn test_tail_recursive_function_handles_deep_recursion() {
    // TODO.md Fase 7: without TCO this crashes the native stack around ~1,000-1,500
    // levels in release (measured). 10,000 is ~7-10x that -- enough to decisively
    // prove the trampoline in a debug-profile unit test without the multi-second
    // runtime deeper depths take unoptimized (each recursive lookup clones the
    // whole captured closure env, prelude included -- see the separate release-mode
    // spike for the real, much deeper number reported in TODO.md).
    let code = r#"
        fn count_down(n) {
            if n <= 0 { 0 } else { count_down(n - 1) }
        }
        let result = count_down(10000);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("result"), Some(Value::I64(0)));
}

#[test]
fn test_mutual_tail_recursion_handles_deep_recursion() {
    let code = r#"
        fn is_even(n) {
            if n <= 0 { true } else { is_odd(n - 1) }
        }
        fn is_odd(n) {
            if n <= 0 { false } else { is_even(n - 1) }
        }
        let result = is_even(10000);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("result"), Some(Value::Bool(true)));
}

#[test]
fn test_tail_call_via_pipe_is_optimized() {
    let code = r#"
        fn count_down(n) {
            if n <= 0 { 0 } else { (n - 1) |> count_down() }
        }
        let result = count_down(10000);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("result"), Some(Value::I64(0)));
}

#[test]
fn test_while_loop_accumulates_via_assignment() {
    // The "real" fix the recursion-depth finding was ultimately asking for: actual
    // imperative iteration + mutable reassignment, not just TCO over recursion.
    let code = r#"
        let mut i = 0;
        let mut acc = 0;
        while i < 10 {
            acc = acc + i;
            i = i + 1;
        };
        let result = acc;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("result"), Some(Value::I64(45)));
}

#[test]
fn test_for_range_accumulates() {
    // `for i in 0..10` should sum 0+1+...+9 = 45, equivalent to the while-loop test.
    let code = r#"
        let mut acc = 0;
        for i in 0..10 {
            acc = acc + i;
        };
        let result = acc;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("result"), Some(Value::I64(45)));
}

#[test]
fn test_for_loop_variable_does_not_leak() {
    // The loop variable `i` must not be visible outside the for loop.
    let code = r#"
        let before = 99;
        for i in 0..3 {
            let tmp = i;
        };
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    // `i` must not exist in the outer env after the loop.
    assert!(interp.env.get("i").is_none(), "loop variable must not leak");
    assert_eq!(interp.env.get("before"), Some(Value::I64(99)));
}

#[test]
fn test_assign_rejects_undeclared_variable() {
    // Interpreter-level check (RuntimeEnv::assign returns false) -- exercised
    // directly, independent of whether the type checker ran first.
    let code = r#"x = 1;"#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let err = interp
        .eval_program(&program)
        .expect_err("undeclared assignment must fail");
    assert_eq!(err.code, "C0101");
}

#[test]
fn test_while_handles_deep_iteration_without_stack_growth() {
    // Unlike the TCO trampoline (eval.rs), a `while` loop never recurses into Rust
    // to begin with, so it was never at risk of the ~1,000-1,500 crash threshold
    // TCO was built to work around -- this is a structurally different, simpler
    // fix, not just another way to reach the same result.
    let code = r#"
        let mut i = 0;
        while i < 2000000 {
            i = i + 1;
        };
        let result = i;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("result"), Some(Value::I64(2_000_000)));
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
    assert_eq!(
        df_column(&only_na, "id"),
        vec![Value::I64(2), Value::I64(4)]
    );

    let without_na = interp.env.get("without_na").expect("without_na exists");
    assert_eq!(
        df_column(&without_na, "id"),
        vec![Value::I64(1), Value::I64(3)]
    );
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
    assert!(
        result.is_err(),
        "filter() with a nonsensical predicate must error, not no-op"
    );
}

#[test]
fn test_break_in_while_and_for_loops() {
    let code = r#"
        let mut count_while = 0;
        while true {
            count_while = count_while + 1;
            if count_while == 5 {
                break;
            }
        };

        let mut sum_for = 0;
        for i in 0..100 {
            if i >= 10 {
                break;
            }
            sum_for = sum_for + i;
        }
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("count_while"), Some(Value::I64(5)));
    assert_eq!(interp.env.get("sum_for"), Some(Value::I64(45)));
}

#[test]
fn test_continue_in_while_and_for_loops() {
    let code = r#"
        let mut sum_evens = 0;
        for i in 0..10 {
            if i % 2 != 0 {
                continue;
            }
            sum_evens = sum_evens + i;
        }

        let mut w = 0;
        let mut skipped_five = 0;
        while w < 10 {
            w = w + 1;
            if w == 5 {
                continue;
            }
            skipped_five = skipped_five + w;
        };
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    // 0 + 2 + 4 + 6 + 8 = 20
    assert_eq!(interp.env.get("sum_evens"), Some(Value::I64(20)));
    // (1..=10 sum = 55) - 5 = 50
    assert_eq!(interp.env.get("skipped_five"), Some(Value::I64(50)));
}

#[test]
fn test_type_conversion_helpers() {
    let code = r#"
        let x_int = to_int(42.8);
        let x_float = to_float(100);
        let x_bool = to_bool(1);
        let str_int = "123".to_int();
        let str_float = "3.1415".to_float();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert_eq!(interp.env.get("x_int"), Some(Value::I64(42)));
    assert_eq!(interp.env.get("x_float"), Some(Value::F64(100.0)));
    assert_eq!(interp.env.get("x_bool"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("str_int"), Some(Value::I64(123)));
    assert_eq!(interp.env.get("str_float"), Some(Value::F64(3.1415)));
}
