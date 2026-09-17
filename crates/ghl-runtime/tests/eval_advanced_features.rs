//! Records, UFCS method calls, bracket indexing, struct/trait/impl, autodiff, HTTP microservice, GPU/Philox PRNG, Parquet/plot export, factors.

mod common;

use ghl_runtime::value::Value;
use ghl_runtime::eval::Interpreter;
use ghl_syntax::parser::parse;

#[test]
fn test_bracket_indexing_and_slicing() {
    let code = r#"
        let v = [10.0, 20.0, 30.0, 40.0, 50.0];
        let first = v[0];
        let sub_half = v[1..3];
        let sub_incl = v[1..=3];
        let all_v = v[..];
        let masked = v[v > 25.0];
        let gathered = v[[0, 2, 4]];

        let m = mat [ 1.0, 2.0, 3.0 ; 4.0, 5.0, 6.0 ; 7.0, 8.0, 9.0 ];
        let elem = m[1, 2];
        let row_slice = m[0..2, :];
        let col_vec = m[:, 1];
        let row_vec = m[0, :];

        let s = "hello world";
        let s_ch = s[0];
        let s_sub = s[0..5];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("first").unwrap(), Value::F64(10.0));

    if let Value::Vector(v_half) = interp.env.get("sub_half").unwrap() {
        assert_eq!(v_half.len(), 2);
        assert_eq!(v_half.value_at(0), Some(Value::F64(20.0)));
        assert_eq!(v_half.value_at(1), Some(Value::F64(30.0)));
    } else {
        panic!("expected Vector for sub_half");
    }

    if let Value::Vector(v_incl) = interp.env.get("sub_incl").unwrap() {
        assert_eq!(v_incl.len(), 3);
        assert_eq!(v_incl.value_at(0), Some(Value::F64(20.0)));
        assert_eq!(v_incl.value_at(1), Some(Value::F64(30.0)));
        assert_eq!(v_incl.value_at(2), Some(Value::F64(40.0)));
    } else {
        panic!("expected Vector for sub_incl");
    }

    if let Value::Vector(v_all) = interp.env.get("all_v").unwrap() {
        assert_eq!(v_all.len(), 5);
    } else {
        panic!("expected Vector for all_v");
    }

    if let Value::Vector(v_mask) = interp.env.get("masked").unwrap() {
        assert_eq!(v_mask.len(), 3);
        assert_eq!(v_mask.value_at(0), Some(Value::F64(30.0)));
        assert_eq!(v_mask.value_at(1), Some(Value::F64(40.0)));
        assert_eq!(v_mask.value_at(2), Some(Value::F64(50.0)));
    } else {
        panic!("expected Vector for masked");
    }

    if let Value::Vector(v_gath) = interp.env.get("gathered").unwrap() {
        assert_eq!(v_gath.len(), 3);
        assert_eq!(v_gath.value_at(0), Some(Value::F64(10.0)));
        assert_eq!(v_gath.value_at(1), Some(Value::F64(30.0)));
        assert_eq!(v_gath.value_at(2), Some(Value::F64(50.0)));
    } else {
        panic!("expected Vector for gathered");
    }

    assert_eq!(interp.env.get("elem").unwrap(), Value::F64(6.0));

    if let Value::Matrix { rows, cols, data } = interp.env.get("row_slice").unwrap() {
        assert_eq!(rows, 2);
        assert_eq!(cols, 3);
        assert_eq!(&data[..], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    } else {
        panic!("expected Matrix for row_slice");
    }

    if let Value::Vector(col) = interp.env.get("col_vec").unwrap() {
        assert_eq!(col.len(), 3);
        assert_eq!(col.value_at(0), Some(Value::F64(2.0)));
        assert_eq!(col.value_at(1), Some(Value::F64(5.0)));
        assert_eq!(col.value_at(2), Some(Value::F64(8.0)));
    } else {
        panic!("expected Vector for col_vec");
    }

    if let Value::Vector(row) = interp.env.get("row_vec").unwrap() {
        assert_eq!(row.len(), 3);
        assert_eq!(row.value_at(0), Some(Value::F64(1.0)));
        assert_eq!(row.value_at(1), Some(Value::F64(2.0)));
        assert_eq!(row.value_at(2), Some(Value::F64(3.0)));
    } else {
        panic!("expected Vector for row_vec");
    }

    assert_eq!(interp.env.get("s_ch").unwrap(), Value::String("h".into()));
    assert_eq!(interp.env.get("s_sub").unwrap(), Value::String("hello".into()));
}

#[test]
fn test_eval_struct_trait_and_impl() {
    let code = r#"
        struct NormalDistribution {
            mean: f64,
            std_dev: f64,
        }

        trait Distribution {
            type Output;
            fn log_pdf(&self, x: Self::Output) -> f64;
        }

        impl Distribution for NormalDistribution {
            type Output = f64;
            fn log_pdf(&self, x: f64) -> f64 {
                let diff = (x - self.mean) / self.std_dev;
                -0.5 * diff * diff
            }
        }

        let dist = NormalDistribution { mean: 2.0, std_dev: 1.0 };
        let val = dist.log_pdf(4.0);
        let mean_val = dist.mean;
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("mean_val").unwrap(), Value::F64(2.0));
    assert_eq!(interp.env.get("val").unwrap(), Value::F64(-2.0));
}

#[test]
fn test_eval_arena_scope_surface_syntax() {
    let code = r#"
        let total = arena::scope(|arena| {
            let v = arena.alloc_vector(4, 2.5);
            let m = arena.alloc_matrix(2, 2, 10.0);
            let b_before = arena.allocated_bytes();
            arena.reset();
            let b_after = arena.allocated_bytes();
            let v2 = arena.alloc_vector(2, 5.0);
            v2[0] + v2[1]
        });
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("total").unwrap(), Value::F64(10.0));
}

#[test]
fn test_autodiff_scalar_and_vector_gradients() {
    let code = r#"
        // Scalar differentiation: f(x) = x^3 - 2x => f'(3) = 25.0
        let f = |x| x * x * x - 2.0 * x;
        let df = grad(f);
        let d_scalar = df(3.0);

        // Vector gradient: loss(w) = w[0]^2 + 3*w[0]*w[1] + 2*w[1]^2
        // ∇loss([1, 2]) = [2*(1) + 3*(2), 3*(1) + 4*(2)] = [8.0, 11.0]
        let loss = |w| w[0] * w[0] + 3.0 * w[0] * w[1] + 2.0 * w[1] * w[1];
        let dloss = grad(loss);
        let g = dloss([1.0, 2.0]);
        let g0 = g[0];
        let g1 = g[1];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let d_scalar = interp.env.get("d_scalar").unwrap().as_f64().unwrap();
    assert!((d_scalar - 25.0).abs() < 1e-6, "Expected 25.0, got {d_scalar}");

    let g0 = interp.env.get("g0").unwrap().as_f64().unwrap();
    let g1 = interp.env.get("g1").unwrap().as_f64().unwrap();
    assert!((g0 - 8.0).abs() < 1e-6, "Expected 8.0, got {g0}");
    assert!((g1 - 11.0).abs() < 1e-6, "Expected 11.0, got {g1}");
}

#[test]
fn test_autodiff_value_and_grad_and_jacobian() {
    let code = r#"
        let loss = |w| w[0] * w[0] + w[1] * w[1];
        let vg = value_and_grad(loss, [3.0, 4.0]);
        let v = vg.value;
        let g = vg.grad;
        let g0 = g[0];
        let g1 = g[1];

        // Jacobian for f(w) = [w[0] * w[1], w[0] + w[1]]
        // J = [w[1], w[0] ; 1, 1] => at [2, 3] => [3, 2 ; 1, 1]
        let f_vec = |w| [w[0] * w[1], w[0] + w[1]];
        let J = jacobian(f_vec, [2.0, 3.0]);
        let j00 = J[0, 0];
        let j01 = J[0, 1];
        let j10 = J[1, 0];
        let j11 = J[1, 1];
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let v = interp.env.get("v").unwrap().as_f64().unwrap();
    assert!((v - 25.0).abs() < 1e-6, "Expected 25.0, got {v}");

    let g0 = interp.env.get("g0").unwrap().as_f64().unwrap();
    let g1 = interp.env.get("g1").unwrap().as_f64().unwrap();
    assert!((g0 - 6.0).abs() < 1e-6, "Expected 6.0, got {g0}");
    assert!((g1 - 8.0).abs() < 1e-6, "Expected 8.0, got {g1}");

    let j00 = interp.env.get("j00").unwrap().as_f64().unwrap();
    let j01 = interp.env.get("j01").unwrap().as_f64().unwrap();
    let j10 = interp.env.get("j10").unwrap().as_f64().unwrap();
    let j11 = interp.env.get("j11").unwrap().as_f64().unwrap();
    assert!((j00 - 3.0).abs() < 1e-6, "Expected 3.0, got {j00}");
    assert!((j01 - 2.0).abs() < 1e-6, "Expected 2.0, got {j01}");
    assert!((j10 - 1.0).abs() < 1e-6, "Expected 1.0, got {j10}");
    assert!((j11 - 1.0).abs() < 1e-6, "Expected 1.0, got {j11}");
}

#[test]
fn test_http_microservice_server_and_client() {
    let code = r#"
        let server = http::serve("127.0.0.1:0", |req| {
            if req.path == "/predict" {
                http::response(200, "{\"prediction\": 42.0}")
            } else {
                http::response(404, "Not Found")
            }
        });

        let url_predict = "http://" + server.addr + "/predict";
        let url_missing = "http://" + server.addr + "/other";

        let resp_ok = http::get(url_predict);
        let resp_not_found = http::get(url_missing);

        let ok_status = resp_ok.status;
        let ok_body = resp_ok.body;
        let ok_flag = resp_ok.ok;
        let not_found_status = resp_not_found.status;

        let post_resp = http::post(url_predict, "{\"input\": 10}", "application/json");
        let post_status = post_resp.status;

        server.stop();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("ok_status").unwrap(), Value::I64(200));
    assert_eq!(interp.env.get("ok_body").unwrap(), Value::String("{\"prediction\": 42.0}".into()));
    assert_eq!(interp.env.get("ok_flag").unwrap(), Value::Bool(true));
    assert_eq!(interp.env.get("not_found_status").unwrap(), Value::I64(404));
    assert_eq!(interp.env.get("post_status").unwrap(), Value::I64(200));
}

#[test]
fn test_rfc05_parallel_iterators() {
    let code = r#"
        let mapped = (0..5).par_iter().map(|x| x * 2).collect();
        let sum_val = (1..=10).par_iter().sum();
        let count_val = (0..50).par_iter().count();
        let reduced = (1..=4).par_iter().reduce(|a, b| a + b);
        let filtered = [1.0, 5.0, 2.0, 8.0].par_iter().filter(|x| x > 3.0).collect();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let mapped = interp.env.get("mapped").unwrap();
    if let Value::Vector(vd) = mapped {
        assert_eq!(vd.to_vec(), vec![Value::I64(0), Value::I64(2), Value::I64(4), Value::I64(6), Value::I64(8)]);
    } else {
        panic!("Expected Vector for mapped, got {:?}", mapped);
    }

    assert_eq!(interp.env.get("sum_val").unwrap(), Value::I64(55));
    assert_eq!(interp.env.get("count_val").unwrap(), Value::I64(50));
    assert_eq!(interp.env.get("reduced").unwrap(), Value::I64(10));

    let filtered = interp.env.get("filtered").unwrap();
    if let Value::Vector(vd) = filtered {
        assert_eq!(vd.to_vec(), vec![Value::F64(5.0), Value::F64(8.0)]);
    } else {
        panic!("Expected Vector for filtered, got {:?}", filtered);
    }
}

#[test]
fn test_rfc05_gpu_and_philox_prng() {
    let code = r#"
        let dev = Device::default_gpu();
        let a = mat [1.0, 2.0; 3.0, 4.0];
        let b = mat [2.0, 0.0; 1.0, 2.0];
        let gpu_a = a.to_gpu(dev);
        let gpu_b = b.to_gpu(dev);
        let gpu_c = gpu_a.matmul(gpu_b);
        let c = gpu_c.to_cpu();

        // Cholesky on symmetric positive-definite matrix
        let spd = mat [4.0, 12.0; 12.0, 45.0];
        let gpu_spd = spd.to_gpu(dev);
        let gpu_l = gpu_spd.cholesky();
        let l = gpu_l.to_cpu();

        // Vector reduce_sum
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        let gpu_v = v.to_gpu(dev);
        let total = gpu_v.reduce_sum();

        // Philox PRNG
        let rng = PhiloxRng::seed(42);
        let uniform_sample = rng.sample_uniform(dev, 100, 0.0, 1.0).to_cpu();
        let normal_sample = rng.sample_normal(dev, 100, 0.0, 1.0).to_cpu();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    // Check GEMM result: [[1, 2], [3, 4]] * [[2, 0], [1, 2]] = [[4, 4], [10, 8]]
    let c = interp.env.get("c").unwrap();
    if let Value::Matrix { rows, cols, data } = c {
        assert_eq!(rows, 2);
        assert_eq!(cols, 2);
        assert_eq!(*data, vec![4.0, 4.0, 10.0, 8.0]);
    } else {
        panic!("Expected Matrix for c, got {:?}", c);
    }

    // Check Cholesky result: L * L^T = [[4, 12], [12, 45]] => L = [[2, 0], [6, 3]]
    let l = interp.env.get("l").unwrap();
    if let Value::Matrix { rows, cols, data } = l {
        assert_eq!(rows, 2);
        assert_eq!(cols, 2);
        assert!((data[0] - 2.0).abs() < 1e-6);
        assert!((data[1] - 0.0).abs() < 1e-6);
        assert!((data[2] - 6.0).abs() < 1e-6);
        assert!((data[3] - 3.0).abs() < 1e-6);
    } else {
        panic!("Expected Matrix for l, got {:?}", l);
    }

    // Check reduce_sum: 1 + 2 + 3 + 4 + 5 = 15.0
    assert_eq!(interp.env.get("total").unwrap(), Value::F64(15.0));

    // Check Philox PRNG samples length and bounds
    let uniform_sample = interp.env.get("uniform_sample").unwrap();
    if let Value::Vector(vd) = uniform_sample {
        assert_eq!(vd.len(), 100);
        for &x in vd.as_f64_view().unwrap().as_slice() {
            assert!(x >= 0.0 && x <= 1.0, "Uniform sample out of bounds: {x}");
        }
    }

    let normal_sample = interp.env.get("normal_sample").unwrap();
    if let Value::Vector(vd) = normal_sample {
        assert_eq!(vd.len(), 100);
    }

    // Verify WGSL shader caching
    let cache = ghl_runtime::gpu::ComputePipelineCache::global();
    assert!(cache.get_shader("gemm").unwrap().contains("@compute"));
    assert!(cache.get_shader("reduce_sum").unwrap().contains("@compute"));
    assert!(cache.get_shader("philox_rng").unwrap().contains("@compute"));
}

#[test]
fn test_view_dataframe_parquet_export() {
    let code = r#"
        let df = dataframe {
            id: [1, 2, 3],
            val: [10.5, 20.0, 31.5]
        };
        let path = view(df);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let path_val = interp.env.get("path").unwrap();
    if let Value::String(s) = path_val {
        assert!(s.ends_with(".parquet"));
        assert!(std::path::Path::new(&s).exists());
        let _ = std::fs::remove_file(&s);
    } else {
        panic!("Expected string path from view()");
    }
}

#[test]
fn test_show_plot_svg_export() {
    let temp_dir = std::env::temp_dir().join("ghl_test_plots");
    let _ = std::fs::create_dir_all(&temp_dir);
    unsafe {
        std::env::set_var("GHL_PLOTS_DIR", &temp_dir);
    }

    let code = r#"
        let df = dataframe {
            x: [1.0, 2.0, 3.0],
            y: [2.0, 4.0, 6.0]
        };
        let p = df |> plot(aes(col("x"), col("y"))) |> geom_point();
        show(p);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let files: Vec<_> = std::fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("svg"))
        .collect();
    assert!(!files.is_empty(), "Expected SVG plot to be saved to GHL_PLOTS_DIR");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
    unsafe {
        std::env::remove_var("GHL_PLOTS_DIR");
    }
}

#[test]
fn test_factor_creation_and_levels() {
    let code = r#"
        let f = factor(["low", "med", "high", "med"]);
        let lvls = levels(f);
        let ord = ordered_factor(["small", "large", "small"]);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let lvls = interp.env.get("lvls").expect("lvls exists");
    if let Value::Vector(vec) = lvls {
        let str_lvls: Vec<String> = vec.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
        assert_eq!(str_lvls, vec!["high", "low", "med"]);
    } else {
        panic!("Expected Vector for levels");
    }
}

#[test]
fn test_ols_categorical_factor_expansion() {
    // Group A: y ~ 10, Group B: y ~ 20, Group C: y ~ 30
    let code = r#"
        let df = dataframe {
            grp: ["A", "A", "B", "B", "C", "C"],
            y: [10.0, 10.0, 20.0, 20.0, 30.0, 30.0]
        };
        let model = ols(y ~ grp, df);
        let coefficients = coef(model);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    let model = interp.env.get("model").expect("model exists");
    if let Value::ModelFit(m) = model {
        // (Intercept) = 10.0, grpB = 10.0, grpC = 20.0
        assert_eq!(m.blueprint.term_names, vec!["(Intercept)", "grpB", "grpC"]);
        assert!((m.coefficients[0] - 10.0).abs() < 1e-10);
        assert!((m.coefficients[1] - 10.0).abs() < 1e-10);
        assert!((m.coefficients[2] - 20.0).abs() < 1e-10);
    } else {
        panic!("Expected ModelFit");
    }
}

#[test]
fn test_plot_themes_minimal_classic_dark() {
    let code = r#"
        let df = dataframe { x: [1.0, 2.0, 3.0], y: [2.0, 4.0, 6.0] };
        let p1 = plot(df, aes(col("x"), col("y"))) |> geom_point() |> theme_minimal();
        let p2 = plot(df, aes(col("x"), col("y"))) |> geom_point() |> theme_classic();
        let p3 = plot(df, aes(col("x"), col("y"))) |> geom_point() |> theme_dark();
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");
    assert!(interp.env.get("p1").is_some());
    assert!(interp.env.get("p2").is_some());
    assert!(interp.env.get("p3").is_some());
}
