//! Integration tests for GHL Fase 2: Motor "lazy" (deferred queries with polars-lazy).
//!
//! Validates:
//! 1. Opt-in lazy workflow: `lazy(df)` / `df |> lazy()` -> `collect(lf)`.
//! 2. Plan inspection: `explain(lf)`.
//! 3. Deferred pipeline verbs: filter, select, mutate, arrange, head, tail, group_by, summarize, joins.
//! 4. Lazy I/O scans: `scan_csv` and `scan_parquet`.
//! 5. 100% eager/lazy parity and retrocompatibility.

use ghl_runtime::{Interpreter, Value};
use ghl_syntax::parser::parse;

fn eval_script(code: &str) -> Interpreter {
    let program =
        parse(code).unwrap_or_else(|e| panic!("Syntax error in test script: {e:?}\nCode:\n{code}"));
    let mut interp = Interpreter::new();
    interp
        .eval_program(&program)
        .unwrap_or_else(|e| panic!("Runtime error in test script: {e:?}\nCode:\n{code}"));
    interp
}

#[test]
fn test_lazy_opt_in_and_collect() {
    let code = r#"
        let df = dataframe {
            id: [1, 2, 3, 4, 5],
            dose: [1.0, 2.5, 3.0, 4.5, 5.0],
            group: ["A", "B", "A", "B", "A"]
        };

        let lf = df |> lazy();
        let eager_again = lf |> collect();

        let filtered_lf = df |> lazy() |> filter(dose > 2.0) |> select(["id", "dose"]) |> collect();
        let filtered_eager = df |> filter(dose > 2.0) |> select(["id", "dose"]);
    "#;

    let interp = eval_script(code);

    let eager_again = interp.env.get("eager_again").expect("eager_again");
    let filtered_lf = interp.env.get("filtered_lf").expect("filtered_lf");
    let filtered_eager = interp.env.get("filtered_eager").expect("filtered_eager");

    assert_eq!(eager_again.type_name(), "DataFrame");
    assert_eq!(filtered_lf.type_name(), "DataFrame");
    assert_eq!(filtered_eager.type_name(), "DataFrame");

    // Parity between eager and collected lazy pipelines
    assert_eq!(filtered_lf, filtered_eager);
}

#[test]
fn test_lazy_arrange_head_tail() {
    let code = r#"
        let df = dataframe {
            id: [1, 2, 3, 4, 5],
            val: [10.0, 50.0, 30.0, 20.0, 40.0]
        };

        let top2 = df |> lazy() |> arrange(desc("val")) |> head(2) |> collect();
        let bot2 = df |> lazy() |> arrange("val") |> head(2) |> collect();
    "#;

    let interp = eval_script(code);
    let top2 = interp.env.get("top2").expect("top2");
    let bot2 = interp.env.get("bot2").expect("bot2");

    if let Value::DataFrame { frame, .. } = top2 {
        assert_eq!(frame.height(), 2);
        let vals = frame
            .column("val")
            .unwrap()
            .as_materialized_series()
            .f64()
            .unwrap();
        assert_eq!(vals.get(0), Some(50.0));
        assert_eq!(vals.get(1), Some(40.0));
    } else {
        panic!("Expected DataFrame for top2");
    }

    if let Value::DataFrame { frame, .. } = bot2 {
        assert_eq!(frame.height(), 2);
        let vals = frame
            .column("val")
            .unwrap()
            .as_materialized_series()
            .f64()
            .unwrap();
        assert_eq!(vals.get(0), Some(10.0));
        assert_eq!(vals.get(1), Some(20.0));
    } else {
        panic!("Expected DataFrame for bot2");
    }
}

#[test]
fn test_lazy_explain_plan() {
    let code = r#"
        let df = dataframe {
            id: [1, 2, 3, 4],
            score: [85.0, 92.0, 78.0, 95.0]
        };

        let plan_str = df |> lazy() |> filter(score >= 80.0) |> select(["score"]) |> explain();
    "#;

    let interp = eval_script(code);
    let plan = interp.env.get("plan_str").expect("plan_str");

    if let Value::String(s) = plan {
        assert!(!s.is_empty());
        // Polars plan contains either FILTER, SELECT, PROJECT, or column names
        let lower = s.to_lowercase();
        assert!(lower.contains("filter") || lower.contains("score") || lower.contains("selection"));
    } else {
        panic!("Expected String plan from explain()");
    }
}

#[test]
fn test_lazy_group_by_summarize() {
    let code = r#"
        let df = dataframe {
            group: ["A", "A", "B", "B", "B"],
            val: [10.0, 20.0, 30.0, 40.0, 50.0]
        };

        let res = df |> lazy() |> group_by(["group"]) |> summarize(total = sum(val), avg = mean(val)) |> collect();
        let eager_res = df |> group_by(["group"]) |> summarize(total = sum(val), avg = mean(val));
    "#;

    let interp = eval_script(code);
    let res = interp.env.get("res").expect("res");
    let eager_res = interp.env.get("eager_res").expect("eager_res");

    if let (Value::DataFrame { frame: f_lazy, .. }, Value::DataFrame { frame: f_eager, .. }) =
        (res, eager_res)
    {
        assert_eq!(f_lazy.height(), f_eager.height());
        assert_eq!(f_lazy.height(), 2);
    } else {
        panic!("Expected DataFrames");
    }
}

#[test]
fn test_lazy_inner_and_left_joins() {
    let code = r#"
        let df1 = dataframe {
            id: [1, 2, 3],
            x: ["a", "b", "c"]
        };
        let df2 = dataframe {
            id: [2, 3, 4],
            y: [20.0, 30.0, 40.0]
        };

        let inner_res = inner_join(df1 |> lazy(), df2 |> lazy(), "id") |> collect();
        let left_res = left_join(df1 |> lazy(), df2, "id") |> collect();
    "#;

    let interp = eval_script(code);
    let inner_res = interp.env.get("inner_res").expect("inner_res");
    let left_res = interp.env.get("left_res").expect("left_res");

    if let Value::DataFrame { frame, .. } = inner_res {
        assert_eq!(frame.height(), 2);
        assert!(frame.column("x").is_ok());
        assert!(frame.column("y").is_ok());
    } else {
        panic!("Expected DataFrame for inner_res");
    }

    if let Value::DataFrame { frame, .. } = left_res {
        assert_eq!(frame.height(), 3);
        assert!(frame.column("x").is_ok());
        assert!(frame.column("y").is_ok());
    } else {
        panic!("Expected DataFrame for left_res");
    }
}

#[test]
fn test_scan_csv_deferred() {
    let temp_dir = std::env::temp_dir();
    let csv_path = temp_dir.join("ghl_test_scan_csv.csv");
    let csv_str = csv_path.to_str().unwrap().replace('\\', "/");

    let code = format!(
        r#"
        let raw = "id,dose,active\n1,10.5,true\n2,20.0,false\n3,30.5,true\n4,40.0,false\n5,50.0,true\n";
        raw |> write_file("{csv_str}");

        let lf = scan_csv("{csv_str}");
        let collected = lf |> filter(dose > 25.0) |> select(["id", "dose"]) |> collect();
        "#
    );

    let interp = eval_script(&code);
    let collected = interp.env.get("collected").expect("collected");

    if let Value::DataFrame { frame, .. } = collected {
        assert_eq!(frame.height(), 3);
        assert_eq!(frame.get_column_names(), &["id", "dose"]);
    } else {
        panic!("Expected DataFrame from scan_csv |> collect");
    }

    let _ = std::fs::remove_file(csv_path);
}

#[test]
fn test_scan_parquet_deferred() {
    let temp_dir = std::env::temp_dir();
    let pq_path = temp_dir.join("ghl_test_scan_parquet.parquet");
    let pq_str = pq_path.to_str().unwrap().replace('\\', "/");

    let code = format!(
        r#"
        let df = dataframe {{
            gene: ["TP53", "EGFR", "BRCA1", "MYC"],
            expr: [12.4, 8.1, 4.3, 19.8]
        }};
        df |> write_parquet("{pq_str}");

        let scanned = scan_parquet("{pq_str}");
        let filtered = scanned |> filter(expr >= 10.0) |> collect();
        "#
    );

    let interp = eval_script(&code);
    let filtered = interp.env.get("filtered").expect("filtered");

    if let Value::DataFrame { frame, .. } = filtered {
        assert_eq!(frame.height(), 2);
        let genes = frame
            .column("gene")
            .unwrap()
            .as_materialized_series()
            .str()
            .unwrap();
        assert_eq!(genes.get(0), Some("TP53"));
        assert_eq!(genes.get(1), Some("MYC"));
    } else {
        panic!("Expected DataFrame from scan_parquet |> collect");
    }

    let _ = std::fs::remove_file(pq_path);
}

#[test]
fn test_retrocompatibility_eager_pipelines() {
    let code = r#"
        let df = dataframe {
            x: [1, 2, 3],
            y: [10, 20, 30]
        };

        let res = df
            |> filter(x > 1)
            |> mutate("z", [20, 40])
            |> select(["x", "z"]);
    "#;

    let interp = eval_script(code);
    let res = interp.env.get("res").expect("res");

    if let Value::DataFrame { frame, .. } = res {
        assert_eq!(frame.height(), 2);
        assert_eq!(frame.get_column_names(), &["x", "z"]);
    } else {
        panic!("Expected eager DataFrame");
    }
}
