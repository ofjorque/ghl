//! High-performance runtime engine for GHL (Generalized Hypothesis Language).
//!
//! Provides memory evaluation, Kleene three-valued logic, linear algebra operators,
//! and native statistical functions.

pub mod value;
pub mod matrix;
pub mod env;
pub mod eval;
pub mod neko;
pub mod io;

pub use value::Value;
pub use env::RuntimeEnv;
pub use eval::Interpreter;
pub use neko::{Blueprint, FittedModel, RowDisposition, VcovKind};

use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::Program;

/// Evaluates a GHL program and returns the final expression value.
pub fn eval(program: &Program) -> Result<Value, Diagnostic> {
    let mut interpreter = Interpreter::new();
    interpreter.eval_program(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghl_syntax::parser::parse;

    #[test]
    fn test_eval_arithmetic_and_kleene_na() {
        let code = r#"
            let a = 10 + 20;
            let with_na = 10 + NA:SensorDropout;
            let kleene_and_false = false && NA:NoResponse;
            let kleene_and_true = true && NA:NoResponse;
            let kleene_or_true = true || NA:SensorDropout;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        assert_eq!(interp.env.get("a"), Some(Value::I64(30)));
        assert_eq!(
            interp.env.get("with_na"),
            Some(Value::NA(Some("SensorDropout".into())))
        );
        // Kleene: false && NA is false
        assert_eq!(interp.env.get("kleene_and_false"), Some(Value::Bool(false)));
        // Kleene: true && NA is NA
        assert_eq!(
            interp.env.get("kleene_and_true"),
            Some(Value::NA(Some("NoResponse".into())))
        );
        // Kleene: true || NA is true
        assert_eq!(interp.env.get("kleene_or_true"), Some(Value::Bool(true)));
    }

    #[test]
    fn test_eval_matrix_solve_gaussian() {
        // Linear system:
        //  1*x + 2*y = 5
        //  3*x + 4*y = 11
        // Solution: x = 1.0, y = 2.0
        let code = r#"
            let A = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
            let b = [5.0, 11.0];
            let x = A \ b;
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let sol = interp.env.get("x").expect("x must be computed");
        match sol {
            Value::Vector(v) => {
                assert_eq!(v.len(), 2);
                let x_val = v[0].as_f64().unwrap();
                let y_val = v[1].as_f64().unwrap();
                assert!((x_val - 1.0).abs() < 1e-6);
                assert!((y_val - 2.0).abs() < 1e-6);
            }
            _ => panic!("Expected vector solution"),
        }
    }

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
        if let Value::DataFrame { columns: _, data } = filtered_val {
            let ids = data.get("id").expect("id column exists");
            assert_eq!(ids.len(), 3);
            assert_eq!(ids[0], Value::I64(2));
            assert_eq!(ids[1], Value::I64(3));
            assert_eq!(ids[2], Value::I64(4));
        } else {
            panic!("Expected DataFrame result");
        }
    }

    #[test]
    fn test_neko_ols_fit_and_projections() {
        // True model: y = 5.0 + 2.0*x1 - 1.0*x2
        // Observations:
        // x1 = [1, 2, 3, 4, 5, 6]
        // x2 = [2, 1, 4, 3, 6, 5]
        // y  = [5 + 2*1 - 2 = 5,
        //       5 + 2*2 - 1 = 8,
        //       5 + 2*3 - 4 = 7,
        //       5 + 2*4 - 3 = 10,
        //       5 + 2*5 - 6 = 9,
        //       5 + 2*6 - 5 = 12]
        let code = r#"
            let df = dataframe {
                x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                x2: [2.0, 1.0, 4.0, 3.0, 6.0, 5.0],
                y:  [5.0, 8.0, 7.0, 10.0, 9.0, 12.0]
            };

            let model = fit(y ~ x1 + x2, df);
            let tidied = tidy(model);
            let glanced = glance(model);
            let coefficients = coef(model);
            let pred = predict(model, df);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        // Verify coefficients
        let coef_val = interp.env.get("coefficients").expect("coef exists");
        if let Value::Vector(coefs) = coef_val {
            assert_eq!(coefs.len(), 3);
            let b0 = coefs[0].as_f64().unwrap();
            let b1 = coefs[1].as_f64().unwrap();
            let b2 = coefs[2].as_f64().unwrap();
            assert!((b0 - 5.0).abs() < 1e-6, "b0 should be 5.0, got {}", b0);
            assert!((b1 - 2.0).abs() < 1e-6, "b1 should be 2.0, got {}", b1);
            assert!((b2 - -1.0).abs() < 1e-6, "b2 should be -1.0, got {}", b2);
        } else {
            panic!("Expected Vector for coef");
        }

        // Verify tidy table
        let tidy_val = interp.env.get("tidied").expect("tidy exists");
        if let Value::DataFrame { columns, data } = tidy_val {
            assert_eq!(columns, vec!["term", "estimate", "std_error", "statistic", "p_value"]);
            let terms = data.get("term").unwrap();
            assert_eq!(terms.len(), 3);
            assert_eq!(terms[0], Value::String("(Intercept)".into()));
            assert_eq!(terms[1], Value::String("x1".into()));
            assert_eq!(terms[2], Value::String("x2".into()));
        } else {
            panic!("Expected DataFrame for tidy");
        }

        // Verify glance table
        let glance_val = interp.env.get("glanced").expect("glance exists");
        if let Value::DataFrame { columns, data } = glance_val {
            assert!(columns.contains(&"r_squared".to_string()));
            let r2 = data.get("r_squared").unwrap()[0].as_f64().unwrap();
            assert!((r2 - 1.0).abs() < 1e-6, "R2 must be 1.0 for perfect linear fit");
        } else {
            panic!("Expected DataFrame for glance");
        }

        // Verify predictions
        let pred_val = interp.env.get("pred").expect("pred exists");
        if let Value::Vector(preds) = pred_val {
            assert_eq!(preds.len(), 6);
            assert!((preds[0].as_f64().unwrap() - 5.0).abs() < 1e-6);
            assert!((preds[1].as_f64().unwrap() - 8.0).abs() < 1e-6);
        } else {
            panic!("Expected Vector for predict");
        }
    }

    #[test]
    fn test_neko_na_disposition_in_augment() {
        let code = r#"
            let df = dataframe {
                x: [1.0, 2.0, NA:SensorDropout, 4.0, 5.0, 6.0],
                y: [2.0, 4.0, 6.0, 8.0, 10.0, 12.0]
            };

            let model = fit(y ~ x, df);
            let eval_df = augment(model, df);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let eval_val = interp.env.get("eval_df").expect("eval_df exists");
        if let Value::DataFrame { columns, data } = eval_val {
            assert!(columns.contains(&".fitted".to_string()));
            assert!(columns.contains(&".used_in_fit".to_string()));
            assert!(columns.contains(&".na_reason".to_string()));

            let used = data.get(".used_in_fit").unwrap();
            let reasons = data.get(".na_reason").unwrap();

            // Row 0 (valid)
            assert_eq!(used[0], Value::Bool(true));
            assert_eq!(reasons[0], Value::String("none".into()));

            // Row 2 (NA:SensorDropout)
            assert_eq!(used[2], Value::Bool(false));
            assert_eq!(reasons[2], Value::String("SensorDropout".into()));
        } else {
            panic!("Expected DataFrame for augment");
        }
    }

    #[test]
    fn test_neko_singular_matrix_emits_s0101() {
        // Collinear predictors: x2 = 2 * x1
        let code = r#"
            let df = dataframe {
                x1: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                x2: [2.0, 4.0, 6.0, 8.0, 10.0, 12.0],
                y:  [3.0, 5.0, 7.0, 9.0, 11.0, 13.0]
            };

            let model = fit(y ~ x1 + x2, df);
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        let res = interp.eval_program(&program);

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code, "S0101");
    }

    #[test]
    fn test_neko_vcov_hc3() {
        let code = r#"
            let df = dataframe {
                x: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
                y: [2.1, 3.9, 6.2, 7.8, 10.1, 12.0]
            };

            let model = fit(y ~ x, df);
            let v_hc3 = vcov(model, "HC3");
        "#;
        let program = parse(code).expect("syntax ok");
        let mut interp = Interpreter::new();
        interp.eval_program(&program).expect("evaluation ok");

        let vcov_val = interp.env.get("v_hc3").expect("v_hc3 exists");
        if let Value::Matrix { rows, cols, data } = vcov_val {
            assert_eq!(rows, 2);
            assert_eq!(cols, 2);
            assert!(data[0] > 0.0); // Var(Intercept) > 0
            assert!(data[3] > 0.0); // Var(x) > 0
        } else {
            panic!("Expected Matrix for vcov");
        }
    }

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
        if let Value::DataFrame { columns, data } = &df2 {
            assert_eq!(columns.len(), 4);
            assert!(columns.contains(&"dose2".to_string()));
            let d2 = data.get("dose2").unwrap();
            assert_eq!(d2[0], Value::F64(1.0));
            assert_eq!(d2[1], Value::F64(4.0));
        } else { panic!("Expected df2 to be DataFrame"); }

        // arrange sorted descending: first dose should be 5.0
        let df_s = interp.env.get("df_sorted").expect("df_sorted");
        if let Value::DataFrame { data, .. } = &df_s {
            let doses = data.get("dose").unwrap();
            assert_eq!(doses[0], Value::F64(5.0));
            assert_eq!(doses[1], Value::F64(4.0));
        } else { panic!("Expected df_sorted to be DataFrame"); }

        // rename: cohort present, group absent
        let df_r = interp.env.get("df_renamed").expect("df_renamed");
        if let Value::DataFrame { columns, data } = &df_r {
            assert!(columns.contains(&"cohort".to_string()));
            assert!(!columns.contains(&"group".to_string()));
            assert!(data.contains_key("cohort"));
        } else { panic!("Expected df_renamed to be DataFrame"); }

        // drop: id column removed
        let df_d = interp.env.get("df_dropped").expect("df_dropped");
        if let Value::DataFrame { columns, .. } = &df_d {
            assert!(!columns.contains(&"id".to_string()));
            assert!(columns.contains(&"dose".to_string()));
        } else { panic!("Expected df_dropped to be DataFrame"); }

        // distinct: 7 rows -> 5 unique (rows with id=2 and id=3 have exact duplicates)
        let df_u = interp.env.get("df_unique").expect("df_unique");
        if let Value::DataFrame { columns, data } = &df_u {
            let n = data.get(&columns[0]).map(|v| v.len()).unwrap_or(0);
            assert_eq!(n, 5, "Expected 5 distinct rows, got {n}");
        } else { panic!("Expected df_unique to be DataFrame"); }

        // nrow = 7, ncol = 3
        assert_eq!(interp.env.get("n_rows"), Some(Value::I64(7)));
        assert_eq!(interp.env.get("n_cols"), Some(Value::I64(3)));

        // colnames = ["id", "dose", "group"]
        let col_names_val = interp.env.get("col_names").expect("col_names");
        if let Value::Vector(names) = col_names_val {
            assert_eq!(names.len(), 3);
            assert_eq!(names[0], Value::String("id".into()));
        } else { panic!("Expected col_names to be Vector"); }

        // slice(1, 4) = rows 1,2,3 — second id should be 2
        let sliced_val = interp.env.get("sliced").expect("sliced");
        if let Value::DataFrame { data, .. } = &sliced_val {
            let ids = data.get("id").unwrap();
            assert_eq!(ids.len(), 3);
            assert_eq!(ids[0], Value::I64(2));
            assert_eq!(ids[2], Value::I64(4));
        } else { panic!("Expected sliced to be DataFrame"); }
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
        if let Value::DataFrame { columns, data } = &summary {
            assert_eq!(columns, &vec!["species".to_string(), "n".to_string(), "mean_x".to_string(), "max_x".to_string()]);
            let species = data.get("species").unwrap();
            let n = data.get("n").unwrap();
            let mean_x = data.get("mean_x").unwrap();
            assert_eq!(species.len(), 2);

            let a_idx = species.iter().position(|v| v == &Value::String("a".into())).unwrap();
            assert_eq!(n[a_idx], Value::I64(3));
            assert_eq!(mean_x[a_idx], Value::F64(3.0)); // (1+3+5)/3

            let b_idx = species.iter().position(|v| v == &Value::String("b".into())).unwrap();
            assert_eq!(n[b_idx], Value::I64(2));
            assert_eq!(mean_x[b_idx], Value::F64(3.0)); // (2+4)/2
        } else {
            panic!("Expected summary to be DataFrame");
        }
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
        if let Value::DataFrame { data, .. } = &sorted {
            let groups = data.get("group").unwrap();
            let xs = data.get("x").unwrap();
            assert_eq!(groups.clone(), vec![
                Value::String("a".into()), Value::String("a".into()),
                Value::String("b".into()), Value::String("b".into()),
            ]);
            assert_eq!(xs.clone(), vec![Value::F64(2.0), Value::F64(1.0), Value::F64(2.0), Value::F64(1.0)]);
        } else {
            panic!("Expected sorted to be DataFrame");
        }
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

        if let Some(Value::DataFrame { data, .. }) = interp.env.get("smallest") {
            let xs = data.get("x").unwrap();
            assert_eq!(xs, &vec![Value::F64(10.0), Value::F64(20.0)]);
        } else {
            panic!("Expected smallest to be DataFrame");
        }

        if let Some(Value::DataFrame { data, .. }) = interp.env.get("largest") {
            let xs = data.get("x").unwrap();
            assert_eq!(xs, &vec![Value::F64(50.0), Value::F64(40.0)]);
        } else {
            panic!("Expected largest to be DataFrame");
        }

        if let Some(Value::DataFrame { data, .. }) = interp.env.get("sampled") {
            assert_eq!(data.get("x").unwrap().len(), 3);
        } else {
            panic!("Expected sampled to be DataFrame");
        }
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
        assert_eq!(interp.env.get("shouted"), Some(Value::String("HELLO".into())));
        assert_eq!(interp.env.get("padded"), Some(Value::String("00042".into())));
        assert_eq!(interp.env.get("contains"), Some(Value::Bool(true)));
    }
}

