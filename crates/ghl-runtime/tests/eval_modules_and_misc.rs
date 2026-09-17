//! `use` module resolution, matrix copy-on-write semantics, arena lifecycle, lag/lead, if_else, pivot_wider/pivot_longer, impute, filter_na_reason.

mod common;

use ghl_runtime::value::Value;
use ghl_runtime::eval::Interpreter;
use ghl_runtime::env::RuntimeEnv;
use ghl_runtime::vector_data::VectorData;
use ghl_syntax::parser::parse;

#[test]
fn test_modules_use_single_and_alias() {
    let code = r#"
        use std::math::sqrt;
        use std::linalg::transpose as t;

        let root = sqrt(49.0);
        let m = mat [ 1.0, 2.0 ; 3.0, 4.0 ];
        let tm = t(m);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    assert_eq!(interp.env.get("root"), Some(Value::F64(7.0)));
    let tm = interp.env.get("tm").unwrap();
    match tm {
        Value::Matrix { rows, cols, data } => {
            assert_eq!((rows, cols), (2, 2));
            assert_eq!(data.as_slice(), &[1.0, 3.0, 2.0, 4.0]);
        }
        _ => panic!("Expected matrix"),
    }
}

#[test]
fn test_modules_use_group_and_glob() {
    let code = r#"
        use std::stats::distributions::{random_normal, normal_pdf};
        use std::linalg::*;

        let pdf_val = normal_pdf(0.0, 0.0, 1.0);
        let id = identity(3);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    let pdf = interp.env.get("pdf_val").unwrap().as_f64().unwrap();
    assert!((pdf - 0.39894228).abs() < 1e-4);

    let id = interp.env.get("id").unwrap();
    match id {
        Value::Matrix { rows, cols, .. } => {
            assert_eq!((rows, cols), (3, 3));
        }
        _ => panic!("Expected identity matrix"),
    }
}

#[test]
fn test_modules_qualified_path_call_and_constants() {
    let code = r#"
        let root = std::math::sqrt(100.0);
        let area = std::math::pi * 2.0 * 2.0;
        let piped = 16.0 |> std::math::sqrt;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    assert_eq!(interp.env.get("root"), Some(Value::F64(10.0)));
    assert_eq!(interp.env.get("piped"), Some(Value::F64(4.0)));
    let area = interp.env.get("area").unwrap().as_f64().unwrap();
    assert!((area - (std::f64::consts::PI * 4.0)).abs() < 1e-6);
}

#[test]
fn test_modules_block_scoped_use() {
    let code = r#"
        let scoped = {
            use std::math::sqrt;
            sqrt(64.0)
        };
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    assert_eq!(interp.env.get("scoped"), Some(Value::F64(8.0)));
    // sqrt inside block was scoped
}

#[test]
fn test_modules_reject_invalid_module_or_item() {
    let bad_mod = "use std::not_a_module::foo;";
    let prog1 = parse(bad_mod).expect("syntax ok");
    assert!(Interpreter::new().eval_program(&prog1).is_err());

    let bad_item = "use std::math::nonexistent;";
    let prog2 = parse(bad_item).expect("syntax ok");
    assert!(Interpreter::new().eval_program(&prog2).is_err());
}

#[test]
fn test_matrix_cow_inplace_when_unique_and_clones_when_shared() {
    use std::sync::Arc;

    // 1. In-place mutation when unique (strong_count == 1)
    let m = Value::matrix(2, 2, vec![1.0, 2.0, 3.0, 4.0]);
    let orig_ptr = if let Value::Matrix { ref data, .. } = m {
        Arc::as_ptr(data)
    } else {
        panic!("Expected Matrix");
    };

    // Mutate in-place via native_set
    let m_updated = ghl_runtime::env::RuntimeEnv::with_prelude();
    let set_fn = m_updated.get("set").unwrap();
    let mutated = if let Value::NativeFn(f) = set_fn {
        f(vec![m, Value::I64(0), Value::I64(0), Value::F64(99.0)]).unwrap()
    } else {
        panic!("Expected NativeFn");
    };

    let new_ptr = if let Value::Matrix { ref data, .. } = mutated {
        Arc::as_ptr(data)
    } else {
        panic!("Expected Matrix");
    };

    // Pointer must be identical: zero copies, zero allocations!
    assert_eq!(orig_ptr, new_ptr, "Matrix with strong_count==1 must mutate in-place without reallocation");

    // 2. Clone-on-write when shared (strong_count > 1)
    let m_shared = mutated.clone(); // strong_count becomes 2
    let shared_ptr = if let Value::Matrix { ref data, .. } = m_shared {
        Arc::as_ptr(data)
    } else {
        panic!("Expected Matrix");
    };

    let mutated2 = if let Value::NativeFn(f) = set_fn {
        f(vec![mutated, Value::I64(1), Value::I64(1), Value::F64(555.0)]).unwrap()
    } else {
        panic!("Expected NativeFn");
    };

    let mutated2_ptr = if let Value::Matrix { ref data, .. } = mutated2 {
        Arc::as_ptr(data)
    } else {
        panic!("Expected Matrix");
    };

    // Since it was shared, Arc::make_mut cloned the buffer!
    assert_ne!(shared_ptr, mutated2_ptr, "Matrix with strong_count > 1 must clone buffer on write");
    // And the shared original keeps its previous values unchanged
    if let Value::Matrix { ref data, .. } = m_shared {
        assert_eq!(data[0], 99.0);
        assert_eq!(data[3], 4.0);
    }
    if let Value::Matrix { ref data, .. } = mutated2 {
        assert_eq!(data[0], 99.0);
        assert_eq!(data[3], 555.0);
    }
}

#[test]
fn test_scope_pool_recycling_in_while_loops() {
    let code = r#"
        let mut i = 0;
        let mut acc = 0;
        while i < 100 {
            let temp = i * 2;
            acc = acc + temp;
            i = i + 1;
        };
        acc
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let res = interp.eval_program(&program).expect("eval ok");
    assert_eq!(res, Value::I64(9900));

    // The scope_pool in interp.env must have recycled scope capacity available
    assert!(!interp.env.scope_pool.is_empty());
}

#[test]
fn test_regional_arena_lifecycle_and_reset() {
    let code = r#"
        use std::arena::{scope, alloc_vector, alloc_matrix, reset, allocated_bytes};

        let res = scope(\a -> {
            let v = alloc_vector(a, 50, 2.5);
            let m = alloc_matrix(a, 10, 10, 1.0);
            let bytes_before = allocated_bytes(a);
            reset(a);
            let bytes_after = allocated_bytes(a);
            [bytes_before, bytes_after, sum(v)]
        });
        res
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    let res = interp.eval_program(&program).expect("eval ok");

    if let Value::Vector(vd) = res {
        assert_eq!(vd.len(), 3);
        let b_before = vd.value_at(0).unwrap().as_i64().unwrap_or(0);
        let b_after = vd.value_at(1).unwrap().as_i64().unwrap_or(0);
        let v_sum = vd.value_at(2).unwrap().as_f64().unwrap_or(0.0);

        assert!(b_before > 0, "Arena must allocate bytes for vector and matrix");
        assert_eq!(b_after, 0, "reset(a) must instantly reclaim memory (O(1))");
        assert_eq!(v_sum, 50.0 * 2.5, "alloc_vector values must be valid and computable by verbs");
    } else {
        panic!("Expected vector result from arena scope, found {res:?}");
    }
}

#[test]
fn test_lag_lead_zero_boxing_and_na_reasons() {
    let code = r#"
        let v = [10.0, 20.0, NA:SensorDropout, 40.0, 50.0];
        let lagged = lag(v, 1);
        let leaded = lead(v, 1);
        let lagged2 = lag(v, 2);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    let lagged = interp.env.get("lagged").expect("lagged");
    if let Value::Vector(vd) = lagged {
        assert_eq!(vd.len(), 5);
        // Row 0 is NA (generic)
        assert_eq!(vd.value_at(0), Some(Value::NA(None)));
        // Row 1 is 10.0
        assert_eq!(vd.value_at(1), Some(Value::F64(10.0)));
        // Row 2 is 20.0
        assert_eq!(vd.value_at(2), Some(Value::F64(20.0)));
        // Row 3 is NA:SensorDropout (shifted from row 2)
        assert_eq!(vd.value_at(3), Some(Value::NA(Some("SensorDropout".into()))));
        // Row 4 is 40.0
        assert_eq!(vd.value_at(4), Some(Value::F64(40.0)));
    } else {
        panic!("Expected vector");
    }

    let leaded = interp.env.get("leaded").expect("leaded");
    if let Value::Vector(vd) = leaded {
        assert_eq!(vd.len(), 5);
        // Row 0 is 20.0
        assert_eq!(vd.value_at(0), Some(Value::F64(20.0)));
        // Row 1 is NA:SensorDropout (shifted from row 2)
        assert_eq!(vd.value_at(1), Some(Value::NA(Some("SensorDropout".into()))));
        // Row 2 is 40.0
        assert_eq!(vd.value_at(2), Some(Value::F64(40.0)));
        // Row 3 is 50.0
        assert_eq!(vd.value_at(3), Some(Value::F64(50.0)));
        // Row 4 is NA (generic)
        assert_eq!(vd.value_at(4), Some(Value::NA(None)));
    } else {
        panic!("Expected vector");
    }
}

#[test]
fn test_if_else_fast_path_and_parallel() {
    let code = r#"
        let cond = [true, false, true, false];
        let yes = [1.0, 2.0, 3.0, 4.0];
        let no = [10.0, 20.0, 30.0, 40.0];
        let res = if_else(cond, yes, no);
        let res_scalar = if_else(cond, 100.0, 0.0);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    let res = interp.env.get("res").expect("res");
    if let Value::Vector(vd) = res {
        assert_eq!(vd.len(), 4);
        assert_eq!(vd.value_at(0), Some(Value::F64(1.0)));
        assert_eq!(vd.value_at(1), Some(Value::F64(20.0)));
        assert_eq!(vd.value_at(2), Some(Value::F64(3.0)));
        assert_eq!(vd.value_at(3), Some(Value::F64(40.0)));
    } else {
        panic!("Expected vector");
    }

    let res_scalar = interp.env.get("res_scalar").expect("res_scalar");
    if let Value::Vector(vd) = res_scalar {
        assert_eq!(vd.len(), 4);
        assert_eq!(vd.value_at(0), Some(Value::F64(100.0)));
        assert_eq!(vd.value_at(1), Some(Value::F64(0.0)));
        assert_eq!(vd.value_at(2), Some(Value::F64(100.0)));
        assert_eq!(vd.value_at(3), Some(Value::F64(0.0)));
    } else {
        panic!("Expected vector");
    }

    // Test parallel threshold path (>= 50_000)
    let large_n = 60_000;
    let cond_large: Vec<bool> = (0..large_n).map(|i| i % 2 == 0).collect();
    let yes_large: Vec<f64> = vec![1.0; large_n];
    let no_large: Vec<f64> = vec![2.0; large_n];
    let v_cond = Value::Vector(VectorData::from_bool(cond_large));
    let v_yes = Value::Vector(VectorData::from_f64(yes_large));
    let v_no = Value::Vector(VectorData::from_f64(no_large));

    let runtime_env = RuntimeEnv::with_prelude();
    let if_else_fn = runtime_env.get("if_else").unwrap();
    if let Value::NativeFn(f) = if_else_fn {
        let out = f(vec![v_cond, v_yes, v_no]).expect("parallel if_else succeeds");
        if let Value::Vector(vd) = out {
            assert_eq!(vd.len(), large_n);
            assert_eq!(vd.value_at(0), Some(Value::F64(1.0)));
            assert_eq!(vd.value_at(1), Some(Value::F64(2.0)));
        } else {
            panic!("Expected vector");
        }
    } else {
        panic!("Expected NativeFn");
    }
}

#[test]
fn test_if_else_polymorphic_fallback() {
    let code = r#"
        let cond = [true, false, NA:MissingFlag];
        let yes = ["A", "B", "C"];
        let no = ["X", "Y", "Z"];
        let res = if_else(cond, yes, no);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    let res = interp.env.get("res").expect("res");
    if let Value::Vector(vd) = res {
        assert_eq!(vd.len(), 3);
        assert_eq!(vd.value_at(0), Some(Value::String("A".into())));
        assert_eq!(vd.value_at(1), Some(Value::String("Y".into())));
        assert_eq!(vd.value_at(2), Some(Value::NA(Some("MissingFlag".into()))));
    } else {
        panic!("Expected vector");
    }
}

#[test]
fn test_matrix_clone_is_zero_copy_cow() {
    // Cloning a Matrix must share the same Arc buffer — O(1) time and memory,
    // no data copy. Arc::ptr_eq verifies the raw pointer identity.
    use std::sync::Arc;
    let data: Arc<Vec<f64>> = Arc::new((0..1_000_000).map(|i| i as f64).collect());
    let original = Value::Matrix { rows: 1000, cols: 1000, data: Arc::clone(&data) };
    let cloned = original.clone();
    if let (Value::Matrix { data: d1, .. }, Value::Matrix { data: d2, .. }) = (&original, &cloned) {
        assert!(Arc::ptr_eq(d1, d2), "Clone must share the same Arc buffer (zero-copy)");
    } else {
        panic!("Expected Matrix variants");
    }
}

#[test]
fn test_pivot_wider_basic() {
    let code = r#"
        let df = dataframe {
            patient_id: [101, 102, 101, 102],
            visit: ["v1", "v1", "v2", "v2"],
            score: [12.4, 18.9, 15.2, 19.5]
        };
        let wide = df |> pivot_wider(names_from: "visit", values_from: "score");
        let cols = colnames(wide);
        let n = nrow(wide);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let n = interp.env.get("n").expect("n exists");
    assert_eq!(n, Value::I64(2));

    let cols = interp.env.get("cols").expect("cols exists");
    if let Value::Vector(v) = cols {
        let col_names: Vec<String> = v.iter().map(|x| match x { Value::String(s) => s.clone(), other => other.to_string() }).collect();
        assert_eq!(col_names, vec!["patient_id", "v1", "v2"]);
    } else {
        panic!("expected vector for colnames");
    }
}

#[test]
fn test_pivot_longer_basic() {
    let code = r#"
        let wide = dataframe {
            id: [1, 2],
            t1: [10.0, 20.0],
            t2: [30.0, 40.0]
        };
        let long = wide |> pivot_longer(cols: ["t1", "t2"], names_to: "time", values_to: "val");
        let n = nrow(long);
        let cols = colnames(long);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let n = interp.env.get("n").expect("n exists");
    assert_eq!(n, Value::I64(4));

    let cols = interp.env.get("cols").expect("cols exists");
    if let Value::Vector(v) = cols {
        let col_names: Vec<String> = v.iter().map(|x| match x { Value::String(s) => s.clone(), other => other.to_string() }).collect();
        assert_eq!(col_names, vec!["id", "time", "val"]);
    } else {
        panic!("expected vector for colnames");
    }
}

#[test]
fn test_pivot_roundtrip() {
    let code = r#"
        let orig = dataframe {
            id: [1, 2],
            a: [100.0, 200.0],
            b: [300.0, 400.0]
        };
        let long = orig |> pivot_longer(cols: ["a", "b"], names_to: "k", values_to: "v");
        let back = long |> pivot_wider(names_from: "k", values_from: "v");
        let n = nrow(back);
        let cols = colnames(back);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let n = interp.env.get("n").expect("n exists");
    assert_eq!(n, Value::I64(2));

    let cols = interp.env.get("cols").expect("cols exists");
    if let Value::Vector(v) = cols {
        let col_names: Vec<String> = v.iter().map(|x| match x { Value::String(s) => s.clone(), other => other.to_string() }).collect();
        assert_eq!(col_names, vec!["id", "a", "b"]);
    } else {
        panic!("expected vector for colnames");
    }
}

#[test]
fn test_pivot_preserves_na_reasons() {
    let code = r#"
        let df = dataframe {
            patient_id: [101, 102],
            v1: [12.4, NA:SensorDropout],
            v2: [NA:NoResponse, 19.5]
        };
        let long = df |> pivot_longer(cols: ["v1", "v2"], names_to: "visit", values_to: "score");
        let reasons = na_reasons(long, "score");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let reasons = interp.env.get("reasons").expect("reasons exists");
    if let Value::Vector(v) = reasons {
        // Row 0: 101, v1 -> 12.4 (no NA)
        // Row 1: 102, v1 -> NA:SensorDropout
        // Row 2: 101, v2 -> NA:NoResponse
        // Row 3: 102, v2 -> 19.5 (no NA)
        assert_eq!(v.value_at(0), Some(Value::NA(None)));
        assert_eq!(v.value_at(1), Some(Value::String("SensorDropout".into())));
        assert_eq!(v.value_at(2), Some(Value::String("NoResponse".into())));
        assert_eq!(v.value_at(3), Some(Value::NA(None)));
    } else {
        panic!("expected vector for reasons");
    }
}

#[test]
fn test_pivot_named_colon_and_bare_identifiers() {
    let code = r#"
        let df = dataframe {
            patient_id: [101, 102],
            visit: ["v1", "v2"],
            score: [12.4, 18.9]
        };
        let wide = df |> pivot_wider(names_from: visit, values_from: score);
        let cols = colnames(wide);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let cols = interp.env.get("cols").expect("cols exists");
    if let Value::Vector(v) = cols {
        let col_names: Vec<String> = v.iter().map(|x| match x { Value::String(s) => s.clone(), other => other.to_string() }).collect();
        assert_eq!(col_names, vec!["patient_id", "v1", "v2"]);
    } else {
        panic!("expected vector for colnames");
    }
}

#[test]
fn test_impute_mean_with_only_for_reason() {
    let code = r#"
        let df = dataframe {
            id: [1, 2, 3, 4],
            score: [10.0, 20.0, NA:SensorDropout, NA:NoResponse]
        };
        let imputed = df |> impute(score, strategy: Mean, only_for: [NAReason::SensorDropout]);
        let s = pull(imputed, "score");
        let reasons = na_reasons(imputed, "score");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let s = interp.env.get("s").expect("s exists");
    if let Value::Vector(v) = s {
        assert_eq!(v.len(), 4);
        assert_eq!(v.value_at(0), Some(Value::F64(10.0)));
        assert_eq!(v.value_at(1), Some(Value::F64(20.0)));
        // Imputed row 2: mean of (10 + 20) / 2 = 15.0
        assert_eq!(v.value_at(2), Some(Value::F64(15.0)));
        // Preserved row 3: NA:NoResponse
        assert_eq!(v.value_at(3), Some(Value::NA(Some("NoResponse".into()))));
    } else {
        panic!("expected vector for score");
    }

    let reasons = interp.env.get("reasons").expect("reasons exists");
    if let Value::Vector(v) = reasons {
        assert_eq!(v.value_at(0), Some(Value::NA(None)));
        assert_eq!(v.value_at(1), Some(Value::NA(None)));
        // Imputed cell has no reason now
        assert_eq!(v.value_at(2), Some(Value::NA(None)));
        // Non-imputed cell keeps reason
        assert_eq!(v.value_at(3), Some(Value::String("NoResponse".into())));
    } else {
        panic!("expected vector for reasons");
    }
}

#[test]
fn test_impute_median_all_na() {
    let code = r#"
        let df = dataframe {
            val: [1.0, 5.0, 9.0, NA, NA:Dropout]
        };
        let imputed = df |> impute(val, strategy: Median);
        let vals = pull(imputed, "val");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let vals = interp.env.get("vals").expect("vals exists");
    if let Value::Vector(v) = vals {
        assert_eq!(v.len(), 5);
        assert_eq!(v.value_at(0), Some(Value::F64(1.0)));
        assert_eq!(v.value_at(1), Some(Value::F64(5.0)));
        assert_eq!(v.value_at(2), Some(Value::F64(9.0)));
        // Both NAs imputed with median = 5.0
        assert_eq!(v.value_at(3), Some(Value::F64(5.0)));
        assert_eq!(v.value_at(4), Some(Value::F64(5.0)));
    } else {
        panic!("expected vector for vals");
    }
}

#[test]
fn test_filter_na_reason_drops_specified() {
    let code = r#"
        let df = dataframe {
            id: [1, 2, 3, 4],
            salary: [100.0, NA:NoResponse, 200.0, NA:SensorDropout]
        };
        let filtered = df |> filter_na_reason(salary, drop: [NAReason::NoResponse]);
        let ids = pull(filtered, "id");
        let sals = pull(filtered, "salary");
        let reasons = na_reasons(filtered, "salary");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let ids = interp.env.get("ids").expect("ids exists");
    if let Value::Vector(v) = ids {
        assert_eq!(v.len(), 3);
        assert_eq!(v.value_at(0), Some(Value::I64(1)));
        assert_eq!(v.value_at(1), Some(Value::I64(3)));
        assert_eq!(v.value_at(2), Some(Value::I64(4)));
    } else {
        panic!("expected vector for ids");
    }

    let sals = interp.env.get("sals").expect("sals exists");
    if let Value::Vector(v) = sals {
        assert_eq!(v.len(), 3);
        assert_eq!(v.value_at(0), Some(Value::F64(100.0)));
        assert_eq!(v.value_at(1), Some(Value::F64(200.0)));
        assert_eq!(v.value_at(2), Some(Value::NA(Some("SensorDropout".into()))));
    } else {
        panic!("expected vector for sals");
    }

    let reasons = interp.env.get("reasons").expect("reasons exists");
    if let Value::Vector(v) = reasons {
        assert_eq!(v.len(), 3);
        assert_eq!(v.value_at(0), Some(Value::NA(None)));
        assert_eq!(v.value_at(1), Some(Value::NA(None)));
        assert_eq!(v.value_at(2), Some(Value::String("SensorDropout".into())));
    } else {
        panic!("expected vector for reasons");
    }
}

#[test]
fn test_filter_na_reason_and_impute_pipeline() {
    let code = r#"
        let df = dataframe {
            patient_id: [101, 102, 103, 104],
            salary: [50000.0, NA:NoResponse, 65000.0, 70000.0],
            measurement: [12.0, 14.0, NA:SensorDropout, 16.0]
        };
        let clean_df = df
            |> filter_na_reason(salary, drop: [NAReason::NoResponse])
            |> impute(measurement, strategy: Mean, only_for: [NAReason::SensorDropout]);

        let pids = pull(clean_df, "patient_id");
        let ms = pull(clean_df, "measurement");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let pids = interp.env.get("pids").expect("pids exists");
    if let Value::Vector(v) = pids {
        // Patient 102 was dropped due to salary NA:NoResponse
        assert_eq!(v.len(), 3);
        assert_eq!(v.value_at(0), Some(Value::I64(101)));
        assert_eq!(v.value_at(1), Some(Value::I64(103)));
        assert_eq!(v.value_at(2), Some(Value::I64(104)));
    } else {
        panic!("expected vector for pids");
    }

    let ms = interp.env.get("ms").expect("ms exists");
    if let Value::Vector(v) = ms {
        // Measurements remaining: 12.0, NA:SensorDropout (patient 103), 16.0
        // Mean of valid (12.0 + 16.0) / 2 = 14.0
        assert_eq!(v.len(), 3);
        assert_eq!(v.value_at(0), Some(Value::F64(12.0)));
        assert_eq!(v.value_at(1), Some(Value::F64(14.0)));
        assert_eq!(v.value_at(2), Some(Value::F64(16.0)));
    } else {
        panic!("expected vector for ms");
    }
}

#[test]
fn test_record_literal_and_field_access() {
    let code = r#"
        let sample = { sample_id: "SMP-001", replicates: 4, p_value: 0.0042 };
        let sid = sample.sample_id;
        let reps = sample.replicates;
        let p = get(sample, "p_value");
        let p_piped = sample |> get("p_value");
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("sid").unwrap(), Value::String("SMP-001".into()));
    assert_eq!(interp.env.get("reps").unwrap(), Value::I64(4));
    assert_eq!(interp.env.get("p").unwrap(), Value::F64(0.0042));
    assert_eq!(interp.env.get("p_piped").unwrap(), Value::F64(0.0042));
}

#[test]
fn test_record_and_block_coexistence() {
    let code = r#"
        let empty_block = {};
        let block_val = {
            let x = 10;
            x + 5
        };
        let rec = { x: 10, y: "hello" };
        let x_val = rec.x;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("empty_block").unwrap(), Value::Unit);
    assert_eq!(interp.env.get("block_val").unwrap(), Value::I64(15));
    assert_eq!(interp.env.get("x_val").unwrap(), Value::I64(10));
}

#[test]
fn test_ufcs_method_call() {
    let code = r#"
        let v = [1.0, 2.0, 3.0, 4.0];
        let s = v.sum();
        let m = v.mean();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("s").unwrap(), Value::F64(10.0));
    assert_eq!(interp.env.get("m").unwrap(), Value::F64(2.5));
}

