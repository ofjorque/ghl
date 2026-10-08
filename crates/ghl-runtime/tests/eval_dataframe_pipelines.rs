//! Grammar of Graphics, DataFrame wrangling pipelines (group_by/summarize/arrange/join), Parquet round-trip, comparisons.

mod common;
use common::*;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_syntax::parser::parse;

#[test]
fn test_grammar_of_graphics_pipeline() {
    let code = r#"
        let df = dataframe {
            dose: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            response: [10.5, 20.0, 31.2, 39.8, 51.0, 60.5]
        };

        let p = df |> plot(aes(col("dose"), col("response")))
                   |> geom_point()
                   |> geom_smooth()
                   |> labs("Dose-Response Fit");

        let h = [1.0, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0] |> hist();
        let b = [10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 100.0] |> boxplot();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let p_val = interp.env.get("p").expect("p exists");
    if let Value::Plot(spec) = p_val {
        assert_eq!(spec.layers.len(), 2);
        assert_eq!(spec.x_data.len(), 6);
        assert_eq!(spec.y_data.len(), 6);
        assert_eq!(spec.labels.title.as_deref(), Some("Dose-Response Fit"));
    } else {
        panic!("Expected Plot for p");
    }

    let h_val = interp.env.get("h").expect("h exists");
    assert!(matches!(h_val, Value::Plot(_)));

    let b_val = interp.env.get("b").expect("b exists");
    assert!(matches!(b_val, Value::Plot(_)));
}

#[test]
fn test_layered_io_and_wrangling_pipeline() {
    let temp_dir = std::env::temp_dir();
    let in_path = temp_dir.join("ghl_test_input.csv");
    let out_path = temp_dir.join("ghl_test_output.csv");
    let in_str = in_path.to_str().unwrap().replace('\\', "/");
    let out_str = out_path.to_str().unwrap().replace('\\', "/");

    let code = format!(
        r#"
        let raw_csv = "id,dose,response,batch\n1,1.0,10.0,A\n2,2.0,20.5,A\n3,3.0,30.2,B\n4,4.0,39.9,B\n5,5.0,50.1,C\n";
        raw_csv |> write_file("{in_str}");

        let df = read_csv("{in_str}");
        let subset = df |> select(["dose", "response"]) |> head(4);
        let model = fit(response ~ dose, subset);
        let tidy_df = tidy(model);

        tidy_df |> write_csv("{out_str}");
        let exists = file_exists("{out_str}");
        let lines = read_lines("{out_str}");
        "#
    );

    let program = parse(&code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let exists_val = interp.env.get("exists").expect("exists val");
    assert_eq!(exists_val, Value::Bool(true));

    let lines_val = interp.env.get("lines").expect("lines val");
    if let Value::Vector(lines) = lines_val {
        assert!(lines.len() >= 3); // Header + Intercept + dose
        assert!(lines[0].to_string().contains("term"));
    } else {
        panic!("Expected Vector for lines");
    }

    let _ = std::fs::remove_file(in_path);
    let _ = std::fs::remove_file(out_path);
}

#[test]
fn test_dataframe_tidyverse_pipeline() {
    // Full Tidyverse-style DataFrame pipeline:
    //   mutate -> arrange -> rename -> drop -> distinct -> nrow/ncol/colnames -> slice
    let code = r#"
        let df = dataframe {
            id:     [1, 2, 3, 4, 5, 2, 3],
            dose:   [1.0, 2.0, 3.0, 4.0, 5.0, 2.0, 3.0],
            group:  ["A", "B", "A", "B", "A", "B", "A"]
        };

        // mutate: add a derived column (dose squared)
        let dose_sq = [1.0, 4.0, 9.0, 16.0, 25.0, 4.0, 9.0];
        let df2 = df |> mutate("dose2", dose_sq);

        // arrange: sort by dose descending
        let df_sorted = df |> arrange(desc(dose));

        // rename: rename group -> cohort
        let df_renamed = df |> rename("group", "cohort");

        // drop: remove id column
        let df_dropped = df |> drop(["id"]);

        // distinct: remove duplicate rows (id 2 and 3 appear twice)
        let df_unique = df |> distinct();

        // nrow / ncol / colnames
        let n_rows     = df |> nrow();
        let n_cols     = df |> ncol();
        let col_names  = df |> colnames();

        // slice: rows 1..3 (0-based, exclusive end)
        let sliced = df |> slice(1, 4);
    "#;

    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    // mutate added a 4th column
    let df2 = interp.env.get("df2").expect("df2");
    assert_eq!(df_columns(&df2).len(), 4);
    assert!(df_columns(&df2).contains(&"dose2".to_string()));
    let d2 = df_column(&df2, "dose2");
    assert_eq!(d2[0], Value::F64(1.0));
    assert_eq!(d2[1], Value::F64(4.0));

    // arrange sorted descending: first dose should be 5.0
    let df_s = interp.env.get("df_sorted").expect("df_sorted");
    let doses = df_column(&df_s, "dose");
    assert_eq!(doses[0], Value::F64(5.0));
    assert_eq!(doses[1], Value::F64(4.0));

    // rename: cohort present, group absent
    let df_r = interp.env.get("df_renamed").expect("df_renamed");
    let renamed_cols = df_columns(&df_r);
    assert!(renamed_cols.contains(&"cohort".to_string()));
    assert!(!renamed_cols.contains(&"group".to_string()));

    // drop: id column removed
    let df_d = interp.env.get("df_dropped").expect("df_dropped");
    let dropped_cols = df_columns(&df_d);
    assert!(!dropped_cols.contains(&"id".to_string()));
    assert!(dropped_cols.contains(&"dose".to_string()));

    // distinct: 7 rows -> 5 unique (rows with id=2 and id=3 have exact duplicates)
    let df_u = interp.env.get("df_unique").expect("df_unique");
    assert_eq!(df_height(&df_u), 5, "Expected 5 distinct rows");

    // nrow = 7, ncol = 3
    assert_eq!(interp.env.get("n_rows"), Some(Value::I64(7)));
    assert_eq!(interp.env.get("n_cols"), Some(Value::I64(3)));

    // colnames = ["id", "dose", "group"]
    let col_names_val = interp.env.get("col_names").expect("col_names");
    if let Value::Vector(names) = col_names_val {
        assert_eq!(names.len(), 3);
        assert_eq!(names[0], Value::String("id".into()));
    } else {
        panic!("Expected col_names to be Vector");
    }

    // slice(1, 4) = rows 1,2,3 — second id should be 2
    let sliced_val = interp.env.get("sliced").expect("sliced");
    let ids = df_column(&sliced_val, "id");
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], Value::I64(2));
    assert_eq!(ids[2], Value::I64(4));
}

#[test]
fn test_group_by_summarize_unquoted_columns() {
    // Bare column names (no quotes, no `col()`) inside `summarize()`'s named args.
    let code = r#"
        let df = dataframe {
            species: ["a", "b", "a", "b", "a"],
            x:       [1.0, 2.0, 3.0, 4.0, 5.0]
        };

        let summary = df
            |> group_by(species)
            |> summarize(n = count(), mean_x = mean(x), max_x = max(x));
    "#;

    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let summary = interp.env.get("summary").expect("summary");
    assert_eq!(
        df_columns(&summary),
        vec![
            "species".to_string(),
            "n".to_string(),
            "mean_x".to_string(),
            "max_x".to_string()
        ]
    );
    let species = df_column(&summary, "species");
    let n = df_column(&summary, "n");
    let mean_x = df_column(&summary, "mean_x");
    assert_eq!(species.len(), 2);

    let a_idx = species
        .iter()
        .position(|v| v == &Value::String("a".into()))
        .unwrap();
    assert_eq!(n[a_idx], Value::I64(3));
    assert_eq!(mean_x[a_idx], Value::F64(3.0)); // (1+3+5)/3

    let b_idx = species
        .iter()
        .position(|v| v == &Value::String("b".into()))
        .unwrap();
    assert_eq!(n[b_idx], Value::I64(2));
    assert_eq!(mean_x[b_idx], Value::F64(3.0)); // (2+4)/2
}

#[test]
fn test_summarize_propagates_na_kleene_style() {
    // RFC 02 sect2.4: GHL never silently skips missing data in aggregates the way
    // pandas/numpy do. summarize()'s native polars reduce (mean_reduce/max_reduce/
    // etc.) skips nulls by default -- this locks in the explicit null_count() guard
    // in compute_agg (io.rs) that makes any NA in a group taint the whole result,
    // same as the standalone mean(vec)/max(vec) functions already do.
    let code = r#"
        let df = dataframe {
            g: ["a", "b", "a", "b", "a"],
            x: [1.0, 2.0, NA:SensorDropout, 4.0, 5.0]
        };
        let summary = df |> group_by(g) |> summarize(
            mean_x = mean(x), max_x = max(x), min_x = min(x),
            sum_x = sum(x), n_distinct_x = n_distinct(x)
        );
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let summary = interp.env.get("summary").expect("summary");
    let groups = df_column(&summary, "g");
    let mean_x = df_column(&summary, "mean_x");
    let max_x = df_column(&summary, "max_x");
    let sum_x = df_column(&summary, "sum_x");

    // Group "a" (values [1.0, NA:SensorDropout, 5.0]) must come back as plain NA for
    // every value-touching aggregate -- not skip the NA and average [1.0, 5.0] to 3.0.
    let a_idx = groups
        .iter()
        .position(|v| v == &Value::String("a".into()))
        .unwrap();
    assert_eq!(mean_x[a_idx], Value::NA(None));
    assert_eq!(max_x[a_idx], Value::NA(None));
    assert_eq!(sum_x[a_idx], Value::NA(None));

    // Group "b" (values [2.0, 4.0], no NA) must still compute normally.
    let b_idx = groups
        .iter()
        .position(|v| v == &Value::String("b".into()))
        .unwrap();
    assert_eq!(mean_x[b_idx], Value::F64(3.0));
    assert_eq!(max_x[b_idx], Value::F64(4.0));
    assert_eq!(sum_x[b_idx], Value::F64(6.0));

    // n_distinct() isn't a Kleene-propagating aggregate (matches the pre-optimization
    // behavior): the NA itself counts as one of the distinct values in the group.
    let n_distinct_x = df_column(&summary, "n_distinct_x");
    assert_eq!(n_distinct_x[a_idx], Value::I64(3));
    assert_eq!(n_distinct_x[b_idx], Value::I64(2));
}

#[test]
fn test_arrange_multi_column_with_desc() {
    let code = r#"
        let df = dataframe {
            group: ["b", "a", "a", "b"],
            x:     [2.0, 1.0, 2.0, 1.0]
        };

        // Ascending by group, then descending by x within each group.
        let sorted = df |> arrange(group, desc(x));
    "#;

    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let sorted = interp.env.get("sorted").expect("sorted");
    let groups = df_column(&sorted, "group");
    let xs = df_column(&sorted, "x");
    assert_eq!(
        groups,
        vec![
            Value::String("a".into()),
            Value::String("a".into()),
            Value::String("b".into()),
            Value::String("b".into()),
        ]
    );
    assert_eq!(
        xs,
        vec![
            Value::F64(2.0),
            Value::F64(1.0),
            Value::F64(2.0),
            Value::F64(1.0)
        ]
    );
}

#[test]
fn test_slice_min_max_and_sample_n() {
    let code = r#"
        let df = dataframe {
            id: [1, 2, 3, 4, 5],
            x:  [30.0, 10.0, 50.0, 20.0, 40.0]
        };

        let smallest = df |> slice_min(x, 2);
        let largest  = df |> slice_max(x, 2);
        let sampled  = df |> sample_n(3);
    "#;

    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    if let Some(smallest) = interp.env.get("smallest") {
        assert_eq!(
            df_column(&smallest, "x"),
            vec![Value::F64(10.0), Value::F64(20.0)]
        );
    } else {
        panic!("Expected smallest to be DataFrame");
    }

    if let Some(largest) = interp.env.get("largest") {
        assert_eq!(
            df_column(&largest, "x"),
            vec![Value::F64(50.0), Value::F64(40.0)]
        );
    } else {
        panic!("Expected largest to be DataFrame");
    }

    if let Some(sampled) = interp.env.get("sampled") {
        assert_eq!(df_column(&sampled, "x").len(), 3);
    } else {
        panic!("Expected sampled to be DataFrame");
    }
}

#[test]
fn test_inner_join_and_left_join() {
    let code = r#"
        let orders = dataframe {
            order_id: [1, 2, 3, 4],
            customer_id: [10, 20, 10, 30]
        };
        let customers = dataframe {
            customer_id: [10, 20],
            name: ["Alice", "Bob"]
        };

        let inner = orders |> inner_join(customers, customer_id);
        let left = orders |> left_join(customers, customer_id);
    "#;

    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let inner = interp.env.get("inner").expect("inner exists");
    assert_eq!(
        df_height(&inner),
        3,
        "unmatched customer_id=30 row should be dropped"
    );
    assert!(df_columns(&inner).contains(&"name".to_string()));

    let left = interp.env.get("left").expect("left exists");
    assert_eq!(
        df_height(&left),
        4,
        "every `orders` row should survive a left join"
    );
    let names = df_column(&left, "name");
    let order_ids = df_column(&left, "order_id");
    let unmatched_idx = order_ids.iter().position(|v| v == &Value::I64(4)).unwrap();
    assert_eq!(
        names[unmatched_idx],
        Value::NA(None),
        "unmatched right side should be NA"
    );
}

#[test]
fn test_join_preserves_na_reasons_on_both_sides() {
    // TODO.md Fase 1: na_reasons used to be dropped entirely by inner_join/left_join
    // (the eager join API doesn't expose per-output-row provenance on its own) --
    // df_join now carries a row-index column through the join on each side to recover
    // it. `score` collides between `left` and `right` on purpose, to also cover the
    // rename-on-collision path (`score` -> `score_right`).
    let code = r#"
        let left = dataframe {
            id:    [1, 2, 3],
            score: [10.0, NA:SensorDropout, 30.0]
        };
        let right = dataframe {
            id:    [1, 2],
            score: [NA:Timeout, 200.0]
        };

        let inner = left |> inner_join(right, id);
        let inner_left_reasons = inner |> na_reasons(score);
        let inner_right_reasons = inner |> na_reasons(score_right);

        let outer = left |> left_join(right, id);
        let outer_left_reasons = outer |> na_reasons(score);
        let outer_right_reasons = outer |> na_reasons(score_right);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let na = Value::NA(None);
    let vec_of = |interp: &Interpreter, name: &str| match interp.env.get(name).expect(name) {
        Value::Vector(vals) => vals.iter().cloned().collect::<Vec<Value>>(),
        other => panic!("expected Vector for `{name}`, got {other:?}"),
    };

    // Inner join: id=3 (left-only) is dropped, id=1/2 both matched.
    assert_eq!(
        vec_of(&interp, "inner_left_reasons"),
        vec![na.clone(), Value::String("SensorDropout".into())]
    );
    assert_eq!(
        vec_of(&interp, "inner_right_reasons"),
        vec![Value::String("Timeout".into()), na.clone()]
    );

    // Left join: every left row survives, including the unmatched id=3 -- its
    // `score_right` is a real (reason-less) NA, not a fabricated reason from a right
    // row that never existed.
    assert_eq!(
        vec_of(&interp, "outer_left_reasons"),
        vec![
            na.clone(),
            Value::String("SensorDropout".into()),
            na.clone()
        ]
    );
    assert_eq!(
        vec_of(&interp, "outer_right_reasons"),
        vec![Value::String("Timeout".into()), na.clone(), na.clone()]
    );
}

#[test]
fn test_parquet_round_trip() {
    let path = std::env::temp_dir().join("ghl_test_round_trip.parquet");
    let path_str = path.to_str().unwrap();

    let code = format!(
        r#"
        let df = dataframe {{
            id: [1, 2, 3],
            score: [10.5, 20.5, NA],
            label: ["a", "b", "c"]
        }};
        df |> write_parquet("{path}");
        let roundtripped = read_parquet("{path}");
        "#,
        path = path_str
    );

    let program = parse(&code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    let _ = std::fs::remove_file(&path);

    let roundtripped = interp.env.get("roundtripped").expect("roundtripped exists");
    assert_eq!(df_height(&roundtripped), 3);
    assert_eq!(
        df_column(&roundtripped, "id"),
        vec![Value::I64(1), Value::I64(2), Value::I64(3)]
    );
    assert_eq!(
        df_column(&roundtripped, "score"),
        vec![Value::F64(10.5), Value::F64(20.5), Value::NA(None)],
        "plain NA (validity bit) must survive Parquet even though a *reason* can't"
    );
    assert_eq!(
        df_column(&roundtripped, "label"),
        vec![
            Value::String("a".into()),
            Value::String("b".into()),
            Value::String("c".into())
        ]
    );
}

#[test]
fn test_na_reason_accessors_survive_filter() {
    // RFC 02 sect2.5: reasons exist so missing-data-analysis code can consume them
    // via ordinary GHL values, not just internally.
    let code = r#"
        let df = dataframe {
            keep:  [1, 1, 0, 1],
            score: [10.0, NA:SensorDropout, 30.0, NA:LowBattery]
        };

        let plain_reason = na_reason(NA);
        let noted_reason = na_reason(NA:NoResponse);

        let reasons_before = df |> na_reasons(score);
        let filtered = df |> filter(keep > 0);
        let reasons_after = filtered |> na_reasons(score);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("plain_reason"), Some(Value::NA(None)));
    assert_eq!(
        interp.env.get("noted_reason"),
        Some(Value::String("NoResponse".into()))
    );

    let before = interp.env.get("reasons_before").expect("reasons_before");
    if let Value::Vector(vals) = before {
        assert_eq!(
            *vals,
            vec![
                Value::NA(None),
                Value::String("SensorDropout".into()),
                Value::NA(None),
                Value::String("LowBattery".into())
            ]
        );
    } else {
        panic!("Expected Vector for na_reasons()");
    }

    // Row index 2 (score=30.0, no reason) is the one dropped by `filter(keep > 0)`;
    // the LowBattery reason at the old row 3 must reindex to the new row 2.
    let after = interp.env.get("reasons_after").expect("reasons_after");
    if let Value::Vector(vals) = after {
        assert_eq!(
            *vals,
            vec![
                Value::NA(None),
                Value::String("SensorDropout".into()),
                Value::String("LowBattery".into())
            ]
        );
    } else {
        panic!("Expected Vector for na_reasons()");
    }
}

#[test]
fn test_is_na() {
    let code = r#"
        let scalar_true = is_na(NA);
        let scalar_false = is_na(5.0);
        let vectorized = is_na([1.0, NA, NA:SensorDropout, 4.0]);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("scalar_true"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("scalar_false"), Some(Value::Bool(false)));
    assert_eq!(
        interp.env.get("vectorized"),
        Some(Value::Vector(
            ghl_runtime::vector_data::VectorData::from_values(vec![
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(true),
                Value::Bool(false)
            ])
        ))
    );
}

#[test]
fn test_math_and_string_helpers() {
    let code = r#"
        let rounded = round(3.14159, 2);
        let clamped = clamp(15.0, 0.0, 10.0);
        let root = sqrt(-1.0);
        let shouted = str_upper("hello");
        let padded = str_pad("42", 5, "0");
        let contains = str_contains("hello world", "world");
    "#;

    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("rounded"), Some(Value::F64(3.14)));
    assert_eq!(interp.env.get("clamped"), Some(Value::F64(10.0)));
    assert!(matches!(interp.env.get("root"), Some(Value::NA(_))));
    assert_eq!(
        interp.env.get("shouted"),
        Some(Value::String("HELLO".into()))
    );
    assert_eq!(
        interp.env.get("padded"),
        Some(Value::String("00042".into()))
    );
    assert_eq!(interp.env.get("contains"), Some(Value::Bool(true)));
}

#[test]
fn test_scalar_comparisons_parity_unchanged() {
    let code = r#"
        let eq1 = 5 == 5;
        let eq2 = 5 == 6;
        let ne1 = 5 != 6;
        let lt1 = 3.0 < 4.0;
        let lt2 = 4.0 < 3.0;
        let lte1 = 3.0 <= 3.0;
        let gt1 = 5.0 > 2.0;
        let gte1 = 5.0 >= 5.0;
        let str_eq = "hello" == "hello";
        let str_ne = "hello" != "world";
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("eq1"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("eq2"), Some(Value::Bool(false)));
    assert_eq!(interp.env.get("ne1"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("lt1"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("lt2"), Some(Value::Bool(false)));
    assert_eq!(interp.env.get("lte1"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("gt1"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("gte1"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("str_eq"), Some(Value::Bool(true)));
    assert_eq!(interp.env.get("str_ne"), Some(Value::Bool(true)));
}

#[test]
fn test_vector_comparisons_elementwise() {
    let code = r#"
        let v1 = [1.0, 2.0, 3.0];
        let v2 = [2.0, 2.0, 1.0];
        let eq_vec = v1 == v2;
        let eq_scalar = v1 == 2.0;
        let scalar_eq = 2.0 == v1;
        let lt_vec = v1 < v2;
        let gt_scalar = v1 > 1.5;
        let scalar_lt = 2.0 < v1;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    fn bools_of(val: Option<Value>) -> Vec<bool> {
        match val.unwrap() {
            Value::Vector(vd) => vd.iter().map(|v| v.as_bool().unwrap()).collect(),
            other => panic!("expected Vector, got {:?}", other),
        }
    }

    assert_eq!(bools_of(interp.env.get("eq_vec")), vec![false, true, false]);
    assert_eq!(
        bools_of(interp.env.get("eq_scalar")),
        vec![false, true, false]
    );
    assert_eq!(
        bools_of(interp.env.get("scalar_eq")),
        vec![false, true, false]
    );
    assert_eq!(bools_of(interp.env.get("lt_vec")), vec![true, false, false]);
    assert_eq!(
        bools_of(interp.env.get("gt_scalar")),
        vec![false, true, true]
    );
    assert_eq!(
        bools_of(interp.env.get("scalar_lt")),
        vec![false, false, true]
    );
}

#[test]
fn test_dataframe_filter_parity_unchanged() {
    let code = r#"
        let df = dataframe { id: [1, 2, 3, 4], score: [70.0, 85.0, 90.0, 60.0] };
        let filtered = filter(df, score > 75.0);
        let n = nrow(filtered);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("n"), Some(Value::I64(2)));
}

#[test]
fn test_vector_filter_by_boolean_mask() {
    let code = r#"
        let y = [10.0, 20.0, 30.0, 40.0];
        let groups = [0, 1, 0, 1];
        let y_0 = filter(y, groups == 0);
        let y_1 = filter(y, groups == 1);
        let s0 = sum(y_0);
        let s1 = sum(y_1);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("s0"), Some(Value::F64(40.0)));
    assert_eq!(interp.env.get("s1"), Some(Value::F64(60.0)));
}
