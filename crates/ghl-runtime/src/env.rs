use std::collections::HashMap;
use ghl_diagnostics::Diagnostic;
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEnv {
    pub scopes: Vec<HashMap<String, Value>>,
    pub scope_pool: Vec<HashMap<String, Value>>,
}

static BASE_PRELUDE_ENV: std::sync::LazyLock<RuntimeEnv> = std::sync::LazyLock::new(|| {
    let mut env = RuntimeEnv::new_kernel_prelude();
    let stdlib_files = [
        ("feols.gh", include_str!("stdlib/feols.gh")),
        ("iv.gh", include_str!("stdlib/iv.gh")),
        ("regularized.gh", include_str!("stdlib/regularized.gh")),
        ("sem.gh", include_str!("stdlib/sem.gh")),
        ("stats.gh", include_str!("stdlib/stats.gh")),
        ("prob.gh", include_str!("stdlib/prob.gh")),
    ];
    for (name, src) in stdlib_files {
        match ghl_syntax::parse(src) {
            Ok(prog) => {
                let mut interp = crate::eval::Interpreter::with_env(env.clone());
                if let Err(e) = interp.eval_program(&prog) {
                    eprintln!("Error evaluating stdlib/{name}: {e:?}");
                } else {
                    env = interp.env;
                }
            }
            Err(errs) => {
                eprintln!("Error parsing stdlib/{name}: {:?}", errs);
            }
        }
    }
    env
});

impl RuntimeEnv {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            scope_pool: Vec::new(),
        }
    }

    pub fn with_prelude() -> Self {
        BASE_PRELUDE_ENV.clone()
    }

    pub fn new_kernel_prelude() -> Self {
        let mut env = Self::new();

        // Native function: mean
        env.set("mean".into(), Value::NativeFn(native_mean));

        // Native function: sum
        env.set("sum".into(), Value::NativeFn(native_sum));

        // Native function: std_dev
        env.set("std_dev".into(), Value::NativeFn(native_std_dev));

        // Native function: var
        env.set("var".into(), Value::NativeFn(native_var));

        // Native function: min
        env.set("min".into(), Value::NativeFn(native_min));

        // Native function: max
        env.set("max".into(), Value::NativeFn(native_max));

        // Native function: col (extracts column reference)
        env.set("col".into(), Value::NativeFn(native_col));

        // Native function: filter (for dataframes)
        env.set("filter".into(), Value::NativeFn(native_filter));

        // Native function: purr (diagnostic trace)
        env.set("purr".into(), Value::NativeFn(|args| {
            let msg = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            println!("ฅ(•⩊ •マ [purr] {}", msg);
            Ok(Value::Unit)
        }));

        // Native function: pounce (diagnostic assertion)
        env.set("pounce".into(), Value::NativeFn(|args| {
            let cond = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
            let msg = args.get(1).map(|v| format!("{}", v)).unwrap_or_else(|| "Assertion failed".into());
            if !cond {
                Err(Diagnostic::compute_error("C0999", format!("[pounce failed] {}", msg)))
            } else {
                Ok(Value::Unit)
            }
        }));
        env.set("assert".into(), env.get("pounce").unwrap().clone());

        // Native function: clock_now (high-resolution seconds)
        env.set("clock_now".into(), Value::NativeFn(|_| {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            Ok(Value::F64(now))
        }));

        // Native function: set_telemetry (toggle Cockpit Deck operation telemetry)
        env.set("set_telemetry".into(), Value::NativeFn(|args| {
            let enable = args.first().and_then(|v| v.as_bool()).unwrap_or(true);
            crate::io::set_telemetry_enabled(enable);
            Ok(Value::Bool(enable))
        }));

        // Print and string helpers
        env.set("println".into(), Value::NativeFn(|args| {
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    print!(" ");
                }
                match a {
                    Value::String(s) => print!("{}", s),
                    other => print!("{}", other),
                }
            }
            println!();
            Ok(Value::Unit)
        }));
        env.set("to_string".into(), Value::NativeFn(|args| {
            let val = args.first().map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            }).unwrap_or_default();
            Ok(Value::String(val))
        }));
        env.set("to_int".into(), Value::NativeFn(|args| {
            let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`to_int()` requires 1 argument"))?;
            match val {
                Value::I64(i) => Ok(Value::I64(*i)),
                Value::F64(f) => Ok(Value::I64(*f as i64)),
                Value::String(s) => s.trim().parse::<i64>().map(Value::I64).map_err(|_| {
                    Diagnostic::compute_error("C0202", format!("Cannot parse `{s}` as integer"))
                }),
                Value::Bool(b) => Ok(Value::I64(if *b { 1 } else { 0 })),
                Value::NA(r) => Ok(Value::NA(r.clone())),
                other => Err(Diagnostic::compute_error("C0202", format!("Cannot convert `{}` to integer", other.type_name()))),
            }
        }));
        env.set("int".into(), env.get("to_int").unwrap().clone());

        env.set("to_float".into(), Value::NativeFn(|args| {
            let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`to_float()` requires 1 argument"))?;
            match val {
                Value::F64(f) => Ok(Value::F64(*f)),
                Value::I64(i) => Ok(Value::F64(*i as f64)),
                Value::String(s) => s.trim().parse::<f64>().map(Value::F64).map_err(|_| {
                    Diagnostic::compute_error("C0202", format!("Cannot parse `{s}` as float"))
                }),
                Value::Bool(b) => Ok(Value::F64(if *b { 1.0 } else { 0.0 })),
                Value::NA(r) => Ok(Value::NA(r.clone())),
                other => Err(Diagnostic::compute_error("C0202", format!("Cannot convert `{}` to float", other.type_name()))),
            }
        }));
        env.set("float".into(), env.get("to_float").unwrap().clone());

        env.set("to_bool".into(), Value::NativeFn(|args| {
            let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`to_bool()` requires 1 argument"))?;
            match val {
                Value::Bool(b) => Ok(Value::Bool(*b)),
                Value::I64(i) => Ok(Value::Bool(*i != 0)),
                Value::F64(f) => Ok(Value::Bool(*f != 0.0)),
                Value::String(s) => Ok(Value::Bool(s == "true" || s == "1")),
                Value::NA(r) => Ok(Value::NA(r.clone())),
                other => Err(Diagnostic::compute_error("C0202", format!("Cannot convert `{}` to bool", other.type_name()))),
            }
        }));
        env.set("bool".into(), env.get("to_bool").unwrap().clone());
        env.set("format".into(), Value::NativeFn(|args| {
            if args.is_empty() {
                return Ok(Value::String(String::new()));
            }
            let first_str = match &args[0] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if first_str.contains('{') && args.len() > 1 {
                let mut res = String::new();
                let mut arg_idx = 1;
                let mut chars = first_str.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '{' {
                        let mut spec = String::new();
                        let mut closed = false;
                        for inner in chars.by_ref() {
                            if inner == '}' {
                                closed = true;
                                break;
                            }
                            spec.push(inner);
                        }
                        if closed && arg_idx < args.len() {
                            let arg = &args[arg_idx];
                            arg_idx += 1;
                            if spec.starts_with(":.2") {
                                match arg {
                                    Value::F64(f) => res.push_str(&format!("{:.2}", f)),
                                    Value::I64(i) => res.push_str(&format!("{:.2}", *i as f64)),
                                    other => res.push_str(&other.to_string()),
                                }
                            } else if spec.starts_with(":.4") {
                                match arg {
                                    Value::F64(f) => res.push_str(&format!("{:.4}", f)),
                                    Value::I64(i) => res.push_str(&format!("{:.4}", *i as f64)),
                                    other => res.push_str(&other.to_string()),
                                }
                            } else {
                                match arg {
                                    Value::String(s) => res.push_str(s),
                                    other => res.push_str(&other.to_string()),
                                }
                            }
                        } else {
                            res.push('{');
                            res.push_str(&spec);
                            if closed {
                                res.push('}');
                            }
                        }
                    } else {
                        res.push(ch);
                    }
                }
                Ok(Value::String(res))
            } else {
                let res = args.iter().map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                }).collect::<Vec<_>>().join("");
                Ok(Value::String(res))
            }
        }));
        env.set("panic".into(), Value::NativeFn(|args| {
            let msg = args.first().map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            }).unwrap_or_else(|| "Explicit panic triggered".into());
            Err(Diagnostic::compute_error("C0999", msg))
        }));
        env.set("length".into(), Value::NativeFn(native_len));
        env.set("filter_na".into(), Value::NativeFn(native_filter_na));
        env.set("quantile".into(), Value::NativeFn(native_quantile));
        env.set("append".into(), Value::NativeFn(native_append));
        env.set("sort".into(), Value::NativeFn(native_sort));


        // NEKO Statistical Modeling Verbs
        env.set("fit".into(), Value::NativeFn(native_fit_ols));
        env.set("ols".into(), Value::NativeFn(native_fit_ols));
        env.set("fit_logistic".into(), Value::NativeFn(native_fit_logistic));
        env.set("logistic".into(), Value::NativeFn(native_fit_logistic));
        env.set("fit_poisson".into(), Value::NativeFn(native_fit_poisson));
        env.set("poisson".into(), Value::NativeFn(native_fit_poisson));
        env.set("fit_gmm".into(), Value::NativeFn(native_fit_gmm));
        env.set("gmm".into(), Value::NativeFn(native_fit_gmm));
        env.set("summary".into(), Value::NativeFnCtx(native_summary));
        env.set("tidy".into(), Value::NativeFnCtx(native_tidy));
        env.set("glance".into(), Value::NativeFnCtx(native_glance));
        env.set("augment".into(), Value::NativeFnCtx(native_augment));
        env.set("predict".into(), Value::NativeFnCtx(native_predict));
        env.set("residuals".into(), Value::NativeFn(native_residuals));
        env.set("coef".into(), Value::NativeFn(native_coef));
        env.set("vcov".into(), Value::NativeFnCtx(native_vcov));
        env.set("optim".into(), Value::NativeFnCtx(crate::optim::native_optim));
        env.set("nls".into(), Value::NativeFnCtx(crate::optim::native_nls));
        env.set("sample_cov".into(), Value::NativeFn(crate::sem::native_sample_cov));
        env.set("__sem_fit_core".into(), Value::NativeFn(crate::sem::native_sem_fit_core));
        env.set("sem".into(), Value::NativeFn(crate::sem::native_sem));
        env.set("formula_parts".into(), Value::NativeFn(native_formula_parts));
        env.set("decompose_spec".into(), Value::NativeFn(native_decompose_spec));
        env.set("feols".into(), Value::NativeFn(crate::feols::native_feols));
        env.set("iv_regress".into(), Value::NativeFn(crate::iv::native_iv_regress));
        env.set("iv".into(), Value::NativeFn(crate::iv::native_iv_regress));
        env.set("model_matrix".into(), Value::NativeFn(native_model_matrix));
        env.set("lasso".into(), Value::NativeFn(crate::regularized::native_lasso));
        env.set("ridge".into(), Value::NativeFn(crate::regularized::native_ridge));
        env.set("elastic_net".into(), Value::NativeFn(crate::regularized::native_elastic_net));
        env.set("cv_glmnet".into(), Value::NativeFn(crate::regularized::native_cv_glmnet));

        // Psychometrics & IRT Kernels (Pilar 4)
        env.set("irt_em_quadrature_kernel".into(), Value::NativeFn(native_irt_em_quadrature_kernel));
        env.set("irt_quadrature_grid".into(), Value::NativeFn(native_irt_quadrature_grid));

        // Fused Tensor Operations & Axis Reductions (Roadmap 09 Parte H / Pilar 5)
        env.set("sigmoid_matmul".into(), Value::NativeFn(native_sigmoid_matmul));
        env.set("sigmoid".into(), Value::NativeFn(native_sigmoid));
        env.set("log_sum_exp".into(), Value::NativeFn(native_log_sum_exp));
        env.set("softmax".into(), Value::NativeFn(native_softmax));
        env.set("fused_mul_add".into(), Value::NativeFn(native_fused_mul_add));
        env.set("row_sums".into(), Value::NativeFn(native_row_sums));
        env.set("col_sums".into(), Value::NativeFn(native_col_sums));
        env.set("row_means".into(), Value::NativeFn(native_row_means));
        env.set("col_means".into(), Value::NativeFn(native_col_means));
        env.set("row_maxs".into(), Value::NativeFn(native_row_maxs));
        env.set("col_maxs".into(), Value::NativeFn(native_col_maxs));
        env.set("row_mins".into(), Value::NativeFn(native_row_mins));
        env.set("col_mins".into(), Value::NativeFn(native_col_mins));

        // Grammar of Graphics (RFC 16) Verbs
        env.set("plot".into(), Value::NativeFn(native_plot));
        env.set("ggplot".into(), Value::NativeFn(native_plot));
        env.set("aes".into(), Value::NativeFn(native_aes));
        env.set("geom_point".into(), Value::NativeFn(native_geom_point));
        env.set("geom_line".into(), Value::NativeFn(native_geom_line));
        env.set("geom_smooth".into(), Value::NativeFn(native_geom_smooth));
        env.set("geom_histogram".into(), Value::NativeFn(native_geom_histogram));
        env.set("geom_boxplot".into(), Value::NativeFn(native_geom_boxplot));
        env.set("geom_bar".into(), Value::NativeFn(native_geom_bar));
        env.set("geom_area".into(), Value::NativeFn(native_geom_area));
        env.set("geom_rug".into(), Value::NativeFn(native_geom_rug));
        env.set("facet_wrap".into(), Value::NativeFn(native_facet_wrap));
        env.set("facet_grid".into(), Value::NativeFn(native_facet_grid));
        env.set("labs".into(), Value::NativeFn(native_labs));
        env.set("scale_x_log10".into(), Value::NativeFn(native_scale_x_log10));
        env.set("scale_y_log10".into(), Value::NativeFn(native_scale_y_log10));
        env.set("scale_size".into(), Value::NativeFn(native_scale_size));
        env.set("scale_size_continuous".into(), Value::NativeFn(native_scale_size));
        env.set("show".into(), Value::NativeFn(native_show));
        env.set("save".into(), Value::NativeFn(native_save));
        env.set("ggsave".into(), Value::NativeFn(native_save));
        env.set("to_vega_json".into(), Value::NativeFn(native_to_vega_json));
        env.set("to_svg".into(), Value::NativeFn(native_to_svg));
        env.set("scatter".into(), Value::NativeFn(native_scatter));
        env.set("hist".into(), Value::NativeFn(native_hist));
        env.set("histogram".into(), Value::NativeFn(native_hist));
        env.set("boxplot".into(), Value::NativeFn(native_boxplot));
        env.set("theme".into(), Value::NativeFn(native_theme));
        env.set("theme_minimal".into(), Value::NativeFn(native_theme_minimal));
        env.set("theme_classic".into(), Value::NativeFn(native_theme_classic));
        env.set("theme_dark".into(), Value::NativeFn(native_theme_dark));
        env.set("beside".into(), Value::NativeFn(native_beside));
        env.set("stack".into(), Value::NativeFn(native_stack));

        // Factor & Categorical primitives
        env.set("factor".into(), Value::NativeFn(native_factor));
        env.set("ordered_factor".into(), Value::NativeFn(native_ordered_factor));
        env.set("levels".into(), Value::NativeFn(native_levels));

        // Primitive File I/O
        env.set("read_file".into(), Value::NativeFn(native_read_file));
        env.set("read_lines".into(), Value::NativeFn(native_read_lines));
        env.set("write_file".into(), Value::NativeFn(native_write_file));
        env.set("append_file".into(), Value::NativeFn(native_append_file));
        env.set("file_exists".into(), Value::NativeFn(native_file_exists));
        env.set("sha256".into(), Value::NativeFn(native_sha256));
        env.set("is_vector".into(), Value::NativeFn(native_is_vector));

        // Tabular CSV I/O
        env.set("read_csv".into(), Value::NativeFn(native_read_csv));
        env.set("parse_csv".into(), Value::NativeFn(native_parse_csv));
        env.set("write_csv".into(), Value::NativeFn(native_write_csv));

        // Parquet I/O
        env.set("read_parquet".into(),  Value::NativeFn(native_read_parquet));
        env.set("write_parquet".into(), Value::NativeFn(native_write_parquet));

        // Lazy Execution Engine (TODO.md Fase 2)
        env.set("lazy".into(),         Value::NativeFn(native_lazy));
        env.set("collect".into(),      Value::NativeFn(native_collect));
        env.set("explain".into(),      Value::NativeFn(native_explain));
        env.set("scan_csv".into(),     Value::NativeFn(native_scan_csv));
        env.set("scan_parquet".into(), Value::NativeFn(native_scan_parquet));

        // DataFrame Wrangling Verbs — Tidyverse-style
        env.set("select".into(),   Value::NativeFn(native_select));
        env.set("head".into(),     Value::NativeFn(native_head));
        env.set("tail".into(),     Value::NativeFn(native_tail));
        env.set("mutate".into(),   Value::NativeFn(native_mutate));
        env.set("arrange".into(),  Value::NativeFn(native_arrange));
        env.set("rename".into(),   Value::NativeFn(native_rename));
        env.set("drop".into(),     Value::NativeFn(native_drop));
        env.set("distinct".into(), Value::NativeFn(native_distinct));
        env.set("nrow".into(),     Value::NativeFn(native_nrow));
        env.set("ncol".into(),     Value::NativeFn(native_ncol));
        env.set("colnames".into(), Value::NativeFn(native_colnames));
        env.set("slice".into(),    Value::NativeFn(native_slice));
        env.set("pivot_wider".into(),  Value::NativeFn(native_pivot_wider));
        env.set("pivot_longer".into(), Value::NativeFn(native_pivot_longer));
        env.set("view".into(),         Value::NativeFn(native_view));
        env.set("View".into(),         Value::NativeFn(native_view));

        // Joins
        env.set("inner_join".into(), Value::NativeFn(native_inner_join));
        env.set("left_join".into(),  Value::NativeFn(native_left_join));

        // Grouping / summarizing
        env.set("group_by".into(),  Value::NativeFn(native_group_by));
        env.set("summarize".into(), Value::NativeFn(native_summarize));
        env.set("ungroup".into(),   Value::NativeFn(native_ungroup));

        // Extra aggregations (usable standalone over a Vector, or inside `summarize()`)
        env.set("first".into(),      Value::NativeFn(native_first));
        env.set("last".into(),       Value::NativeFn(native_last));
        env.set("median".into(),     Value::NativeFn(native_median));
        env.set("n_distinct".into(), Value::NativeFn(native_n_distinct));
        env.set("count".into(),      Value::NativeFn(native_count));
        env.set("coalesce".into(),   Value::NativeFn(native_coalesce));

        // Sorting
        env.set("desc".into(), Value::NativeFn(native_desc));

        // Column / row-selection helpers
        env.set("pull".into(),        Value::NativeFn(native_pull));
        env.set("na_reason".into(),   Value::NativeFn(native_na_reason));
        env.set("na_reasons".into(),  Value::NativeFn(native_na_reasons));
        env.set("is_na".into(),       Value::NativeFn(native_is_na));
        env.set("fill_na".into(),     Value::NativeFn(native_fill_na));
        env.set("fill_na_all".into(), Value::NativeFn(native_fill_na_all));
        env.set("impute".into(),      Value::NativeFn(native_impute));
        env.set("filter_na_reason".into(), Value::NativeFn(native_filter_na_reason));

        // Canonical imputation strategies (RFC 01 §7.3 & RFC 02 §2.2)
        env.set("Mean".into(),   Value::String("mean".into()));
        env.set("Median".into(), Value::String("median".into()));
        env.set("Mode".into(),   Value::String("mode".into()));
        env.set("Const".into(),  Value::String("const".into()));

        env.set("glimpse".into(),     Value::NativeFn(native_glimpse));
        env.set("slice_min".into(),   Value::NativeFn(native_slice_min));
        env.set("slice_max".into(),   Value::NativeFn(native_slice_max));
        env.set("sample_n".into(),    Value::NativeFn(native_sample_n));
        env.set("sample_frac".into(), Value::NativeFn(native_sample_frac));

        // Math helpers — scalar + Vector[f64], NaN-safe
        env.set("log".into(),   Value::NativeFn(native_log));
        env.set("ln".into(),    Value::NativeFn(native_log));
        env.set("log2".into(),  Value::NativeFn(native_log2));
        env.set("log10".into(), Value::NativeFn(native_log10));
        env.set("exp".into(),   Value::NativeFn(native_exp));
        env.set("sqrt".into(),  Value::NativeFn(native_sqrt));
        env.set("abs".into(),   Value::NativeFn(native_abs));
        env.set("floor".into(), Value::NativeFn(native_floor));
        env.set("ceil".into(),  Value::NativeFn(native_ceil));
        env.set("sin".into(),   Value::NativeFn(native_sin));
        env.set("cos".into(),   Value::NativeFn(native_cos));
        env.set("round".into(), Value::NativeFn(native_round));
        env.set("pow".into(),   Value::NativeFn(native_pow));
        env.set("clamp".into(), Value::NativeFn(native_clamp));
        env.set("pi".into(), Value::F64(std::f64::consts::PI));
        env.set("e".into(),  Value::F64(std::f64::consts::E));

        // Dense linear algebra (TODO.md Fase 3, faer-backed) -- accessor functions
        // instead of field syntax, since GHL's grammar has no `.field` access.
        env.set("dot".into(),          Value::NativeFn(native_dot));
        env.set("map".into(),          Value::NativeFnCtx(native_map));
        env.set("random_uniform".into(), Value::NativeFn(native_random_uniform));
        env.set("bootstrap_mean".into(), Value::NativeFn(native_bootstrap_mean));
        env.set("random_normal".into(), Value::NativeFn(native_random_normal));
        env.set("random_gamma".into(), Value::NativeFn(native_random_gamma));
        env.set("normal_pdf".into(), Value::NativeFn(native_normal_pdf));
        env.set("normal_cdf".into(), Value::NativeFn(native_normal_cdf));
        env.set("normal_quantile".into(), Value::NativeFn(native_normal_quantile));
        env.set("gamma_pdf".into(), Value::NativeFn(native_gamma_pdf));
        env.set("gamma_cdf".into(), Value::NativeFn(native_gamma_cdf));
        env.set("gamma_quantile".into(), Value::NativeFn(native_gamma_quantile));

        // Statistical distributions: Continuous (Student-t, F, ChiSq, Beta, Uniform, Exp)
        env.set("student_t_pdf".into(), Value::NativeFn(native_student_t_pdf));
        env.set("student_t_cdf".into(), Value::NativeFn(native_student_t_cdf));
        env.set("student_t_quantile".into(), Value::NativeFn(native_student_t_quantile));
        env.set("random_student_t".into(), Value::NativeFn(native_random_student_t));
        env.set("t_pdf".into(), Value::NativeFn(native_student_t_pdf));
        env.set("t_cdf".into(), Value::NativeFn(native_student_t_cdf));
        env.set("t_quantile".into(), Value::NativeFn(native_student_t_quantile));
        env.set("random_t".into(), Value::NativeFn(native_random_student_t));

        env.set("f_dist_pdf".into(), Value::NativeFn(native_f_dist_pdf));
        env.set("f_dist_cdf".into(), Value::NativeFn(native_f_dist_cdf));
        env.set("f_dist_quantile".into(), Value::NativeFn(native_f_dist_quantile));
        env.set("random_f_dist".into(), Value::NativeFn(native_random_f_dist));
        env.set("f_pdf".into(), Value::NativeFn(native_f_dist_pdf));
        env.set("f_cdf".into(), Value::NativeFn(native_f_dist_cdf));
        env.set("f_quantile".into(), Value::NativeFn(native_f_dist_quantile));
        env.set("random_f".into(), Value::NativeFn(native_random_f_dist));

        env.set("chisq_pdf".into(), Value::NativeFn(native_chisq_pdf));
        env.set("chisq_cdf".into(), Value::NativeFn(native_chisq_cdf));
        env.set("chisq_quantile".into(), Value::NativeFn(native_chisq_quantile));
        env.set("random_chisq".into(), Value::NativeFn(native_random_chisq));

        env.set("beta_pdf".into(), Value::NativeFn(native_beta_pdf));
        env.set("beta_cdf".into(), Value::NativeFn(native_beta_cdf));
        env.set("beta_quantile".into(), Value::NativeFn(native_beta_quantile));
        env.set("random_beta".into(), Value::NativeFn(native_random_beta));

        env.set("uniform_pdf".into(), Value::NativeFn(native_uniform_pdf));
        env.set("uniform_cdf".into(), Value::NativeFn(native_uniform_cdf));
        env.set("uniform_quantile".into(), Value::NativeFn(native_uniform_quantile));

        env.set("exp_pdf".into(), Value::NativeFn(native_exp_pdf));
        env.set("exp_cdf".into(), Value::NativeFn(native_exp_cdf));
        env.set("exp_quantile".into(), Value::NativeFn(native_exp_quantile));
        env.set("random_exp".into(), Value::NativeFn(native_random_exp));
        env.set("exponential_pdf".into(), Value::NativeFn(native_exp_pdf));
        env.set("exponential_cdf".into(), Value::NativeFn(native_exp_cdf));
        env.set("exponential_quantile".into(), Value::NativeFn(native_exp_quantile));
        env.set("random_exponential".into(), Value::NativeFn(native_random_exp));

        // Statistical distributions: Discrete (Binomial, Poisson)
        env.set("binomial_pmf".into(), Value::NativeFn(native_binomial_pmf));
        env.set("binomial_cdf".into(), Value::NativeFn(native_binomial_cdf));
        env.set("binomial_quantile".into(), Value::NativeFn(native_binomial_quantile));
        env.set("random_binomial".into(), Value::NativeFn(native_random_binomial));

        env.set("poisson_pmf".into(), Value::NativeFn(native_poisson_pmf));
        env.set("poisson_cdf".into(), Value::NativeFn(native_poisson_cdf));
        env.set("poisson_quantile".into(), Value::NativeFn(native_poisson_quantile));
        env.set("random_poisson".into(), Value::NativeFn(native_random_poisson));
        env.set("qr".into(),           Value::NativeFn(native_qr));
        env.set("qr_q".into(),         Value::NativeFn(native_qr_q));
        env.set("qr_r".into(),         Value::NativeFn(native_qr_r));
        env.set("cholesky".into(),     Value::NativeFn(native_cholesky));
        env.set("svd".into(),          Value::NativeFn(native_svd));
        env.set("svd_u".into(),        Value::NativeFn(native_svd_u));
        env.set("svd_s".into(),        Value::NativeFn(native_svd_s));
        env.set("svd_v".into(),        Value::NativeFn(native_svd_v));
        env.set("eigen".into(),        Value::NativeFn(native_eigen));
        env.set("eigen_values".into(), Value::NativeFn(native_eigen_values));
        env.set("eigen_vectors".into(), Value::NativeFn(native_eigen_vectors));
        env.set("zeros".into(),        Value::NativeFn(native_zeros));
        env.set("matrix".into(),       Value::NativeFn(native_matrix));
        env.set("as_matrix".into(),    Value::NativeFn(native_matrix));
        env.set("len".into(),          Value::NativeFn(native_len));
        env.set("get".into(),          Value::NativeFn(native_get));
        env.set("set".into(),          Value::NativeFn(native_set));
        env.set("get_row".into(),      Value::NativeFn(native_get_row));
        env.set("set_row".into(),      Value::NativeFn(native_set_row));
        env.set("get_col".into(),      Value::NativeFn(native_get_col));
        env.set("transpose".into(),    Value::NativeFn(native_transpose));
        env.set("t".into(),            Value::NativeFn(native_transpose));
        env.set("identity".into(),     Value::NativeFn(native_identity));
        env.set("eye".into(),          Value::NativeFn(native_identity));
        env.set("diag".into(),         Value::NativeFn(native_diag));

        // Vector / window helpers
        env.set("cumsum".into(),    Value::NativeFn(native_cumsum));
        env.set("cumprod".into(),   Value::NativeFn(native_cumprod));
        env.set("cummax".into(),    Value::NativeFn(native_cummax));
        env.set("cummin".into(),    Value::NativeFn(native_cummin));
        env.set("lag".into(),       Value::NativeFn(native_lag));
        env.set("lead".into(),      Value::NativeFn(native_lead));
        env.set("if_else".into(),   Value::NativeFn(native_if_else));
        env.set("between".into(),   Value::NativeFn(native_between));
        env.set("sort_asc".into(),  Value::NativeFn(native_sort_asc));
        env.set("sort_desc".into(), Value::NativeFn(native_sort_desc));
        env.set("rank".into(),      Value::NativeFn(native_rank));

        // String helpers — scalar String + Vector[String], both vectorized
        env.set("str_upper".into(),    Value::NativeFn(native_str_upper));
        env.set("str_lower".into(),    Value::NativeFn(native_str_lower));
        env.set("str_trim".into(),     Value::NativeFn(native_str_trim));
        env.set("str_len".into(),      Value::NativeFn(native_str_len));
        env.set("str_contains".into(), Value::NativeFn(native_str_contains));
        env.set("str_starts".into(),   Value::NativeFn(native_str_starts));
        env.set("str_ends".into(),     Value::NativeFn(native_str_ends));
        env.set("str_replace".into(),  Value::NativeFn(native_str_replace));
        env.set("str_split".into(),    Value::NativeFn(native_str_split));
        env.set("str_pad".into(),      Value::NativeFn(native_str_pad));

        // Print alias (same as println)
        env.set("print".into(), Value::NativeFn(|args| {
            print!("{}", args.first().map(|v| v.to_string()).unwrap_or_default());
            Ok(Value::Unit)
        }));

        // Regional Memory Arenas (RFC 03 §2.2)
        env.set("scope".into(),                  Value::NativeFnCtx(native_arena_scope));
        env.set("arena::scope".into(),           Value::NativeFnCtx(native_arena_scope));
        env.set("alloc_vector".into(),           Value::NativeFn(native_alloc_vector));
        env.set("arena::alloc_vector".into(),    Value::NativeFn(native_alloc_vector));
        env.set("alloc_matrix".into(),           Value::NativeFn(native_alloc_matrix));
        env.set("arena::alloc_matrix".into(),    Value::NativeFn(native_alloc_matrix));
        env.set("reset".into(),                  Value::NativeFn(native_arena_reset));
        env.set("arena::reset".into(),           Value::NativeFn(native_arena_reset));
        env.set("allocated_bytes".into(),        Value::NativeFn(native_arena_allocated_bytes));
        env.set("arena::allocated_bytes".into(), Value::NativeFn(native_arena_allocated_bytes));

        // Automatic Differentiation (RFC 04 §5)
        env.set("grad".into(),                     Value::NativeFnCtx(crate::autodiff::native_autodiff_grad));
        env.set("autodiff::grad".into(),           Value::NativeFnCtx(crate::autodiff::native_autodiff_grad));
        env.set("diff".into(),                     Value::NativeFnCtx(crate::autodiff::native_autodiff_diff));
        env.set("autodiff::diff".into(),           Value::NativeFnCtx(crate::autodiff::native_autodiff_diff));
        env.set("value_and_grad".into(),           Value::NativeFnCtx(crate::autodiff::native_autodiff_value_and_grad));
        env.set("autodiff::value_and_grad".into(), Value::NativeFnCtx(crate::autodiff::native_autodiff_value_and_grad));
        env.set("jacobian".into(),                 Value::NativeFnCtx(crate::autodiff::native_autodiff_jacobian));
        env.set("autodiff::jacobian".into(),       Value::NativeFnCtx(crate::autodiff::native_autodiff_jacobian));

        // Network & HTTP Microservices (RFC 04 §6)
        env.set("http::serve".into(),              Value::NativeFnCtx(crate::net::native_http_serve));
        env.set("http_serve".into(),               Value::NativeFnCtx(crate::net::native_http_serve));
        env.set("http::response".into(),           Value::NativeFn(crate::net::native_http_response));
        env.set("http_response".into(),            Value::NativeFn(crate::net::native_http_response));
        env.set("http::get".into(),                Value::NativeFn(crate::net::native_http_get));
        env.set("http_get".into(),                 Value::NativeFn(crate::net::native_http_get));
        env.set("http::post".into(),               Value::NativeFn(crate::net::native_http_post));
        env.set("http_post".into(),                Value::NativeFn(crate::net::native_http_post));
        env.set("net::tcp_connect".into(),         Value::NativeFn(crate::net::native_tcp_connect));
        env.set("tcp_connect".into(),              Value::NativeFn(crate::net::native_tcp_connect));

        // Parallel Iterators (RFC 05 §2)
        env.set("par_iter".into(),                  Value::NativeFn(crate::concurrency::native_par_iter));
        env.set("concurrency::par_iter".into(),     Value::NativeFn(crate::concurrency::native_par_iter));
        env.set("ParallelIterator::map".into(),     Value::NativeFnCtx(crate::concurrency::native_par_map));
        env.set("ParallelIterator::filter".into(),  Value::NativeFnCtx(crate::concurrency::native_par_filter));
        env.set("ParallelIterator::collect".into(), Value::NativeFn(crate::concurrency::native_par_collect));
        env.set("ParallelIterator::reduce".into(),  Value::NativeFnCtx(crate::concurrency::native_par_reduce));
        env.set("ParallelIterator::sum".into(),     Value::NativeFn(crate::concurrency::native_par_sum));
        env.set("ParallelIterator::count".into(),   Value::NativeFn(crate::concurrency::native_par_count));

        // GPU Acceleration & Shaders (RFC 05 §4)
        env.set("Device".into(),                    Value::NativeFn(crate::gpu::native_device_default_gpu));
        env.set("gpu::Device".into(),               Value::NativeFn(crate::gpu::native_device_default_gpu));
        env.set("Device::default_gpu".into(),       Value::NativeFn(crate::gpu::native_device_default_gpu));
        env.set("Device::cpu_fallback".into(),      Value::NativeFn(crate::gpu::native_device_cpu_fallback));
        env.set("to_gpu".into(),                    Value::NativeFn(crate::gpu::native_to_gpu));
        env.set("gpu::to_gpu".into(),               Value::NativeFn(crate::gpu::native_to_gpu));
        env.set("to_cpu".into(),                    Value::NativeFn(crate::gpu::native_to_cpu));
        env.set("gpu::to_cpu".into(),               Value::NativeFn(crate::gpu::native_to_cpu));
        env.set("GpuMatrix::to_cpu".into(),         Value::NativeFn(crate::gpu::native_gpu_matrix_to_cpu));
        env.set("GpuMatrix::matmul".into(),         Value::NativeFn(crate::gpu::native_gpu_matmul));
        env.set("GpuMatrix::cholesky".into(),       Value::NativeFn(crate::gpu::native_gpu_cholesky));
        env.set("gemm".into(),                      Value::NativeFn(crate::gpu::native_gpu_matmul));
        env.set("gpu::gemm".into(),                 Value::NativeFn(crate::gpu::native_gpu_matmul));
        env.set("GpuVector::to_cpu".into(),         Value::NativeFn(crate::gpu::native_gpu_vector_to_cpu));
        env.set("GpuVector::reduce_sum".into(),     Value::NativeFn(crate::gpu::native_gpu_reduce_sum));
        env.set("reduce_sum".into(),                Value::NativeFn(crate::gpu::native_gpu_reduce_sum));
        env.set("gpu::reduce_sum".into(),           Value::NativeFn(crate::gpu::native_gpu_reduce_sum));
        env.set("PhiloxRng".into(),                 Value::NativeFn(crate::gpu::native_philox_seed));
        env.set("gpu::PhiloxRng".into(),            Value::NativeFn(crate::gpu::native_philox_seed));
        env.set("PhiloxRng::seed".into(),           Value::NativeFn(crate::gpu::native_philox_seed));
        env.set("PhiloxRng::sample_uniform".into(), Value::NativeFn(crate::gpu::native_philox_sample_uniform));
        env.set("PhiloxRng::sample_normal".into(),  Value::NativeFn(crate::gpu::native_philox_sample_normal));

        // Environment inspection & manipulation (R-like)
        env.set("rm".into(),                        Value::NativeFnCtx(native_rm));
        env.set("help".into(),                      Value::NativeFn(native_help));
        env.set("doc".into(),                       Value::NativeFn(native_help));

        // Cockpit Deck Native Primitives (RFC 14)
        env.set("cockpit".into(),                   Value::NativeFn(crate::cockpit::native_cockpit));
        env.set("cockpit_with_badge".into(),        Value::NativeFn(crate::cockpit::native_cockpit_with_badge));
        env.set("cockpit_add_kv".into(),            Value::NativeFn(crate::cockpit::native_cockpit_add_kv));
        env.set("cockpit_add_line".into(),          Value::NativeFn(crate::cockpit::native_cockpit_add_line));
        env.set("cockpit_add_divider".into(),       Value::NativeFn(crate::cockpit::native_cockpit_add_divider));
        env.set("render_cockpit".into(),            Value::NativeFn(crate::cockpit::native_render_cockpit));
        env.set("show_cockpit".into(),              Value::NativeFn(crate::cockpit::native_render_cockpit));
        env.set("cockpit_render".into(),            Value::NativeFn(crate::cockpit::native_render_cockpit));
        env.set("format_cockpit".into(),            Value::NativeFn(crate::cockpit::native_format_cockpit));
        env.set("cockpit_format".into(),            Value::NativeFn(crate::cockpit::native_format_cockpit));
        env.set("sparkline".into(),                 Value::NativeFn(crate::cockpit::native_sparkline));
        env.set("cockpit_sparkline".into(),         Value::NativeFn(crate::cockpit::native_sparkline));

        env
    }

    pub fn push_scope(&mut self) {
        if let Some(mut recycled) = self.scope_pool.pop() {
            recycled.clear();
            self.scopes.push(recycled);
        } else {
            self.scopes.push(HashMap::new());
        }
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            if let Some(mut popped) = self.scopes.pop() {
                popped.clear();
                self.scope_pool.push(popped);
            }
        }
    }

    pub fn set(&mut self, name: String, val: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, val);
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val.clone());
            }
        }
        None
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(val) = scope.get_mut(name) {
                return Some(val);
            }
        }
        None
    }

    pub fn remove(&mut self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(val) = scope.remove(name) {
                return Some(val);
            }
        }
        None
    }

    pub fn assign(&mut self, name: &str, val: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), val);
                return true;
            }
        }
        false
    }
}

use crate::native_core::*;
use crate::native_dataframe_agg::*;
use crate::native_math::*;
use crate::native_vector_ops::*;
use crate::native_string_ops::*;
use crate::native_neko_ops::*;
use crate::native_linalg::*;
use crate::native_plot::*;
use crate::native_io_ops::*;
use crate::native_dataframe_ext::*;
use crate::native_arena_ops::*;
use crate::native_distributions::*;
use crate::native_irt::*;
use crate::native_fused::*;
