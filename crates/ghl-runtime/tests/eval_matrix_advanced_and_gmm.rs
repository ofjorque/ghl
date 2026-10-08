//! Vector/matrix get/set, Gibbs sampler, transpose/diag/log_sum_exp, EM Gaussian Mixture Models.

mod common;

use ghl_runtime::eval::Interpreter;
use ghl_runtime::value::Value;
use ghl_runtime::vector_data::VectorData;
use ghl_syntax::parser::parse;
use rand::SeedableRng;

#[test]
fn test_zeros_vector_and_matrix() {
    let code = r#"
        let v = zeros(4);
        let m = zeros(2, 3);
        let v_len = len(v);
        let m_len = len(m);
        let m_val = get(m, 1, 2);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("v_len"), Some(Value::I64(4)));
    assert_eq!(interp.env.get("m_len"), Some(Value::I64(2)));
    assert_eq!(interp.env.get("m_val"), Some(Value::F64(0.0)));
}

#[test]
fn test_len_polymorphic() {
    let code = r#"
        let l_vec = len([1, 2, 3, 4, 5]);
        let l_str = len("hello");
        let l_df = len(dataframe { a: [1, 2], b: [3, 4] });
        let l_mat = len(zeros(3, 7));
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("l_vec"), Some(Value::I64(5)));
    assert_eq!(interp.env.get("l_str"), Some(Value::I64(5)));
    assert_eq!(interp.env.get("l_df"), Some(Value::I64(2)));
    assert_eq!(interp.env.get("l_mat"), Some(Value::I64(3)));
}

#[test]
fn test_get_set_vector_and_matrix() {
    let code = r#"
        let mut v = [10.0, 20.0, 30.0];
        let first_val = get(v, 0);
        v = set(v, 1, 99.0);
        let mid_val = get(v, 1);

        let mut m = zeros(2, 2);
        m = set(m, 0, 1, 42.0);
        let cell = get(m, 0, 1);

        // Vector gather
        let mu = [100.0, 200.0];
        let idx = [0, 1, 1, 0];
        let gathered = get(mu, idx);
        let g_sum = sum(gathered);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("first_val"), Some(Value::F64(10.0)));
    assert_eq!(interp.env.get("mid_val"), Some(Value::F64(99.0)));
    assert_eq!(interp.env.get("cell"), Some(Value::F64(42.0)));
    assert_eq!(interp.env.get("g_sum"), Some(Value::F64(600.0)));
}

#[test]
fn test_matrix_get_set_row_col() {
    let code = r#"
        let mut m = zeros(2, 3);
        m = set_row(m, 0, [1.0, 2.0, 3.0]);
        m = set_row(m, 1, [4.0, 5.0, 6.0]);
        let r0 = get_row(m, 0);
        let r1 = get_row(m, 1);
        let c2 = get_col(m, 2);
        let s_r0 = sum(r0);
        let s_r1 = sum(r1);
        let s_c2 = sum(c2);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("s_r0"), Some(Value::F64(6.0)));
    assert_eq!(interp.env.get("s_r1"), Some(Value::F64(15.0)));
    assert_eq!(interp.env.get("s_c2"), Some(Value::F64(9.0)));
}

#[test]
fn test_gibbs_sampler_runs_end_to_end_and_recovers_means() {
    // Synthetic data: 3 groups, true means: group 0 = 3.0, group 1 = 8.0, group 2 = -4.0
    let mut y = Vec::new();
    let mut groups = Vec::new();
    let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(42);
    let true_means = [3.0, 8.0, -4.0];
    let n_per_group = 80;
    use rand_distr::{Distribution as _, Normal as RNormal};
    for (g, &m) in true_means.iter().enumerate() {
        let dist = RNormal::new(m, 0.8).unwrap();
        for _ in 0..n_per_group {
            y.push(dist.sample(&mut rng));
            groups.push(g as i64);
        }
    }

    let runner_code = r#"
        fn run_gibbs(y, groups, num_iterations) {
            let num_groups = max(groups) + 1;
            let mut trace = zeros(num_iterations, num_groups);
            let mut tau = 1.0;
            let mut mu_vec = zeros(num_groups);
            let n_total = len(y);

            let mut iter = 0;
            while iter < num_iterations {
                let mut j = 0;
                while j < num_groups {
                    let y_j = filter(y, groups == j);
                    let n_j = len(y_j);
                    let post_mean = (sum(y_j) * tau) / (n_j * tau + 1.0);
                    let post_sd = 1.0 / sqrt(n_j * tau + 1.0);
                    let draw = first(random_normal(1, post_mean, post_sd));
                    mu_vec = set(mu_vec, j, draw);
                    j = j + 1;
                };

                let diff = y - get(mu_vec, groups);
                let ssq = dot(diff, diff);
                let alpha_post = 1.0 + n_total / 2.0;
                let beta_post = 1.0 + ssq / 2.0;
                tau = first(random_gamma(1, alpha_post, beta_post));

                trace = set_row(trace, iter, mu_vec);
                iter = iter + 1;
            };

            trace
        }

        let iterations = 200;
        let trace = run_gibbs(y, groups, iterations);
        let trace_rows = len(trace);
        let col0 = get_col(trace, 0);
        let col1 = get_col(trace, 1);
        let col2 = get_col(trace, 2);
        let mean0 = mean(col0);
        let mean1 = mean(col1);
        let mean2 = mean(col2);
    "#;
    let program = parse(runner_code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp
        .env
        .set("y".to_string(), Value::Vector(VectorData::from_f64(y)));
    interp.env.set(
        "groups".to_string(),
        Value::Vector(VectorData::from_values(
            groups.into_iter().map(Value::I64).collect(),
        )),
    );
    interp.eval_program(&program).expect("evaluation ok");

    assert_eq!(interp.env.get("trace_rows"), Some(Value::I64(200)));
    let m0 = interp.env.get("mean0").unwrap().as_f64().unwrap();
    let m1 = interp.env.get("mean1").unwrap().as_f64().unwrap();
    let m2 = interp.env.get("mean2").unwrap().as_f64().unwrap();

    // Check that posterior means converge closely to true values (3.0, 8.0, -4.0)
    assert!(
        (m0 - 3.0).abs() < 0.25,
        "mean0 {} expected close to 3.0",
        m0
    );
    assert!(
        (m1 - 8.0).abs() < 0.25,
        "mean1 {} expected close to 8.0",
        m1
    );
    assert!(
        (m2 - (-4.0)).abs() < 0.25,
        "mean2 {} expected close to -4.0",
        m2
    );
}

#[test]
fn test_transpose_and_identity() {
    let code = r#"
        let id = identity(3);
        let id_rows = len(id);
        let d0 = get(id, 0, 0);
        let d1 = get(id, 1, 1);
        let d2 = get(id, 2, 2);
        let off = get(id, 0, 1);

        let m = mat [ 1.0, 2.0 ; 3.0, 4.0 ; 5.0, 6.0 ];
        let mt = transpose(m);
        let mt_rows = len(mt);
        let mt_00 = get(mt, 0, 0);
        let mt_02 = get(mt, 0, 2);
        let mt_11 = get(mt, 1, 1);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    assert_eq!(interp.env.get("id_rows"), Some(Value::I64(3)));
    assert_eq!(interp.env.get("d0").unwrap().as_f64(), Some(1.0));
    assert_eq!(interp.env.get("d1").unwrap().as_f64(), Some(1.0));
    assert_eq!(interp.env.get("d2").unwrap().as_f64(), Some(1.0));
    assert_eq!(interp.env.get("off").unwrap().as_f64(), Some(0.0));

    assert_eq!(interp.env.get("mt_rows"), Some(Value::I64(2)));
    assert_eq!(interp.env.get("mt_00").unwrap().as_f64(), Some(1.0));
    assert_eq!(interp.env.get("mt_02").unwrap().as_f64(), Some(5.0));
    assert_eq!(interp.env.get("mt_11").unwrap().as_f64(), Some(4.0));
}

#[test]
fn test_diag_vector_and_matrix() {
    let code = r#"
        let v = [10.0, 20.0, 30.0];
        let dm = diag(v);
        let dm_00 = get(dm, 0, 0);
        let dm_11 = get(dm, 1, 1);
        let dm_01 = get(dm, 0, 1);

        let extracted = diag(dm);
        let ex_0 = get(extracted, 0);
        let ex_1 = get(extracted, 1);
        let ex_2 = get(extracted, 2);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    assert_eq!(interp.env.get("dm_00").unwrap().as_f64(), Some(10.0));
    assert_eq!(interp.env.get("dm_11").unwrap().as_f64(), Some(20.0));
    assert_eq!(interp.env.get("dm_01").unwrap().as_f64(), Some(0.0));

    assert_eq!(interp.env.get("ex_0").unwrap().as_f64(), Some(10.0));
    assert_eq!(interp.env.get("ex_1").unwrap().as_f64(), Some(20.0));
    assert_eq!(interp.env.get("ex_2").unwrap().as_f64(), Some(30.0));
}

#[test]
fn test_log_sum_exp_stability() {
    let code = r#"
        let normal_v = [1.0, 2.0, 3.0];
        let lse_normal = log_sum_exp(normal_v);

        // Large values that would overflow naive exp()
        let big_v = [1000.0, 1001.0];
        let lse_big = log_sum_exp(big_v);

        // Negative values that would underflow naive exp()
        let small_v = [-1001.0, -1000.0];
        let lse_small = log_sum_exp(small_v);
    "#;
    let program = parse(code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    let normal = interp.env.get("lse_normal").unwrap().as_f64().unwrap();
    let expected_normal = (1.0_f64.exp() + 2.0_f64.exp() + 3.0_f64.exp()).ln();
    assert!((normal - expected_normal).abs() < 1e-10);

    let big = interp.env.get("lse_big").unwrap().as_f64().unwrap();
    let expected_big = 1001.0 + (1.0 + (-1.0_f64).exp()).ln();
    assert!((big - expected_big).abs() < 1e-10);

    let small = interp.env.get("lse_small").unwrap().as_f64().unwrap();
    let expected_small = -1000.0 + (1.0 + (-1.0_f64).exp()).ln();
    assert!((small - expected_small).abs() < 1e-10);
}

#[test]
fn test_em_gmm_end_to_end_and_recovers_clusters() {
    use rand::SeedableRng;
    use rand_distr::{Distribution, Normal as RNormal};
    use rand_xoshiro::Xoshiro256PlusPlus;

    // 1. Generate 200 2D synthetic observations from 2 Gaussian clusters:
    // Cluster 0: 100 points around (-3.0, -3.0)
    // Cluster 1: 100 points around (4.0, 4.0)
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(42);
    let n_per_cluster = 100;
    let mut x_data = Vec::with_capacity(n_per_cluster * 2 * 2);

    let d0 = RNormal::new(-3.0, 0.5).unwrap();
    for _ in 0..n_per_cluster {
        x_data.push(d0.sample(&mut rng));
        x_data.push(d0.sample(&mut rng));
    }

    let d1 = RNormal::new(4.0, 0.5).unwrap();
    for _ in 0..n_per_cluster {
        x_data.push(d1.sample(&mut rng));
        x_data.push(d1.sample(&mut rng));
    }

    let n = (n_per_cluster * 2) as i64;
    let d = 2_i64;

    // GHL EM script:
    let ghl_code = r#"
        fn run_em(X, num_clusters, num_iterations) {
            let N = len(X);
            let D = 2;
            let K = num_clusters;

            // Initialize pi uniformly: [0.5, 0.5]
            let mut pi = [0.5, 0.5];

            // Initialize mu with two distinct points
            let mut mu = zeros(K, D);
            mu = set_row(mu, 0, [-1.0, 0.0]);
            mu = set_row(mu, 1, [1.0, 0.0]);

            // Initialize diagonal variances to 1.0
            let mut vars = zeros(K, D);
            vars = set_row(vars, 0, [1.0, 1.0]);
            vars = set_row(vars, 1, [1.0, 1.0]);

            let two_pi = 6.283185307179586;
            let log_two_pi_d = 2.0 * log(two_pi);

            let mut iter = 0;
            while iter < num_iterations {
                // --- E-step ---
                // Compute responsibilities Gamma (N x K)
                let mut Gamma = zeros(N, K);
                let mut i = 0;
                while i < N {
                    let xi = get_row(X, i);
                    let mut log_p = zeros(K);

                    let mut c = 0;
                    while c < K {
                        let mu_c = get_row(mu, c);
                        let var_c = get_row(vars, c);
                        let diff = xi - mu_c;
                        let diff_sq = diff * diff;
                        let mahal = sum(diff_sq / var_c);
                        let log_det = sum(log(var_c));
                        let log_gauss = -0.5 * (log_two_pi_d + log_det + mahal);
                        let log_prior = log(get(pi, c));
                        log_p = set(log_p, c, log_prior + log_gauss);
                        c = c + 1;
                    };

                    let lse = log_sum_exp(log_p);
                    let mut c2 = 0;
                    while c2 < K {
                        let resp = exp(get(log_p, c2) - lse);
                        Gamma = set(Gamma, i, c2, resp);
                        c2 = c2 + 1;
                    };

                    i = i + 1;
                };

                // --- M-step ---
                // N_k = sum of responsibilities for cluster k
                let Gamma_T = transpose(Gamma);
                let weighted_X = Gamma_T * X;

                let mut c3 = 0;
                while c3 < K {
                    let gamma_c = get_row(Gamma_T, c3);
                    let N_c = sum(gamma_c);
                    pi = set(pi, c3, N_c / N);

                    // Updated mean
                    let new_mu_c = get_row(weighted_X, c3) / N_c;
                    mu = set_row(mu, c3, new_mu_c);

                    // Updated variance with small ridge regularization
                    let mut sum_sq = zeros(D);
                    let mut i2 = 0;
                    while i2 < N {
                        let xi2 = get_row(X, i2);
                        let diff2 = xi2 - new_mu_c;
                        let w = get(gamma_c, i2);
                        sum_sq = sum_sq + (diff2 * diff2) * w;
                        i2 = i2 + 1;
                    };
                    let new_var_c = sum_sq / N_c + [0.0001, 0.0001];
                    vars = set_row(vars, c3, new_var_c);

                    c3 = c3 + 1;
                };

                iter = iter + 1;
            };

            mu
        }

        let final_mu = run_em(X, 2, 20);
        let mu0 = get_row(final_mu, 0);
        let mu1 = get_row(final_mu, 1);
        let m0_x = get(mu0, 0);
        let m0_y = get(mu0, 1);
        let m1_x = get(mu1, 0);
        let m1_y = get(mu1, 1);
    "#;

    let program = parse(ghl_code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.env.set(
        "X".to_string(),
        Value::Matrix {
            rows: n as usize,
            cols: d as usize,
            data: std::sync::Arc::new(x_data),
        },
    );
    interp.eval_program(&program).expect("eval ok");

    let m0_x = interp.env.get("m0_x").unwrap().as_f64().unwrap();
    let m0_y = interp.env.get("m0_y").unwrap().as_f64().unwrap();
    let m1_x = interp.env.get("m1_x").unwrap().as_f64().unwrap();
    let m1_y = interp.env.get("m1_y").unwrap().as_f64().unwrap();

    // One cluster should converge to ~(-3, -3) and the other to ~(4, 4)
    let (lo_x, hi_x) = if m0_x < m1_x {
        (m0_x, m1_x)
    } else {
        (m1_x, m0_x)
    };
    let (lo_y, hi_y) = if m0_y < m1_y {
        (m0_y, m1_y)
    } else {
        (m1_y, m0_y)
    };

    assert!(
        (lo_x - (-3.0)).abs() < 0.2,
        "lo_x {} expected near -3.0",
        lo_x
    );
    assert!(
        (lo_y - (-3.0)).abs() < 0.2,
        "lo_y {} expected near -3.0",
        lo_y
    );
    assert!((hi_x - 4.0).abs() < 0.2, "hi_x {} expected near 4.0", hi_x);
    assert!((hi_y - 4.0).abs() < 0.2, "hi_y {} expected near 4.0", hi_y);
}

#[test]
fn test_neko_fit_gmm_recovers_clusters_and_verbs() {
    use ghl_syntax::parse;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use rand_distr::{Distribution, Normal};

    let mut x_vals = Vec::new();
    let mut y_vals = Vec::new();
    let mut rng = StdRng::seed_from_u64(42);
    let d0 = Normal::new(-3.0, 0.5).unwrap();
    let d1 = Normal::new(4.0, 0.5).unwrap();
    for _ in 0..50 {
        x_vals.push(Value::F64(d0.sample(&mut rng)));
        y_vals.push(Value::F64(d0.sample(&mut rng)));
    }
    for _ in 0..50 {
        x_vals.push(Value::F64(d1.sample(&mut rng)));
        y_vals.push(Value::F64(d1.sample(&mut rng)));
    }

    let (frame, na_reasons) = ghl_runtime::polars_bridge::build_dataframe(&[
        ("x".to_string(), x_vals),
        ("y".to_string(), y_vals),
    ])
    .unwrap();

    let ghl_code = r#"
        let model = fit_gmm(df, 2, 50, 0.0001);

        // Test Cockpit rendering
        summary(model);

        // Test Tidyverse / NEKO verbs
        let td = tidy(model);
        let gl = glance(model);
        let aug = augment(model, df);
        let pred = predict(model, df);
        let centers = coef(model);
    "#;

    let program = parse(ghl_code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp
        .env
        .set("df".to_string(), Value::DataFrame { frame, na_reasons });
    interp.eval_program(&program).expect("eval ok");

    // Verify model exists and is GmmFit
    let model_val = interp.env.get("model").expect("model exists");
    assert_eq!(model_val.type_name(), "GmmFit");

    if let Value::GmmFit(ref gmm) = model_val {
        assert_eq!(gmm.k, 2);
        assert_eq!(gmm.dim, 2);
        assert_eq!(gmm.n_obs, 100);
        assert!(gmm.converged);
        assert!(gmm.log_likelihood < 0.0);
        assert!(gmm.aic > 0.0);

        // Check recovered centers
        let (m0_x, m0_y) = (gmm.means[0], gmm.means[1]);
        let (m1_x, m1_y) = (gmm.means[2], gmm.means[3]);
        let (lo_x, hi_x) = if m0_x < m1_x {
            (m0_x, m1_x)
        } else {
            (m1_x, m0_x)
        };
        let (lo_y, hi_y) = if m0_y < m1_y {
            (m0_y, m1_y)
        } else {
            (m1_y, m0_y)
        };

        assert!((lo_x - (-3.0)).abs() < 0.5, "GMM lo_x {} near -3.0", lo_x);
        assert!((lo_y - (-3.0)).abs() < 0.5, "GMM lo_y {} near -3.0", lo_y);
        assert!((hi_x - 4.0).abs() < 0.5, "GMM hi_x {} near 4.0", hi_x);
        assert!((hi_y - 4.0).abs() < 0.5, "GMM hi_y {} near 4.0", hi_y);
    } else {
        panic!("Expected GmmFit, got {:?}", model_val);
    }

    // Verify tidy DataFrame
    let td_val = interp.env.get("td").expect("td exists");
    assert_eq!(td_val.type_name(), "DataFrame");
    if let Value::DataFrame { frame, .. } = td_val {
        assert_eq!(frame.height(), 2);
        assert!(frame.column("component").is_ok());
        assert!(frame.column("weight").is_ok());
        assert!(frame.column("est_size").is_ok());
        assert!(frame.column("mean_x").is_ok());
        assert!(frame.column("mean_y").is_ok());
    }

    // Verify glance DataFrame
    let gl_val = interp.env.get("gl").expect("gl exists");
    assert_eq!(gl_val.type_name(), "DataFrame");
    if let Value::DataFrame { frame, .. } = gl_val {
        assert_eq!(frame.height(), 1);
        assert!(frame.column("log_likelihood").is_ok());
        assert!(frame.column("aic").is_ok());
        assert!(frame.column("bic").is_ok());
        assert!(frame.column("converged").is_ok());
    }

    // Verify augment DataFrame
    let aug_val = interp.env.get("aug").expect("aug exists");
    assert_eq!(aug_val.type_name(), "DataFrame");
    if let Value::DataFrame { frame, .. } = aug_val {
        assert_eq!(frame.height(), 100);
        assert!(frame.column(".cluster").is_ok());
        assert!(frame.column(".probability").is_ok());
        assert!(frame.column("x").is_ok());
        assert!(frame.column("y").is_ok());
    }

    // Verify predict Vector
    let pred_val = interp.env.get("pred").expect("pred exists");
    assert_eq!(pred_val.type_name(), "Vector");
    if let Value::Vector(v) = pred_val {
        assert_eq!(v.len(), 100);
    }

    // Verify coef Matrix
    let centers_val = interp.env.get("centers").expect("centers exists");
    assert_eq!(centers_val.type_name(), "Matrix");
    if let Value::Matrix { rows, cols, .. } = centers_val {
        assert_eq!(rows, 2);
        assert_eq!(cols, 2);
    }
}

#[test]
fn test_neko_fit_gmm_on_matrix_and_error_handling() {
    use ghl_syntax::parse;

    let ghl_code = r#"
        let m = mat [
            -3.0, -3.0 ;
            -2.9, -3.1 ;
            -3.1, -2.9 ;
             4.0,  4.0 ;
             3.9,  4.1 ;
             4.1,  3.9
        ];

        let model = fit_gmm(m, 2, 20);
        let preds = predict(model, m);
    "#;

    let program = parse(ghl_code).expect("syntax ok");
    let mut interp = Interpreter::new();
    interp.eval_program(&program).expect("eval ok");

    let model_val = interp.env.get("model").unwrap();
    assert_eq!(model_val.type_name(), "GmmFit");
    if let Value::GmmFit(ref gmm) = model_val {
        assert_eq!(gmm.k, 2);
        assert_eq!(gmm.dim, 2);
        assert_eq!(gmm.n_obs, 6);
        assert!(gmm.converged);
    }

    let preds_val = interp.env.get("preds").unwrap();
    if let Value::Vector(v) = preds_val {
        assert_eq!(v.len(), 6);
        // First 3 should belong to one cluster, last 3 to the other
        assert_eq!(v[0], v[1]);
        assert_eq!(v[1], v[2]);
        assert_eq!(v[3], v[4]);
        assert_eq!(v[4], v[5]);
        assert_ne!(v[0], v[3]);
    }

    // Test error on k = 0
    let err_code = "let err_m = fit_gmm(m, 0);";
    let err_prog = parse(err_code).expect("syntax ok");
    let mut interp_err = Interpreter::new();
    interp_err
        .env
        .set("m".to_string(), interp.env.get("m").unwrap());
    let res = interp_err.eval_program(&err_prog);
    assert!(res.is_err());
}
