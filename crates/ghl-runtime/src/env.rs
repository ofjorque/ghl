use std::collections::HashMap;
use std::sync::Arc;
use ghl_diagnostics::{AestheticMap, Diagnostic, GeomLayer, PlotSpec, RenderCaps};
use ghl_types::ContrastScheme;
use polars_core::prelude::{IdxCa, IdxSize, PlSmallStr, PolarsError};
use rand::{RngExt, SeedableRng};
use rand::distr::Distribution;
use rayon::prelude::*;
use statrs::distribution::{Continuous, ContinuousCDF, Gamma, Normal};
use crate::eval::Interpreter;
use crate::polars_bridge;
use crate::value::Value;
use crate::vector_data::{NumericView, VectorData};

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEnv {
    pub scopes: Vec<HashMap<String, Value>>,
    pub scope_pool: Vec<HashMap<String, Value>>,
}

impl RuntimeEnv {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            scope_pool: Vec::new(),
        }
    }

    pub fn with_prelude() -> Self {
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
            println!("(=^･ω･^=) [purr] {}", msg);
            Ok(Value::Unit)
        }));

        // Native function: pounce (diagnostic assertion)
        env.set("pounce".into(), Value::NativeFn(|args| {
            let cond = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
            let msg = args.get(1).map(|v| format!("{}", v)).unwrap_or_else(|| "Assertion failed".into());
            if !cond {
                Err(Diagnostic::compute_error("C0999", format!("(・`ω´・) [pounce failed] {}", msg)))
            } else {
                Ok(Value::Unit)
            }
        }));

        // Print helpers
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

        // NEKO Statistical Modeling Verbs
        env.set("fit".into(), Value::NativeFn(native_fit_ols));
        env.set("ols".into(), Value::NativeFn(native_fit_ols));
        env.set("fit_logistic".into(), Value::NativeFn(native_fit_logistic));
        env.set("fit_gmm".into(), Value::NativeFn(native_fit_gmm));
        env.set("gmm".into(), Value::NativeFn(native_fit_gmm));
        env.set("summary".into(), Value::NativeFn(native_summary));
        env.set("tidy".into(), Value::NativeFn(native_tidy));
        env.set("glance".into(), Value::NativeFn(native_glance));
        env.set("augment".into(), Value::NativeFn(native_augment));
        env.set("predict".into(), Value::NativeFn(native_predict));
        env.set("residuals".into(), Value::NativeFn(native_residuals));
        env.set("coef".into(), Value::NativeFn(native_coef));
        env.set("vcov".into(), Value::NativeFn(native_vcov));

        // Grammar of Graphics (RFC 09) Verbs
        env.set("plot".into(), Value::NativeFn(native_plot));
        env.set("aes".into(), Value::NativeFn(native_aes));
        env.set("geom_point".into(), Value::NativeFn(native_geom_point));
        env.set("geom_line".into(), Value::NativeFn(native_geom_line));
        env.set("geom_smooth".into(), Value::NativeFn(native_geom_smooth));
        env.set("geom_histogram".into(), Value::NativeFn(native_geom_histogram));
        env.set("geom_boxplot".into(), Value::NativeFn(native_geom_boxplot));
        env.set("geom_bar".into(), Value::NativeFn(native_geom_bar));
        env.set("labs".into(), Value::NativeFn(native_labs));
        env.set("show".into(), Value::NativeFn(native_show));
        env.set("save".into(), Value::NativeFn(native_save));
        env.set("scatter".into(), Value::NativeFn(native_scatter));
        env.set("hist".into(), Value::NativeFn(native_hist));
        env.set("histogram".into(), Value::NativeFn(native_hist));
        env.set("boxplot".into(), Value::NativeFn(native_boxplot));
        env.set("theme_minimal".into(), Value::NativeFn(native_theme_minimal));
        env.set("theme_classic".into(), Value::NativeFn(native_theme_classic));
        env.set("theme_dark".into(), Value::NativeFn(native_theme_dark));

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
        env.set("gamma_pdf".into(), Value::NativeFn(native_gamma_pdf));
        env.set("gamma_cdf".into(), Value::NativeFn(native_gamma_cdf));
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
        env.set("log_sum_exp".into(),  Value::NativeFn(native_log_sum_exp));

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

// Built-in Native Functions

pub(crate) fn native_rm(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    for arg in args {
        match arg {
            Value::String(name) => {
                interp.env.remove(&name);
            }
            Value::ColRef(name) => {
                interp.env.remove(&name);
            }
            other => {
                let s = format!("{}", other);
                interp.env.remove(&s);
            }
        }
    }
    Ok(Value::Unit)
}

pub(crate) fn native_help(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let caps = RenderCaps::rich_terminal(72);
    if let Some(arg) = args.first() {
        let name = match arg {
            Value::String(s) => s.clone(),
            Value::ColRef(s) => s.clone(),
            other => format!("{}", other),
        };
        if let Some(doc) = crate::doc::lookup_doc(&name) {
            println!("{}\n", doc.render(&caps));
            return Ok(Value::Unit);
        } else {
            return Err(Diagnostic::compute_error(
                "C0204",
                format!("No documentation found for `{name}`. Type `help()` to list common functions."),
            ));
        }
    }

    println!("(=^･ω･^=) GHL Help System:");
    println!("Type `help(\"mean\")` or `?mean` to see signature and mathematical formula.");
    println!("Common functions: mean, sum, var, std_dev, median, ols, fit_logistic, fit_gmm,");
    println!("                  summary, predict, residuals, dot, cholesky, qr, svd, eigen,");
    println!("                  filter, select, mutate, group_by, summarize, plot, rm\n");
    Ok(Value::Unit)
}

/// Shared native-reduce path for `mean`/`sum`/`min`/`max`/`median` (TODO.md Fase 3, Track
/// 2, Punto 2): reduces on `VectorData`'s underlying `Column` directly (Arrow-vectorized,
/// no `Vec<Value>` boxing) instead of the boxed loop the pre-Punto-2 versions used.
/// Kleene NA propagation (RFC 02 sect2.4) preserves the *specific* reason of the first NA
/// found (`VectorData::first_na`, an O(1) check in the common no-NA case) -- an explicit
/// decision documented in TODO.md, not an oversight: unlike `summarize()`'s per-group
/// aggregation, a single `Vector` has no "which group's reason wins" ambiguity to hide
/// behind, so there's no reason to throw the specific reason away here.
fn vector_native_reduce(vd: &crate::vector_data::VectorData, kind: &str) -> Result<Value, Diagnostic> {
    if let Some(na) = vd.first_na() {
        return Ok(na);
    }
    let column = vd.column();
    let reduce_err = |e: PolarsError| Diagnostic::compute_error("C0210", format!("`{kind}()` failed: {e}"));
    let scalar = match kind {
        "mean" => column.mean_reduce(),
        "sum" => column.sum_reduce(),
        "min" => column.min_reduce(),
        "max" => column.max_reduce(),
        "median" => column.median_reduce(),
        other => unreachable!("vector_native_reduce: unknown kind `{other}`"),
    }.map_err(reduce_err)?;
    Ok(polars_bridge::any_value_to_plain_value(scalar.value()))
}

pub(crate) fn native_mean(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`mean()` requires at least 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "mean".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::NA(Some("EmptyVector".into())));
            }
            vector_native_reduce(vd, "mean")
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`mean()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

pub(crate) fn native_sum(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`sum()` requires 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "sum".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                // Matches the old boxed loop's `sum = 0.0; has_float = false` starting
                // state: an empty sum is the additive identity, not a missing value.
                return Ok(Value::I64(0));
            }
            vector_native_reduce(vd, "sum")
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`sum()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

pub(crate) fn native_var(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`var()` requires 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "var".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.len() < 2 {
                return Err(Diagnostic::statistical_warning(
                    "SW0002",
                    "Sample variance requires at least N=2 observations (N-1 degrees of freedom)",
                ));
            }
            if let Some(na) = vd.first_na() {
                return Ok(na);
            }
            let reduce_err = |e: PolarsError| Diagnostic::compute_error("C0210", format!("`var()` failed: {e}"));
            let scalar = vd.column().var_reduce(1).map_err(reduce_err)?;
            Ok(polars_bridge::any_value_to_plain_value(scalar.value()))
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`var()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

pub(crate) fn native_std_dev(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if let Some(Value::ColRef(name)) = args.first() {
        return Ok(Value::AggSpec { kind: "std_dev".into(), col: Some(name.clone()) });
    }
    let var_val = native_var(args)?;
    if let Value::F64(v) = var_val {
        Ok(Value::F64(v.sqrt()))
    } else {
        Ok(var_val)
    }
}

pub(crate) fn native_min(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`min()` requires 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "min".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::NA(None));
            }
            vector_native_reduce(vd, "min")
        }
        _ => Err(Diagnostic::compute_error("C0202", "`min()` expects a Vector")),
    }
}

pub(crate) fn native_max(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`max()` requires 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "max".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::NA(None));
            }
            vector_native_reduce(vd, "max")
        }
        _ => Err(Diagnostic::compute_error("C0202", "`max()` expects a Vector")),
    }
}

/// `dot(a, b)` — dot product of two equal-length numeric `Vector`s (TODO.md Fase 3, Track
/// 2, Punto 2, Caso 1.1). Real SIMD via `MatrixOps::dot` (faer's blocked GEMM kernel via
/// `RowRef * ColRef`), not a hand-rolled loop -- `VectorData::as_f64_view` gets to that
/// kernel's input without boxing through `Vec<Value>` either way (zero-copy when both
/// vectors are already `Float64`, single-chunk, no nulls; a cast/rechunk otherwise, still
/// far cheaper than per-cell `Value` boxing). Kleene NA propagation: a NA anywhere in
/// either vector makes the whole dot product NA, preserving that cell's specific reason
/// (`VectorData::first_na`), same rationale as `mean()`/`sum()`/etc. above.
///
/// TODO.md Fase 5, Punto 1: seeds a `Xoshiro256PlusPlus` (already in `ghl-runtime`'s
/// dependencies since Fase 0, unused until now) from an explicit `i64` -- the "PRNG" here
/// is a plain value passed as an argument, not a persistent stateful object GHL scripts
/// hold onto and mutate (`PRNG::seed(seed)`'s aspirational style in Suite 03's own doc):
/// GHL has no variable reassignment (`is_mut` exists on `StmtKind::Let` but nothing ever
/// reads it -- there's no `ExprKind::Assign` at all) and no tuples/destructuring for a
/// `let (val, rng2) = draw(rng)` style either, so a mutating-handle PRNG object isn't
/// expressible today without a much bigger language change than "add reproducibility".
/// An explicit per-call seed sidesteps that entirely, and is naturally
/// parallel-friendly (see `native_bootstrap_mean` below) since nothing is shared/mutated
/// across calls.
fn seeded_rng(seed: i64) -> rand_xoshiro::Xoshiro256PlusPlus {
    rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(seed as u64)
}

/// `random_uniform(n)` — a `Vector[f64]` of `n` values drawn uniformly from `[0, 1)`
/// (TODO.md Fase 3, Track 2, Punto 4, Suite 01's own reference examples). Built straight
/// via `VectorData::from_f64`, no boxing, matching every other numeric-fast-path
/// constructor from Punto 1-3. Optional second argument `random_uniform(n, seed)` (same
/// optional-arity pattern as `round(v, digits)`) makes it bit-for-bit reproducible: same
/// `seed` and `n` always produce the same `Vector`, in any run. Without a `seed`, still
/// `rand::rng()` (thread-local, OS-seeded) -- not reproducible, unchanged from before.
fn native_random_uniform(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_uniform()` requires an integer length argument")
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_uniform()` length must be non-negative, found {n}"),
        ));
    }
    let data: Vec<f64> = match args.get(1).and_then(|v| v.as_i64()) {
        Some(seed) => {
            let mut rng = seeded_rng(seed);
            (0..n).map(|_| rng.random::<f64>()).collect()
        }
        None => {
            let mut rng = rand::rng();
            (0..n).map(|_| rng.random::<f64>()).collect()
        }
    };
    Ok(Value::Vector(VectorData::from_f64(data)))
}

/// Elements per chunk when parallelizing a seeded, per-element distribution sample (Fase
/// 5, Punto 2). Deliberately *not* one `Xoshiro256PlusPlus::jump()` per output element
/// (unlike `bootstrap_mean`, where each replica already does O(base_len) internal work,
/// making an O(n_replicas) sequential jump-setup negligible): here each element is a
/// single scalar draw, so an O(n) sequential setup before an O(n) parallel body would
/// scale badly and cap the achievable speedup as n grows (Amdahl's law) -- an
/// unavoidable trade-off of the same technique, not a hypothetical. Chunking to 1024
/// elements per jump cuts the sequential setup to O(n/1024) while staying far below
/// `PARALLEL_THRESHOLD` (50,000) itself, so there are always several chunks per thread
/// for rayon to balance.
const RNG_CHUNK_SIZE: usize = 1024;

/// Shared by `random_normal`/`random_gamma`: draws `n` i.i.d. samples from `dist`,
/// dispatching on size (`PARALLEL_THRESHOLD`, Fase 4 punto (a)) and on whether `seed` is
/// `Some` exactly like `random_uniform`/`bootstrap_mean` already do. The seeded+parallel
/// path assigns each *chunk* (not each element) its own `Xoshiro256PlusPlus` stream via
/// `.jump()` before the parallel `par_chunks_mut` loop starts, so which chunk lands on
/// which thread never affects the output -- same reproducibility guarantee as
/// `bootstrap_mean`, just chunked instead of per-replica.
fn sample_distribution<D: Distribution<f64> + Sync>(n: usize, dist: &D, seed: Option<i64>) -> Vec<f64> {
    match seed {
        Some(seed) => {
            if n < crate::eval::PARALLEL_THRESHOLD {
                let mut rng = seeded_rng(seed);
                return (0..n).map(|_| dist.sample(&mut rng)).collect();
            }
            let num_chunks = n.div_ceil(RNG_CHUNK_SIZE);
            let mut cursor = seeded_rng(seed);
            let mut chunk_rngs = Vec::with_capacity(num_chunks);
            for _ in 0..num_chunks {
                chunk_rngs.push(cursor.clone());
                cursor.jump();
            }
            let mut out = vec![0.0f64; n];
            out.par_chunks_mut(RNG_CHUNK_SIZE)
                .zip(chunk_rngs.into_par_iter())
                .for_each(|(chunk, mut rng)| {
                    for slot in chunk.iter_mut() {
                        *slot = dist.sample(&mut rng);
                    }
                });
            out
        }
        None => {
            if n < crate::eval::PARALLEL_THRESHOLD {
                let mut rng = rand::rng();
                return (0..n).map(|_| dist.sample(&mut rng)).collect();
            }
            let mut out = vec![0.0f64; n];
            out.par_chunks_mut(RNG_CHUNK_SIZE).for_each(|chunk| {
                let mut rng = rand::rng();
                for slot in chunk.iter_mut() {
                    *slot = dist.sample(&mut rng);
                }
            });
            out
        }
    }
}

/// `random_normal(n, mean, sd)` — a `Vector[f64]` of `n` samples from `Normal(mean, sd)`
/// (TODO.md Fase 5, Punto 2). Optional fourth argument `random_normal(n, mean, sd, seed)`
/// for bit-for-bit reproducibility, same convention as `random_uniform`/`bootstrap_mean`.
/// Backed by `statrs::distribution::Normal` rather than `rand_distr`'s -- see
/// `native_random_gamma` for why the choice actually matters for `Gamma` (not just here,
/// where both crates agree on the parameterization); kept the same crate for both so
/// there's one dependency to reason about instead of two.
fn native_random_normal(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_normal()` requires an integer length as its first argument")
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`random_normal()` length must be non-negative, found {n}")));
    }
    let mean = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_normal()` second argument (mean) must be numeric")
    })?;
    let sd = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_normal()` third argument (sd) must be numeric")
    })?;
    let dist = Normal::new(mean, sd).map_err(|e| Diagnostic::compute_error("C0201", format!("`random_normal()`: {e}")))?;
    let seed = args.get(3).and_then(|v| v.as_i64());
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

/// `random_gamma(n, shape, rate)` — a `Vector[f64]` of `n` samples from `Gamma(shape,
/// rate)`, **shape-rate** parameterization (`rate = 1/scale`), matching
/// `statrs::distribution::Gamma::new` -- deliberately *not* shape-scale
/// (`rand_distr::Gamma`'s convention). This isn't cosmetic: Suite 03's own Gibbs sampler
/// reference computes `Gamma::new(1.0 + n/2.0, 1.0 + ssq/2.0)` to update a precision --
/// the standard Normal-Gamma Bayesian conjugate update, which is shape-rate. Naming the
/// GHL parameter `rate` (not `scale`) keeps this unambiguous; passing a scale value here
/// by mistake would silently produce a statistically different distribution, not an
/// error.
fn native_random_gamma(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_gamma()` requires an integer length as its first argument")
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`random_gamma()` length must be non-negative, found {n}")));
    }
    let shape = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_gamma()` second argument (shape) must be numeric")
    })?;
    let rate = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`random_gamma()` third argument (rate) must be numeric")
    })?;
    let dist = Gamma::new(shape, rate).map_err(|e| Diagnostic::compute_error("C0201", format!("`random_gamma()`: {e}")))?;
    let seed = args.get(3).and_then(|v| v.as_i64());
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

/// `normal_pdf(x, mean, sd)` / `normal_cdf(x, mean, sd)` -- reuse the same
/// `statrs::distribution::Normal` already instantiated for sampling, just calling
/// `.pdf(x)`/`.cdf(x)` (its `Continuous`/`ContinuousCDF` impls) instead of `.sample(...)`.
/// `x` is a scalar, same treatment as `mean`/`sd` -- no broadcasting over a `Vector` in
/// this pass.
fn native_normal_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_pdf()` requires a numeric first argument (x)")
    })?;
    let mean = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_pdf()` second argument (mean) must be numeric")
    })?;
    let sd = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_pdf()` third argument (sd) must be numeric")
    })?;
    let dist = Normal::new(mean, sd).map_err(|e| Diagnostic::compute_error("C0201", format!("`normal_pdf()`: {e}")))?;
    Ok(Value::F64(dist.pdf(x)))
}

fn native_normal_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_cdf()` requires a numeric first argument (x)")
    })?;
    let mean = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_cdf()` second argument (mean) must be numeric")
    })?;
    let sd = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`normal_cdf()` third argument (sd) must be numeric")
    })?;
    let dist = Normal::new(mean, sd).map_err(|e| Diagnostic::compute_error("C0201", format!("`normal_cdf()`: {e}")))?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `gamma_pdf(x, shape, rate)` / `gamma_cdf(x, shape, rate)` -- same shape-rate
/// parameterization as `random_gamma()`, see its doc comment.
fn native_gamma_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_pdf()` requires a numeric first argument (x)")
    })?;
    let shape = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_pdf()` second argument (shape) must be numeric")
    })?;
    let rate = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_pdf()` third argument (rate) must be numeric")
    })?;
    let dist = Gamma::new(shape, rate).map_err(|e| Diagnostic::compute_error("C0201", format!("`gamma_pdf()`: {e}")))?;
    Ok(Value::F64(dist.pdf(x)))
}

fn native_gamma_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_cdf()` requires a numeric first argument (x)")
    })?;
    let shape = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_cdf()` second argument (shape) must be numeric")
    })?;
    let rate = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`gamma_cdf()` third argument (rate) must be numeric")
    })?;
    let dist = Gamma::new(shape, rate).map_err(|e| Diagnostic::compute_error("C0201", format!("`gamma_cdf()`: {e}")))?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `bootstrap_mean(v, n_replicas)` — a `Vector[f64]` of `n_replicas` bootstrap sample
/// means, each computed by resampling `v` with replacement (TODO.md Fase 4, punto (c),
/// Suite 03's Caso 3.3). Deliberately just the mean, not an arbitrary user-supplied
/// estimator: a closure (like `map()`'s) captures `RuntimeEnv` mutably and swaps it into
/// the interpreter per call, which isn't `Send`/`Sync`-safe to hand to rayon without
/// redesigning that mechanism -- the mean, on the other hand, needs nothing but a scalar
/// accumulator per replica, so it parallelizes cleanly as-is.
///
/// "Shared memory without duplicating the base sample" (the case's own wording) comes
/// for free from `VectorData::as_f64_view()` (Punto 2, Fase 3): `base` below is a single
/// borrowed `&[f64]`, taken once, shared read-only across every rayon task -- no replica
/// ever copies the base sample, and no replica materializes its resampled subset either
/// (only a running `f64` sum), so memory use stays O(n + n_replicas), not O(n *
/// n_replicas).
///
/// TODO.md Fase 5, Punto 1: optional fourth-turned-third argument `bootstrap_mean(v,
/// n_replicas, seed)` makes this bit-for-bit reproducible -- and, since replicas run in
/// parallel, *independent of thread count/scheduling* too, which a naive `rand::rng()`
/// swap alone wouldn't give: two runs of the same seeded call could still assign
/// different random streams to different replicas depending on how rayon happens to
/// schedule them. Fixed by assigning each replica its own generator via
/// `Xoshiro256PlusPlus::jump()` *before* the parallel loop starts -- `.jump()` advances
/// the state by the equivalent of 2^128 draws, producing statistically independent,
/// non-overlapping streams (the standard way to parallelize this generator family;
/// naively combining `seed + replica_index` risks correlating neighboring streams
/// instead). Replica `i`'s stream is fixed by `i` alone, not by which thread happens to
/// run it, so 1 thread and 16 threads produce the same `Vector` of means. Without a
/// `seed`, unchanged: each task calls `rand::rng()` same as before.
fn native_bootstrap_mean(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = match args.first() {
        Some(Value::Vector(vd)) => vd,
        Some(other) => {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!("`bootstrap_mean()` expects a Vector as its first argument, found `{}`", other.type_name()),
            ));
        }
        None => return Err(Diagnostic::compute_error("C0201", "`bootstrap_mean()` requires 2 arguments")),
    };
    let n_replicas = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`bootstrap_mean()` requires an integer replica count as its second argument")
    })?;
    if n_replicas < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`bootstrap_mean()` replica count must be non-negative, found {n_replicas}"),
        ));
    }
    if let Some(na) = vd.first_na() {
        // Same collapse-to-NA Kleene convention as dot()/mean(): any NA in the base sample
        // makes the estimator undefined, rather than silently excluding it from the
        // resampling pool (which would change the effective sample size).
        return Ok(na);
    }
    let view = vd.as_f64_view()?;
    let base = view.as_slice();
    let n = base.len();
    if n == 0 {
        return Err(Diagnostic::statistical_error("S0412", "`bootstrap_mean()` requires a non-empty Vector"));
    }

    let n_replicas = n_replicas as usize;
    let means: Vec<f64> = match args.get(2).and_then(|v| v.as_i64()) {
        Some(seed) => {
            // Assign each replica an independent, non-overlapping stream *before*
            // handing them to rayon -- see the doc comment above for why this (rather
            // than a shared/mutated generator, or combining seed+index by hand) is what
            // makes the result independent of thread count and scheduling.
            let mut cursor = seeded_rng(seed);
            let mut sub_rngs = Vec::with_capacity(n_replicas);
            for _ in 0..n_replicas {
                sub_rngs.push(cursor.clone());
                cursor.jump();
            }
            sub_rngs
                .into_par_iter()
                .map(|mut rng| {
                    let mut acc = 0.0;
                    for _ in 0..n {
                        let idx: usize = rng.random_range(0..n);
                        acc += base[idx];
                    }
                    acc / n as f64
                })
                .collect()
        }
        None => (0..n_replicas)
            .into_par_iter()
            .map(|_| {
                let mut rng = rand::rng();
                let mut acc = 0.0;
                for _ in 0..n {
                    let idx: usize = rng.random_range(0..n);
                    acc += base[idx];
                }
                acc / n as f64
            })
            .collect(),
    };

    Ok(Value::Vector(VectorData::from_f64(means)))
}

fn native_dot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let a = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`dot()` requires 2 arguments"))?;
    let b = args.get(1).ok_or_else(|| Diagnostic::compute_error("C0201", "`dot()` requires 2 arguments"))?;

    match (a, b) {
        (Value::Vector(va), Value::Vector(vb)) => {
            if va.len() != vb.len() {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("`dot()`: vectors have different lengths ({} vs {})", va.len(), vb.len()),
                ));
            }
            if let Some(na) = va.first_na().or_else(|| vb.first_na()) {
                return Ok(na);
            }
            let view_a = va.as_f64_view()?;
            let view_b = vb.as_f64_view()?;
            let result = crate::matrix::MatrixOps::dot(view_a.as_slice(), view_b.as_slice())?;
            Ok(Value::F64(result))
        }
        (l, r) => Err(Diagnostic::compute_error(
            "C0202",
            format!("`dot()` expects two Vectors, found `{}` and `{}`", l.type_name(), r.type_name()),
        )),
    }
}

/// `map(x, f)` — applies `f` to every element of `Vector` `x` in a single pass (TODO.md
/// Fase 3, Track 2, Punto 3, Caso 1.4). This is a `NativeFnCtx` (not a plain `NativeFn`)
/// because it has to *call* `f` -- a `Value::Closure` or `Value::NativeFn` -- once per
/// element, and the only thing that knows how to invoke a `Closure` is
/// `Interpreter::call_value`.
///
/// The `Closure` branch deliberately does **not** call `call_value` in a loop: that
/// method's `Closure` arm consumes the closure's captured `RuntimeEnv` on every call
/// (`push_scope`, run the body, discard it), so calling it once per element would clone
/// the whole captured environment once per element too -- a real, avoidable cost, not
/// just overhead. Instead the environment is cloned and `push_scope`'d exactly once
/// before the loop, and each iteration only rebinds the one parameter (`RuntimeEnv::set`,
/// a `HashMap` insert) and does two `mem::replace` swaps (pointer/struct-field moves, not
/// clones) around a single `eval_expr` call.
///
/// The result is collected into one `Vec<Value>` (via `VectorData::from_values`), not a
/// flat `Vec<f64>`: `f` is arbitrary code and may legitimately produce `NA` for some
/// elements (e.g. `log(-1.0)` already does via `map_numeric_fn`), so the output isn't
/// guaranteed all-`f64` up front. The actual fusion this closes is avoiding one whole
/// intermediate `Vector` allocation *per sub-operation* of an expression like
/// `log(1.0 + exp(-abs(xi))) + sin(xi)` (six full-vector passes today if chained via the
/// existing vectorized helpers) -- not eliminating the one, unavoidable final boxing pass.
fn native_map(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = match args.first() {
        Some(Value::Vector(vd)) => vd.clone(),
        Some(other) => {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!("`map()` expects a Vector as its first argument, found `{}`", other.type_name()),
            ));
        }
        None => return Err(Diagnostic::compute_error("C0201", "`map()` requires 2 arguments")),
    };
    let callable = args.get(1).cloned().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`map()` requires 2 arguments")
    })?;

    match callable {
        Value::NativeFn(func) => {
            let mut out = Vec::with_capacity(vd.len());
            for elem in vd.iter() {
                out.push(func(vec![elem.clone()])?);
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        Value::Closure { params, body, env: closure_env } => {
            let param_name = params.first().cloned().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`map()`'s function must take exactly 1 parameter")
            })?;

            // One-time setup -- see the doc comment above for why this isn't `call_value`
            // in a loop.
            let mut call_env = closure_env;
            if let Some(global_scope) = interp.env.scopes.first().cloned() {
                for (k, v) in global_scope {
                    if call_env.get(&k).is_none() {
                        call_env.set(k, v);
                    }
                }
            }
            call_env.push_scope();

            let mut out = Vec::with_capacity(vd.len());
            for elem in vd.iter() {
                call_env.set(param_name.clone(), elem.clone());
                let old_env = std::mem::replace(&mut interp.env, call_env);
                let result = interp.eval_expr(&body);
                call_env = std::mem::replace(&mut interp.env, old_env);
                out.push(result?);
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        other => Err(Diagnostic::compute_error(
            "C0203",
            format!("`map()`'s second argument must be callable, found `{}`", other.type_name()),
        )),
    }
}

fn native_col(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let col_name = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`col()` requires a string column name")
    })?;
    Ok(Value::ColRef(col_name.to_string()))
}

// =========================================================================
// Extra aggregation functions (dual behavior: real compute over a Vector,
// deferred `AggSpec` over a `ColRef` for use inside `summarize()`)
// =========================================================================

pub(crate) fn native_first(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`first()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "first".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => Ok(vd.value_at(0).unwrap_or(Value::NA(None))),
        other => Err(Diagnostic::compute_error("C0202", format!("`first()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_last(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`last()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "last".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => Ok(vd.value_at(vd.len().saturating_sub(1)).unwrap_or(Value::NA(None))),
        other => Err(Diagnostic::compute_error("C0202", format!("`last()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_median(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`median()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "median".into(), col: Some(name.clone()) }),
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::NA(None));
            }
            vector_native_reduce(vd, "median")
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`median()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_n_distinct(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`n_distinct()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "n_distinct".into(), col: Some(name.clone()) }),
        // Delegates straight to polars' own `Column::n_unique()` instead of
        // `format!("{:?}", it)`-ing every element into a `HashSet<String>` -- a real
        // correctness fix, not just a speedup: two NAs with different reasons
        // (`NA(Some("EmptyVector"))` vs `NA(Some("NaN"))`) used to Debug-format
        // differently and count as *two* distinct values, when semantically there's just
        // one kind of "missing" here. `n_unique()` counts null as at most one distinct
        // value, matching how every other statistics library treats it.
        Value::Vector(vd) => {
            let n = vd.column().n_unique().map_err(|e| {
                Diagnostic::compute_error("C0210", format!("internal error computing `n_distinct()`: {e}"))
            })?;
            Ok(Value::I64(n as i64))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`n_distinct()` expects a Vector, found `{}`", other.type_name()))),
    }
}

/// `count()` (zero args, inside `summarize()`) — row-count aggregate.
/// `count(df, col)` — standalone frequency-table verb, returns `DataFrame[col, n]`.
fn native_count(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Ok(Value::AggSpec { kind: "count".into(), col: None });
    }
    let df = &args[0];
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`count(df, col)` requires a column name")
    })?;
    crate::io::df_count(df, &col)
}

fn native_coalesce(args: Vec<Value>) -> Result<Value, Diagnostic> {
    for a in &args {
        if !a.is_na() {
            return Ok(a.clone());
        }
    }
    Ok(Value::NA(None))
}

/// Extracts a column name from a `ColRef`/`String` value, used throughout the
/// verbs below wherever a column argument may be bare (`col_ctx`-resolved) or quoted.
fn col_name_of(v: &Value) -> Option<String> {
    match v {
        Value::ColRef(s) | Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

// =========================================================================
// Grouping / summarizing
// =========================================================================

/// `inner_join(left, right, on1, on2, ...)` — hash join, keeping only matching rows.
fn native_inner_join(args: Vec<Value>) -> Result<Value, Diagnostic> {
    native_join(args, "inner_join", crate::io::df_inner_join)
}

/// `left_join(left, right, on1, on2, ...)` — hash join, keeping every row of `left`.
fn native_left_join(args: Vec<Value>) -> Result<Value, Diagnostic> {
    native_join(args, "left_join", crate::io::df_left_join)
}

fn native_join(
    args: Vec<Value>,
    verb: &str,
    join_fn: fn(&Value, &Value, &[String]) -> Result<Value, Diagnostic>,
) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`{verb}()` requires two DataFrames: `{verb}(left, right, on)`"),
        ));
    }
    let left = &args[0];
    let right = &args[1];

    let mut on = Vec::new();
    for arg in &args[2..] {
        match arg {
            Value::Vector(items) => {
                for it in items.iter() {
                    if let Some(name) = col_name_of(it) {
                        on.push(name);
                    }
                }
            }
            other => {
                if let Some(name) = col_name_of(other) {
                    on.push(name);
                }
            }
        }
    }
    if on.is_empty() {
        return Err(Diagnostic::compute_error("C0201", format!("`{verb}()` requires at least one join column")));
    }

    join_fn(left, right, &on)
}

fn native_group_by(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`group_by()` requires a DataFrame as first argument")
    })?;

    let mut keys = Vec::new();
    for arg in &args[1..] {
        match arg {
            Value::Vector(items) => {
                for it in items.iter() {
                    if let Some(name) = col_name_of(it) {
                        keys.push(name);
                    }
                }
            }
            other => {
                if let Some(name) = col_name_of(other) {
                    keys.push(name);
                }
            }
        }
    }
    if keys.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`group_by()` requires at least one grouping column"));
    }

    crate::io::df_group_by(df, &keys)
}

/// `summarize(gdf, n = count(), mean_x = mean(x))` — consumes the `GroupedDataFrame`,
/// returns a plain `DataFrame`. Bare column names inside the aggregation calls resolve
/// to `Value::ColRef` (see `col_ctx` in `eval.rs`), which `mean`/`sum`/etc. turn into
/// a deferred `Value::AggSpec` rather than computing anything (there's no data yet).
fn native_summarize(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let gdf = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`summarize()` requires a GroupedDataFrame as first argument (did you forget `group_by()`?)",
        )
    })?;

    let mut specs: Vec<(String, String, Option<String>)> = Vec::new();
    for arg in &args[1..] {
        match arg {
            Value::NamedArg(name, value) => match value.as_ref() {
                Value::AggSpec { kind, col } => specs.push((name.clone(), kind.clone(), col.clone())),
                other => {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!(
                            "`summarize()`: `{}` must be an aggregation like `mean(x)` or `count()`, found `{}`",
                            name, other.type_name()
                        ),
                    ));
                }
            },
            other => {
                return Err(Diagnostic::compute_error(
                    "C0201",
                    format!("`summarize()` expects named arguments like `n = count()`, found `{}`", other.type_name()),
                ));
            }
        }
    }
    if specs.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`summarize()` requires at least one named aggregation, e.g. `summarize(n = count())`",
        ));
    }

    crate::io::df_summarize(gdf, &specs)
}

fn native_ungroup(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::GroupedDataFrame { frame, na_reasons, .. }) => {
            Ok(Value::DataFrame { frame: frame.clone(), na_reasons: na_reasons.clone() })
        }
        Some(Value::GroupedLazyFrame { plan, na_reasons, .. }) => {
            Ok(Value::LazyFrame { plan: plan.clone(), na_reasons: na_reasons.clone() })
        }
        Some(df @ Value::DataFrame { .. }) => Ok(df.clone()),
        Some(lf @ Value::LazyFrame { .. }) => Ok(lf.clone()),
        Some(other) => Err(Diagnostic::compute_error(
            "C0201",
            format!("`ungroup()` requires a GroupedDataFrame or GroupedLazyFrame, found `{}`", other.type_name()),
        )),
        None => Err(Diagnostic::compute_error("C0201", "`ungroup()` requires an argument")),
    }
}

// =========================================================================
// Sorting: multi-column `arrange()` + `desc()`
// =========================================================================

/// `desc(col)` — marks a column descending inside `arrange()`.
fn native_desc(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let name = args.first().and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`desc()` requires a column name")
    })?;
    Ok(Value::SortSpec { col: name, desc: true })
}

/// `arrange(df, col1, col2, ...)` — variadic, multi-column sort.
/// Each column may be bare/`ColRef`/string (ascending) or `desc(col)` (descending).
fn native_arrange(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`arrange()` requires a DataFrame as first argument")
    })?;

    let mut specs: Vec<(String, bool)> = Vec::new();
    for arg in &args[1..] {
        match arg {
            Value::Vector(items) => {
                for it in items.iter() {
                    match it {
                        Value::SortSpec { col, desc } => specs.push((col.clone(), *desc)),
                        other => match col_name_of(other) {
                            Some(name) => specs.push((name, false)),
                            None => {
                                return Err(Diagnostic::compute_error(
                                    "C0201",
                                    format!("`arrange()` expects column names or `desc(col)`, found `{}`", other.type_name()),
                                ));
                            }
                        },
                    }
                }
            }
            Value::SortSpec { col, desc } => specs.push((col.clone(), *desc)),
            other => match col_name_of(other) {
                Some(name) => specs.push((name, false)),
                None => {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("`arrange()` expects column names or `desc(col)`, found `{}`", other.type_name()),
                    ));
                }
            },
        }
    }
    if specs.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`arrange()` requires at least one column"));
    }

    crate::io::df_arrange(df, &specs)
}

// =========================================================================
// Column / row-selection helpers (pull, fill_na, glimpse, slice family)
// =========================================================================

fn native_pull(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`pull()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`pull()` requires a column name")
    })?;
    crate::io::df_pull(df, &col)
}

/// `na_reason(x)` — the recorded reason for a single NA value, or `NA` if `x` isn't NA
/// or has no reason attached. Mirrors RFC 02 §2.2's `x.na_reason()`.
fn native_na_reason(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`na_reason()` requires 1 argument"))?;
    match v.na_reason() {
        Some(reason) => Ok(Value::String(reason.to_string())),
        None => Ok(Value::NA(None)),
    }
}

/// `na_reasons(df, col)` — the recorded reasons for a whole column, aligned by row.
fn native_na_reasons(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`na_reasons()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`na_reasons()` requires a column name")
    })?;
    crate::io::df_na_reasons(df, &col)
}

/// `is_na(x)` — RFC 02's `x.is_na()` as a callable function, vectorized over a `Vector`
/// the same way the math/string helpers are (`is_na(pull(df, "col"))` -> `Vector[Bool]`).
fn native_is_na(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`is_na()` requires 1 argument"))?;
    match v {
        // `is_na(col(...))` / `is_na(bare_column)` inside filter()'s argument tree:
        // no data to check yet, stay deferred as a predicate the same way `col(x) > 5`
        // does (see Value::IsNaPredicate).
        Value::ColRef(col) => Ok(Value::IsNaPredicate(col.clone())),
        Value::Vector(items) => Ok(Value::Vector(VectorData::from_values(items.iter().map(|it| Value::Bool(it.is_na())).collect()))),
        other => Ok(Value::Bool(other.is_na())),
    }
}

fn native_fill_na(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`fill_na()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`fill_na()` requires a column name")
    })?;
    let default = args.get(2).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`fill_na()` requires a default value")
    })?;
    crate::io::df_fill_na(df, &col, default)
}

fn native_fill_na_all(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`fill_na_all()` requires a DataFrame"))?;
    let default = args.get(1).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`fill_na_all()` requires a default value")
    })?;
    crate::io::df_fill_na_all(df, default)
}

fn native_glimpse(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`glimpse()` requires a DataFrame"))?;
    crate::io::df_glimpse(df)
}

fn native_slice_min(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`slice_min()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`slice_min()` requires a column name")
    })?;
    let n = args.get(2).and_then(|v| v.as_i64()).unwrap_or(1) as usize;
    crate::io::df_slice_min(df, &col, n)
}

fn native_slice_max(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`slice_max()` requires a DataFrame"))?;
    let col = args.get(1).and_then(col_name_of).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`slice_max()` requires a column name")
    })?;
    let n = args.get(2).and_then(|v| v.as_i64()).unwrap_or(1) as usize;
    crate::io::df_slice_max(df, &col, n)
}

fn native_sample_n(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`sample_n()` requires a DataFrame"))?;
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(1) as usize;
    crate::io::df_sample_n(df, n)
}

fn native_sample_frac(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`sample_frac()` requires a DataFrame"))?;
    let frac = args.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0);
    crate::io::df_sample_frac(df, frac)
}

// =========================================================================
// Math helpers — scalar + `Vector[f64]`, NaN-safe (never panics; NaN -> NA)
// =========================================================================

fn map_numeric_fn(v: &Value, f: impl Fn(f64) -> f64 + Clone + Sync) -> Value {
    match v {
        Value::NA(r) => Value::NA(r.clone()),
        // Fast path: no NA in the input -> read/write straight through `&[f64]`, zero
        // `Value` boxing. Can't use `VectorData::from_f64` for the output, though: `f`
        // can still produce a NaN even from a clean input (`pow(-8.0, 0.5)`), so the
        // output needs `from_f64_opt`'s `Option<f64>` to represent that per-element NA
        // without falling back to the boxed path entirely.
        Value::Vector(vd) if vd.null_count() == 0 => {
            match vd.as_f64_view() {
                Ok(view) => {
                    let base = view.as_slice();
                    let compute = |&x: &f64| -> Option<f64> {
                        let y = f(x);
                        if y.is_nan() { None } else { Some(y) }
                    };
                    let data: Vec<Option<f64>> = if base.len() >= crate::eval::PARALLEL_THRESHOLD {
                        base.par_iter().map(compute).collect()
                    } else {
                        base.iter().map(compute).collect()
                    };
                    Value::Vector(VectorData::from_f64_opt(data))
                }
                // Not actually numeric (e.g. Vector[String]) -- fall through to the
                // generic boxed path below, which already reports `NotNumeric` per
                // element rather than failing the whole call.
                Err(_) => Value::Vector(VectorData::from_values(vd.iter().map(|it| map_numeric_fn(it, f.clone())).collect())),
            }
        }
        Value::Vector(vd) => Value::Vector(VectorData::from_values(vd.iter().map(|it| map_numeric_fn(it, f.clone())).collect())),
        other => match other.as_f64() {
            Some(x) => {
                let y = f(x);
                if y.is_nan() { Value::NA(Some("NaN".into())) } else { Value::F64(y) }
            }
            None => Value::NA(Some(format!("NotNumeric:{}", other.type_name()))),
        },
    }
}

macro_rules! native_math_fn {
    ($name:ident, $f:expr) => {
        fn $name(args: Vec<Value>) -> Result<Value, Diagnostic> {
            let v = args.first().ok_or_else(|| {
                Diagnostic::compute_error("C0201", concat!("`", stringify!($name), "()` requires 1 argument"))
            })?;
            Ok(map_numeric_fn(v, $f))
        }
    };
}

native_math_fn!(native_log, f64::ln);
native_math_fn!(native_log2, f64::log2);
native_math_fn!(native_log10, f64::log10);
native_math_fn!(native_exp, f64::exp);
native_math_fn!(native_sqrt, f64::sqrt);
native_math_fn!(native_abs, f64::abs);
native_math_fn!(native_floor, f64::floor);
native_math_fn!(native_ceil, f64::ceil);
// `sin`/`cos` didn't exist before Punto 3 -- added because Caso 1.4's own reference
// expression (`log(1.0 + exp(-abs(xi))) + sin(xi)`) needs `sin` to actually run
// end-to-end, not a simplified stand-in for it.
native_math_fn!(native_sin, f64::sin);
native_math_fn!(native_cos, f64::cos);

fn native_pow(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let base = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`pow()` requires 2 arguments"))?;
    let exp = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`pow()` second argument must be numeric")
    })?;
    Ok(map_numeric_fn(base, move |x| x.powf(exp)))
}

fn native_round(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`round()` requires at least 1 argument"))?;
    let digits = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
    let factor = 10f64.powi(digits as i32);
    Ok(map_numeric_fn(v, move |x| (x * factor).round() / factor))
}

fn native_clamp(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`clamp()` requires 3 arguments"))?;
    let lo = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`clamp()` second argument (lo) must be numeric")
    })?;
    let hi = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`clamp()` third argument (hi) must be numeric")
    })?;
    Ok(map_numeric_fn(v, move |x| x.max(lo).min(hi)))
}

// =========================================================================
// Vector / window helpers
// =========================================================================

#[allow(dead_code)]
fn as_vector(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::Vector(items) => Some(items),
        _ => None,
    }
}

/// Like `as_vector`, but returns the `VectorData` itself instead of forcing it through
/// `Deref` -- needed by fast paths that must check `null_count()`/`as_f64_view()` *before*
/// paying for a full `Vec<Value>` materialization.
fn as_vector_data(v: &Value) -> Option<&VectorData> {
    match v {
        Value::Vector(vd) => Some(vd),
        _ => None,
    }
}

fn cumulative(items: &[Value], init: f64, combine: impl Fn(f64, f64) -> f64) -> Vec<Value> {
    let mut acc = init;
    let mut out = Vec::with_capacity(items.len());
    let mut poisoned: Option<Option<String>> = None;
    for it in items.iter() {
        if let Some(reason) = &poisoned {
            out.push(Value::NA(reason.clone()));
            continue;
        }
        if let Value::NA(r) = it {
            poisoned = Some(r.clone());
            out.push(Value::NA(r.clone()));
            continue;
        }
        acc = combine(acc, it.as_f64().unwrap_or(0.0));
        out.push(Value::F64(acc));
    }
    out
}

/// `cumsum`/`cumprod`/`cummax`/`cummin`'s dispatcher. Fast path only when there's no NA
/// *anywhere* in the input: a cumulative scan has a strict sequential dependency (each
/// value depends on the one before it), so this deliberately stays single-threaded --
/// parallelizing a scan for real needs a dedicated parallel-scan algorithm, not just
/// `rayon::par_iter()`, and isn't attempted here. With any NA present, falls back
/// unchanged to `cumulative()`'s existing "one NA poisons everything after it" loop --
/// that boxed path already handles NA-mixed input correctly, no change needed there.
fn cumulative_fast(vd: &VectorData, init: f64, combine: impl Fn(f64, f64) -> f64) -> Value {
    if vd.null_count() == 0 {
        if let Ok(view) = vd.as_f64_view() {
            let mut acc = init;
            let data: Vec<f64> = view
                .as_slice()
                .iter()
                .map(|&x| {
                    acc = combine(acc, x);
                    acc
                })
                .collect();
            return Value::Vector(VectorData::from_f64(data));
        }
    }
    Value::Vector(VectorData::from_values(cumulative(vd, init, combine)))
}

fn native_cumsum(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cumsum()` requires a Vector argument")
    })?;
    Ok(cumulative_fast(vd, 0.0, |a, b| a + b))
}

fn native_cumprod(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cumprod()` requires a Vector argument")
    })?;
    Ok(cumulative_fast(vd, 1.0, |a, b| a * b))
}

fn native_cummax(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cummax()` requires a Vector argument")
    })?;
    Ok(cumulative_fast(vd, f64::NEG_INFINITY, f64::max))
}

fn native_cummin(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cummin()` requires a Vector argument")
    })?;
    Ok(cumulative_fast(vd, f64::INFINITY, f64::min))
}

fn native_lag(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`lag()` requires a Vector argument")
    })?;
    let len = vd.len();
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(1).max(0) as usize;
    if n == 0 {
        return Ok(Value::Vector(vd.clone()));
    }
    let shifted_col = vd.column().shift(n as i64);
    let shifted_reasons = vd.na_reasons().shift(n as i64, len);
    Ok(Value::Vector(VectorData::from_column_and_reasons(shifted_col, Arc::new(shifted_reasons))))
}

fn native_lead(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`lead()` requires a Vector argument")
    })?;
    let len = vd.len();
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(1).max(0) as usize;
    if n == 0 {
        return Ok(Value::Vector(vd.clone()));
    }
    let shifted_col = vd.column().shift(-(n as i64));
    let shifted_reasons = vd.na_reasons().shift(-(n as i64), len);
    Ok(Value::Vector(VectorData::from_column_and_reasons(shifted_col, Arc::new(shifted_reasons))))
}

fn broadcast_get(v: &Value, i: usize) -> Value {
    match v {
        Value::Vector(vd) => vd.value_at(i).unwrap_or(Value::NA(None)),
        scalar => scalar.clone(),
    }
}

enum NumericSource<'a> {
    Scalar(f64),
    Slice(&'a [f64]),
    Owned(Vec<f64>),
}

impl<'a> NumericSource<'a> {
    #[inline(always)]
    fn get(&self, i: usize) -> f64 {
        match self {
            NumericSource::Scalar(s) => *s,
            NumericSource::Slice(sl) => sl[i],
            NumericSource::Owned(v) => v[i],
        }
    }
}

fn as_numeric_source(v: &Value, expected_len: usize) -> Option<NumericSource<'_>> {
    match v {
        Value::F64(n) => Some(NumericSource::Scalar(*n)),
        Value::I64(n) => Some(NumericSource::Scalar(*n as f64)),
        Value::Vector(vd) => {
            if vd.len() == expected_len && vd.null_count() == 0 {
                match vd.as_f64_view() {
                    Ok(NumericView::Borrowed(s)) => Some(NumericSource::Slice(s)),
                    Ok(NumericView::Owned(v)) => Some(NumericSource::Owned(v)),
                    Err(_) => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn native_if_else(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let cond = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;
    let yes = args.get(1).ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;
    let no = args.get(2).ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;

    match cond {
        Value::Bool(b) => Ok(if *b { yes.clone() } else { no.clone() }),
        Value::NA(r) => Ok(Value::NA(r.clone())),
        Value::Vector(cond_data) => {
            let len = cond_data.len();
            // Fast-path: numeric yes/no, cond has no NAs and is boolean
            if cond_data.null_count() == 0 {
                if let (Some(yes_src), Some(no_src)) = (as_numeric_source(yes, len), as_numeric_source(no, len)) {
                    if let Ok(bool_ca) = cond_data.column().bool() {
                        let data: Vec<f64> = if len >= crate::eval::PARALLEL_THRESHOLD {
                            (0..len).into_par_iter().map(|i| {
                                if bool_ca.get(i).unwrap_or(false) {
                                    yes_src.get(i)
                                } else {
                                    no_src.get(i)
                                }
                            }).collect()
                        } else {
                            (0..len).map(|i| {
                                if bool_ca.get(i).unwrap_or(false) {
                                    yes_src.get(i)
                                } else {
                                    no_src.get(i)
                                }
                            }).collect()
                        };
                        return Ok(Value::Vector(VectorData::from_f64(data)));
                    }
                }
            }

            // General fallback: preserves arbitrary types and Kleene NA logic without full materialization
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                out.push(match cond_data.value_at(i) {
                    Some(Value::Bool(true)) => broadcast_get(yes, i),
                    Some(Value::Bool(false)) => broadcast_get(no, i),
                    Some(Value::NA(r)) => Value::NA(r),
                    _ => Value::NA(None),
                });
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("`if_else()` condition must be Bool or Vector[Bool], found `{}`", other.type_name()),
        )),
    }
}

fn native_between(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`between()` requires 3 arguments"))?;
    let lo = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`between()` second argument (lo) must be numeric")
    })?;
    let hi = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`between()` third argument (hi) must be numeric")
    })?;

    fn check(x: &Value, lo: f64, hi: f64) -> Value {
        match x {
            Value::NA(r) => Value::NA(r.clone()),
            other => match other.as_f64() {
                Some(n) => Value::Bool(n >= lo && n <= hi),
                None => Value::NA(Some("NotNumeric".into())),
            },
        }
    }

    match v {
        // Fast path: no NA in the input, and the comparison itself can't produce a NaN
        // (unlike map_numeric_fn's transforms) -- so the Bool output is unconditionally
        // safe to build via `from_bool`, no `Option` needed.
        Value::Vector(vd) if vd.null_count() == 0 => {
            if let Ok(view) = vd.as_f64_view() {
                let base = view.as_slice();
                let data: Vec<bool> = if base.len() >= crate::eval::PARALLEL_THRESHOLD {
                    base.par_iter().map(|&x| x >= lo && x <= hi).collect()
                } else {
                    base.iter().map(|&x| x >= lo && x <= hi).collect()
                };
                Ok(Value::Vector(VectorData::from_bool(data)))
            } else {
                Ok(Value::Vector(VectorData::from_values(vd.iter().map(|it| check(it, lo, hi)).collect())))
            }
        }
        Value::Vector(items) => Ok(Value::Vector(VectorData::from_values(items.iter().map(|it| check(it, lo, hi)).collect()))),
        other => Ok(check(other, lo, hi)),
    }
}

fn sort_vector(args: Vec<Value>, desc: bool, fn_name: &str) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", format!("`{}()` requires a Vector argument", fn_name))
    })?;

    // Fast path: no NA, and the Vector is numeric -- get the sort order from a cheap
    // `&[f64]` read, then reorder the *original* Column via `.take()` (the same native
    // gather `io.rs::take_rows` uses for `sample_n`) instead of reconstructing from
    // floats. This is what preserves dtype (`Int64` stays `Int64`) -- `sort_asc`/
    // `sort_desc` return the *original values*, just reordered, so rebuilding from
    // `as_f64_view()`'s f64s would have silently turned every sorted Int64 vector into
    // Float64.
    if vd.null_count() == 0 {
        if let Ok(view) = vd.as_f64_view() {
            let base = view.as_slice();
            let n = base.len();
            let mut idx: Vec<IdxSize> = (0..n as IdxSize).collect();
            let cmp = |&a: &IdxSize, &b: &IdxSize| {
                let ord = base[a as usize].total_cmp(&base[b as usize]);
                if desc { ord.reverse() } else { ord }
            };
            if n >= crate::eval::PARALLEL_THRESHOLD_SORT {
                idx.par_sort_unstable_by(cmp);
            } else {
                idx.sort_unstable_by(cmp);
            }
            let idx_ca = IdxCa::from_vec(PlSmallStr::EMPTY, idx);
            let new_col = vd.column().take(&idx_ca).map_err(|e| {
                Diagnostic::compute_error("C0210", format!("internal error reordering `{}()`'s result: {e}", fn_name))
            })?;
            return Ok(Value::Vector(VectorData::from_column_no_na(new_col)));
        }
    }

    let mut sorted: Vec<Value> = vd.iter().cloned().collect();
    sorted.sort_by(|a, b| {
        let ord = crate::io::compare_values(Some(a), Some(b));
        if desc { ord.reverse() } else { ord }
    });
    Ok(Value::Vector(VectorData::from_values(sorted)))
}

fn native_sort_asc(args: Vec<Value>) -> Result<Value, Diagnostic> {
    sort_vector(args, false, "sort_asc")
}

fn native_sort_desc(args: Vec<Value>) -> Result<Value, Diagnostic> {
    sort_vector(args, true, "sort_desc")
}

/// Integer rank with ties resolved by averaging (matches R's default `rank()`). Output is
/// always `F64` regardless of the input's dtype (it's a rank, not the original values),
/// so there's no dtype-preservation concern here like `sort_vector`'s -- only the *read*
/// side needs to go fast: `as_f64_view()` + `total_cmp` when the input is NA-free and
/// numeric, instead of `compare_values` over a fully materialized `Vec<Value>`.
fn native_rank(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vd = args.first().and_then(as_vector_data).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`rank()` requires a Vector argument")
    })?;

    if vd.null_count() == 0 {
        if let Ok(view) = vd.as_f64_view() {
            let base = view.as_slice();
            let n = base.len();
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by(|&a, &b| base[a].total_cmp(&base[b]));
            let mut ranks = vec![0.0; n];
            let mut i = 0;
            while i < n {
                let mut j = i;
                while j + 1 < n && base[order[j + 1]] == base[order[i]] {
                    j += 1;
                }
                let avg_rank = ((i + j) as f64 / 2.0) + 1.0;
                for slot in order.iter().take(j + 1).skip(i) {
                    ranks[*slot] = avg_rank;
                }
                i = j + 1;
            }
            return Ok(Value::Vector(VectorData::from_f64(ranks)));
        }
    }

    let items: &Vec<Value> = vd;
    let n = items.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| crate::io::compare_values(items.get(a), items.get(b)));

    let mut ranks = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n
            && crate::io::compare_values(items.get(order[j + 1]), items.get(order[i])) == std::cmp::Ordering::Equal
        {
            j += 1;
        }
        let avg_rank = ((i + j) as f64 / 2.0) + 1.0;
        for slot in order.iter().take(j + 1).skip(i) {
            ranks[*slot] = avg_rank;
        }
        i = j + 1;
    }
    Ok(Value::Vector(VectorData::from_f64(ranks)))
}

// =========================================================================
// String helpers — scalar `String` + `Vector[String]`, both vectorized
// =========================================================================

fn map_string_fn(v: &Value, f: impl Fn(&str) -> Value + Clone) -> Value {
    match v {
        Value::String(s) => f(s),
        // Fast *read*: iterate the underlying `StringChunked` directly (`Option<&str>`
        // per cell) instead of recursing through the fully `Deref`-materialized
        // `Vec<Value>`. The *output* still goes through `Vec<Value>`/`from_values`
        // unchanged -- `f: impl Fn(&str) -> Value` can return a `String` (`str_upper`), a
        // `Bool` (`str_contains`) or an `I64` (`str_len`), so there's no single-dtype
        // fast constructor for it the way `from_f64`/`from_bool` cover the numeric side.
        // Real but smaller win than Group A's functions -- this project's benchmarks are
        // numeric, not text, so a full zero-boxing string engine isn't justified here.
        Value::Vector(vd) => match vd.column().str() {
            Ok(ca) => {
                let mut out = Vec::with_capacity(vd.len());
                for i in 0..vd.len() {
                    out.push(match ca.get(i) {
                        Some(s) => f(s),
                        None => vd.value_at(i).unwrap_or(Value::NA(None)),
                    });
                }
                Value::Vector(VectorData::from_values(out))
            }
            Err(_) => Value::Vector(VectorData::from_values(vd.iter().map(|it| map_string_fn(it, f.clone())).collect())),
        },
        Value::NA(r) => Value::NA(r.clone()),
        other => Value::NA(Some(format!("NotString:{}", other.type_name()))),
    }
}

fn native_str_upper(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_upper()` requires 1 argument"))?;
    Ok(map_string_fn(v, |s| Value::String(s.to_uppercase())))
}

fn native_str_lower(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_lower()` requires 1 argument"))?;
    Ok(map_string_fn(v, |s| Value::String(s.to_lowercase())))
}

fn native_str_trim(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_trim()` requires 1 argument"))?;
    Ok(map_string_fn(v, |s| Value::String(s.trim().to_string())))
}

fn native_str_len(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_len()` requires 1 argument"))?;
    Ok(map_string_fn(v, |s| Value::I64(s.chars().count() as i64)))
}

fn native_str_contains(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_contains()` requires 2 arguments"))?;
    let pat = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_contains()` second argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| Value::Bool(s.contains(&pat))))
}

fn native_str_starts(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_starts()` requires 2 arguments"))?;
    let pat = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_starts()` second argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| Value::Bool(s.starts_with(&pat))))
}

fn native_str_ends(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_ends()` requires 2 arguments"))?;
    let pat = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_ends()` second argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| Value::Bool(s.ends_with(&pat))))
}

fn native_str_replace(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_replace()` requires 3 arguments"))?;
    let from = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_replace()` second argument must be a string")
    })?.to_string();
    let to = args.get(2).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_replace()` third argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| Value::String(s.replace(&from, &to))))
}

fn native_str_split(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_split()` requires 2 arguments"))?;
    let sep = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_split()` second argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| {
        Value::Vector(VectorData::from_values(s.split(sep.as_str()).map(|p| Value::String(p.to_string())).collect()))
    }))
}

fn native_str_pad(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_pad()` requires 3 arguments"))?;
    let width = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_pad()` second argument (width) must be an integer")
    })? as usize;
    let pad_char = args.get(2).and_then(|v| v.as_str()).and_then(|s| s.chars().next()).unwrap_or(' ');
    Ok(map_string_fn(v, move |s| {
        let len = s.chars().count();
        if len >= width {
            Value::String(s.to_string())
        } else {
            let padding: String = std::iter::repeat(pad_char).take(width - len).collect();
            Value::String(format!("{}{}", padding, s))
        }
    }))
}


fn native_filter(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`filter()` requires a DataFrame or Vector as its first argument"));
    }

    let target = args[0].clone();
    match target {
        Value::DataFrame { frame, na_reasons } => {
            if args.len() < 2 {
                return Ok(Value::DataFrame { frame, na_reasons });
            }
            let predicate = &args[1];

            // A boolean Vector mask (`filter(df, [true, false, ...])`).
            if let Value::Vector(mask) = predicate {
                let keep_indices: Vec<usize> = mask.iter().enumerate()
                    .filter(|(_, m)| m.as_bool() == Some(true))
                    .map(|(i, _)| i)
                    .collect();
                let (new_frame, new_reasons) = crate::io::take_rows(&frame, &na_reasons, &keep_indices)?;
                return Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons });
            }

            // Any predicate tree: `col(x) > 5`, `is_na(col(y))`, and `!`/`&&`/`||`
            // combinations of those (Suite 02, Caso 2.3) -- vectorized straight against
            // polars columns, no boxing to Vec<Value> and comparing scalar-by-scalar.
            if crate::eval::is_predicate(predicate) {
                return crate::io::df_filter_by_predicate(&Value::DataFrame { frame, na_reasons }, predicate);
            }

            // Anything else is a mistake, not a no-op: GHL doesn't silently ignore a
            // condition it doesn't understand and hand back the DataFrame unfiltered
            // (RFC 00: no silent state) -- it used to, and that was a real bug.
            Err(Diagnostic::compute_error(
                "C0202",
                format!(
                    "`filter()` does not understand the second argument (`{}`) as a \
                     predicate -- expected a column comparison (`col(x) > 5` or bare \
                     `x > 5`), `is_na(col(x))`, a combination of those with `!`/`&&`/`||`, \
                     or a boolean Vector",
                    predicate.type_name()
                ),
            ))
        }
        Value::LazyFrame { .. } => {
            if args.len() < 2 {
                return Ok(target);
            }
            let predicate = &args[1];
            if crate::eval::is_predicate(predicate) {
                return crate::io::df_filter_by_predicate(&target, predicate);
            }
            Err(Diagnostic::compute_error(
                "C0202",
                format!(
                    "`filter()` on LazyFrame does not understand the second argument (`{}`) as a predicate",
                    predicate.type_name()
                ),
            ))
        }
        Value::Vector(vd) => {
            if args.len() < 2 {
                return Ok(Value::Vector(vd));
            }
            let predicate = &args[1];
            if let Value::Vector(mask) = predicate {
                if mask.len() != vd.len() {
                    return Err(Diagnostic::compute_error(
                        "C0202",
                        format!(
                            "`filter()` on Vector: mask length ({}) must match vector length ({})",
                            mask.len(),
                            vd.len()
                        ),
                    ));
                }
                // Arrow fast path: if mask is Boolean ChunkedArray
                if let Ok(ca) = mask.column().bool() {
                    if let Ok(filtered_col) = vd.column().filter(ca) {
                        let mut new_reasons = crate::na_reasons::NaReasonTable::new();
                        if vd.null_count() > 0 {
                            let mut new_row = 0;
                            for old_row in 0..vd.len() {
                                if ca.get(old_row) == Some(true) {
                                    if let Some(r) = vd.na_reasons().get("__ghl_vector__", old_row) {
                                        new_reasons.set("__ghl_vector__", new_row, r.to_string());
                                    }
                                    new_row += 1;
                                }
                            }
                        }
                        return Ok(Value::Vector(VectorData::from_column_and_reasons(
                            filtered_col,
                            std::sync::Arc::new(new_reasons),
                        )));
                    }
                }
                // Fallback for non-Arrow boolean chunked or mixed values
                let keep_indices: Vec<usize> = mask.iter().enumerate()
                    .filter(|(_, m)| m.as_bool() == Some(true))
                    .map(|(i, _)| i)
                    .collect();
                let mut out_vals = Vec::with_capacity(keep_indices.len());
                for &idx in &keep_indices {
                    if let Some(val) = vd.value_at(idx) {
                        out_vals.push(val);
                    }
                }
                return Ok(Value::Vector(VectorData::from_values(out_vals)));
            }
            Err(Diagnostic::compute_error(
                "C0202",
                format!("`filter()` on a Vector expects a boolean Vector mask, found `{}`", predicate.type_name()),
            ))
        }
        other => Ok(other),
    }
}

fn native_zeros(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.len() {
        1 => {
            let n = args[0].as_i64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`zeros(n)` requires an integer length argument")
            })?;
            if n < 0 {
                return Err(Diagnostic::compute_error("C0201", format!("`zeros()` length must be non-negative, found {n}")));
            }
            Ok(Value::Vector(VectorData::from_f64(vec![0.0; n as usize])))
        }
        2 => {
            let r = args[0].as_i64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`zeros(rows, cols)` requires integer dimensions")
            })?;
            let c = args[1].as_i64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`zeros(rows, cols)` requires integer dimensions")
            })?;
            if r < 0 || c < 0 {
                return Err(Diagnostic::compute_error("C0201", format!("`zeros()` dimensions must be non-negative, found ({r}, {c})")));
            }
            let rows = r as usize;
            let cols = c as usize;
            Ok(Value::Matrix {
                rows,
                cols,
                data: std::sync::Arc::new(vec![0.0; rows * cols]),
            })
        }
        _ => Err(Diagnostic::compute_error("C0201", "`zeros()` expects 1 argument (vector length) or 2 arguments (matrix rows, cols)")),
    }
}

fn native_len(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`len()` requires 1 argument")
    })?;
    match val {
        Value::Vector(vd) => Ok(Value::I64(vd.len() as i64)),
        Value::String(s) => Ok(Value::I64(s.chars().count() as i64)),
        Value::DataFrame { frame, .. } => Ok(Value::I64(frame.height() as i64)),
        Value::Matrix { rows, .. } => Ok(Value::I64(*rows as i64)),
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("`len()` expects a Vector, String, DataFrame, or Matrix, found `{}`", other.type_name()),
        )),
    }
}

fn native_get(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.len() {
        2 => {
            let collection = &args[0];
            let index = &args[1];
            match (collection, index) {
                (Value::Vector(vd), Value::I64(idx)) => {
                    let i = *idx;
                    if i < 0 || (i as usize) >= vd.len() {
                        return Err(Diagnostic::compute_error(
                            "C0203",
                            format!("Index out of bounds in `get(vector, idx)`: index {i} for vector of length {}", vd.len()),
                        ));
                    }
                    Ok(vd.value_at(i as usize).unwrap_or(Value::NA(None)))
                }
                (Value::Vector(vd), Value::Vector(indices)) => {
                    // Gather operation: v[indices]
                    if vd.null_count() == 0 {
                        if let Ok(view) = vd.as_f64_view() {
                            let slice = view.as_slice();
                            let mut gathered = Vec::with_capacity(indices.len());
                            for idx_val in indices.iter() {
                                let i = idx_val.as_i64().ok_or_else(|| {
                                    Diagnostic::compute_error("C0202", "`get()` with index vector expects integer indices")
                                })?;
                                if i < 0 || (i as usize) >= slice.len() {
                                    return Err(Diagnostic::compute_error(
                                        "C0203",
                                        format!("Index out of bounds in `get()`: index {i} for vector of length {}", slice.len()),
                                    ));
                                }
                                gathered.push(slice[i as usize]);
                            }
                            return Ok(Value::Vector(VectorData::from_f64(gathered)));
                        }
                    }
                    let mut gathered = Vec::with_capacity(indices.len());
                    for idx_val in indices.iter() {
                        let i = idx_val.as_i64().ok_or_else(|| {
                            Diagnostic::compute_error("C0202", "`get()` with index vector expects integer indices")
                        })?;
                        if i < 0 || (i as usize) >= vd.len() {
                            return Err(Diagnostic::compute_error(
                                "C0203",
                                format!("Index out of bounds in `get()`: index {i} for vector of length {}", vd.len()),
                            ));
                        }
                        gathered.push(vd.value_at(i as usize).unwrap_or(Value::NA(None)));
                    }
                    Ok(Value::Vector(VectorData::from_values(gathered)))
                }
                (Value::String(s), Value::I64(idx)) => {
                    let i = *idx;
                    let len = s.chars().count();
                    if i < 0 || (i as usize) >= len {
                        return Err(Diagnostic::compute_error(
                            "C0203",
                            format!("Index out of bounds in `get(string, idx)`: index {i} for string of length {len}"),
                        ));
                    }
                    let ch = s.chars().nth(i as usize).unwrap();
                    Ok(Value::String(ch.to_string()))
                }
                (Value::Record(map), Value::String(field)) => {
                    map.get(field).cloned().ok_or_else(|| {
                        Diagnostic::compute_error("C0102", format!("Field `{field}` not found in record"))
                    })
                }
                (c, i) => Err(Diagnostic::compute_error(
                    "C0202",
                    format!("`get()` requires a (Vector, index), (String, index), or (Record, field), found (`{}`, `{}`)", c.type_name(), i.type_name()),
                )),
            }
        }
        3 => {
            // Matrix get: get(m, row, col)
            let matrix = &args[0];
            let row = args[1].as_i64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`get(matrix, row, col)` requires integer row index")
            })?;
            let col = args[2].as_i64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`get(matrix, row, col)` requires integer col index")
            })?;
            if let Value::Matrix { rows, cols, data } = matrix {
                let r = row as usize;
                let c = col as usize;
                if row < 0 || r >= *rows || col < 0 || c >= *cols {
                    return Err(Diagnostic::compute_error(
                        "C0203",
                        format!("Matrix index out of bounds in `get(m, {row}, {col})`: matrix is ({rows}x{cols})"),
                    ));
                }
                Ok(Value::F64(data[r * cols + c]))
            } else {
                Err(Diagnostic::compute_error(
                    "C0202",
                    format!("`get()` with 3 arguments requires a Matrix, found `{}`", matrix.type_name()),
                ))
            }
        }
        _ => Err(Diagnostic::compute_error("C0201", "`get()` expects 2 arguments (collection, index) or 3 arguments (matrix, row, col)")),
    }
}

fn native_set(mut args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.len() {
        3 => {
            // set(vector, index, val)
            let val = args.pop().unwrap();
            let idx = args.pop().unwrap().as_i64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`set(vector, index, val)` requires an integer index")
            })?;
            let collection = args.pop().unwrap();
            if let Value::Vector(vd) = collection {
                let i = idx as usize;
                if idx < 0 || i >= vd.len() {
                    return Err(Diagnostic::compute_error(
                        "C0203",
                        format!("Index out of bounds in `set(vector, {idx}, val)`: vector length is {}", vd.len()),
                    ));
                }
                if vd.null_count() == 0 {
                    if let (Ok(view), Some(fval)) = (vd.as_f64_view(), val.as_f64()) {
                        let mut data = view.as_slice().to_vec();
                        data[i] = fval;
                        return Ok(Value::Vector(VectorData::from_f64(data)));
                    }
                }
                let values: Vec<Value> = (0..vd.len())
                    .map(|k| if k == i { val.clone() } else { vd.value_at(k).unwrap_or(Value::NA(None)) })
                    .collect();
                Ok(Value::Vector(VectorData::from_values(values)))
            } else {
                Err(Diagnostic::compute_error(
                    "C0202",
                    format!("`set()` with 3 arguments requires a Vector, found `{}`", collection.type_name()),
                ))
            }
        }
        4 => {
            // set(matrix, row, col, val)
            let val = args.pop().unwrap().as_f64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`set(matrix, row, col, val)` requires numeric value")
            })?;
            let col = args.pop().unwrap().as_i64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`set(matrix, row, col, val)` requires integer col index")
            })?;
            let row = args.pop().unwrap().as_i64().ok_or_else(|| {
                Diagnostic::compute_error("C0201", "`set(matrix, row, col, val)` requires integer row index")
            })?;
            let matrix = args.pop().unwrap();
            if let Value::Matrix { rows, cols, mut data } = matrix {
                let r = row as usize;
                let c = col as usize;
                if row < 0 || r >= rows || col < 0 || c >= cols {
                    return Err(Diagnostic::compute_error(
                        "C0203",
                        format!("Matrix index out of bounds in `set(m, {row}, {col}, val)`: matrix is ({rows}x{cols})"),
                    ));
                }
                // CoW: in-place mutation if unique (strong_count == 1), clone-on-write if shared
                let slice = std::sync::Arc::make_mut(&mut data);
                slice[r * cols + c] = val;
                Ok(Value::Matrix {
                    rows,
                    cols,
                    data,
                })
            } else {
                Err(Diagnostic::compute_error(
                    "C0202",
                    format!("`set()` with 4 arguments requires a Matrix, found `{}`", matrix.type_name()),
                ))
            }
        }
        _ => Err(Diagnostic::compute_error("C0201", "`set()` expects 3 arguments (vector, index, val) or 4 arguments (matrix, row, col, val)")),
    }
}

fn native_get_row(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error("C0201", "`get_row()` requires a Matrix and an integer row index"));
    }
    let matrix = &args[0];
    let row_idx = args[1].as_i64().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`get_row()` requires an integer row index")
    })?;
    if let Value::Matrix { rows, cols, data } = matrix {
        let r = row_idx as usize;
        if row_idx < 0 || r >= *rows {
            return Err(Diagnostic::compute_error(
                "C0203",
                format!("Row index out of bounds in `get_row(m, {row_idx})`: matrix has {rows} rows"),
            ));
        }
        let start = r * cols;
        let slice = &data[start..start + cols];
        Ok(Value::Vector(VectorData::from_f64(slice.to_vec())))
    } else {
        Err(Diagnostic::compute_error(
            "C0202",
            format!("`get_row()` requires a Matrix, found `{}`", matrix.type_name()),
        ))
    }
}

fn native_set_row(mut args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error("C0201", "`set_row()` requires a Matrix, an integer row index, and a Vector"));
    }
    let vec_val = args.pop().unwrap();
    let row_idx = args.pop().unwrap().as_i64().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`set_row()` requires an integer row index")
    })?;
    let matrix = args.pop().unwrap();
    if let (Value::Matrix { rows, cols, mut data }, Value::Vector(vd)) = (matrix, vec_val) {
        let r = row_idx as usize;
        if row_idx < 0 || r >= rows {
            return Err(Diagnostic::compute_error(
                "C0203",
                format!("Row index out of bounds in `set_row(m, {row_idx}, vec)`: matrix has {rows} rows"),
            ));
        }
        if vd.len() != cols {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!("`set_row()` dimension mismatch: matrix has {} columns, but vector has length {}", cols, vd.len()),
            ));
        }
        let start = r * cols;
        // CoW: in-place mutation if unique (strong_count == 1), clone-on-write if shared
        let slice = std::sync::Arc::make_mut(&mut data);
        if vd.null_count() == 0 {
            if let Ok(view) = vd.as_f64_view() {
                slice[start..start + cols].copy_from_slice(view.as_slice());
                return Ok(Value::Matrix {
                    rows,
                    cols,
                    data,
                });
            }
        }
        for (j, item) in vd.iter().enumerate() {
            slice[start + j] = item.as_f64().unwrap_or(0.0);
        }
        Ok(Value::Matrix {
            rows,
            cols,
            data,
        })
    } else {
        Err(Diagnostic::compute_error(
            "C0202",
            format!("`set_row()` requires (Matrix, integer, Vector)"),
        ))
    }
}

fn native_get_col(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error("C0201", "`get_col()` requires a Matrix and an integer column index"));
    }
    let matrix = &args[0];
    let col_idx = args[1].as_i64().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`get_col()` requires an integer column index")
    })?;
    if let Value::Matrix { rows, cols, data } = matrix {
        let c = col_idx as usize;
        if col_idx < 0 || c >= *cols {
            return Err(Diagnostic::compute_error(
                "C0203",
                format!("Column index out of bounds in `get_col(m, {col_idx})`: matrix has {cols} columns"),
            ));
        }
        let mut col_data = Vec::with_capacity(*rows);
        for i in 0..*rows {
            col_data.push(data[i * cols + c]);
        }
        Ok(Value::Vector(VectorData::from_f64(col_data)))
    } else {
        Err(Diagnostic::compute_error(
            "C0202",
            format!("`get_col()` requires a Matrix, found `{}`", matrix.type_name()),
        ))
    }
}

fn native_transpose(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let m = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`transpose()` requires a Matrix")
    })?;
    match m {
        Value::Matrix { rows, cols, data } => {
            let (new_rows, new_cols, new_data) = crate::matrix::MatrixOps::transpose(*rows, *cols, data)?;
            Ok(Value::Matrix {
                rows: new_rows,
                cols: new_cols,
                data: std::sync::Arc::new(new_data),
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("`transpose()` requires a Matrix, found `{}`", other.type_name()),
        )),
    }
}

fn native_identity(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`identity()` requires an integer dimension n")
    })?;
    let n = n_val.as_i64().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`identity()` requires an integer dimension n")
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`identity()` dimension must be non-negative, found {n}")));
    }
    let dim = n as usize;
    let mut data = vec![0.0; dim * dim];
    for i in 0..dim {
        data[i * dim + i] = 1.0;
    }
    Ok(Value::Matrix { rows: dim, cols: dim, data: std::sync::Arc::new(data) })
}

fn native_diag(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let x = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`diag()` requires 1 argument (Vector or Matrix)")
    })?;
    match x {
        Value::Vector(vd) => {
            let n = vd.len();
            let mut data = vec![0.0; n * n];
            if vd.null_count() == 0 {
                if let Ok(view) = vd.as_f64_view() {
                    let slice = view.as_slice();
                    for i in 0..n {
                        data[i * n + i] = slice[i];
                    }
                    return Ok(Value::Matrix { rows: n, cols: n, data: std::sync::Arc::new(data) });
                }
            }
            for i in 0..n {
                let v = vd.value_at(i).and_then(|val| val.as_f64()).unwrap_or(0.0);
                data[i * n + i] = v;
            }
            Ok(Value::Matrix { rows: n, cols: n, data: std::sync::Arc::new(data) })
        }
        Value::Matrix { rows, cols, data } => {
            let n = (*rows).min(*cols);
            let mut diag_vals = Vec::with_capacity(n);
            for i in 0..n {
                diag_vals.push(data[i * cols + i]);
            }
            Ok(Value::Vector(VectorData::from_f64(diag_vals)))
        }
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("`diag()` requires a Vector or Matrix, found `{}`", other.type_name()),
        )),
    }
}

fn native_log_sum_exp(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`log_sum_exp()` requires a Vector")
    })?;
    match v {
        Value::Vector(vd) => {
            if vd.is_empty() {
                return Ok(Value::F64(f64::NEG_INFINITY));
            }
            if vd.null_count() == 0 {
                if let Ok(view) = vd.as_f64_view() {
                    let slice = view.as_slice();
                    let max_val = slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    if max_val.is_infinite() && max_val < 0.0 {
                        return Ok(Value::F64(f64::NEG_INFINITY));
                    }
                    let sum_exp: f64 = slice.iter().map(|&x| (x - max_val).exp()).sum();
                    return Ok(Value::F64(max_val + sum_exp.ln()));
                }
            }
            let mut max_val = f64::NEG_INFINITY;
            let mut vals = Vec::with_capacity(vd.len());
            for i in 0..vd.len() {
                if let Some(x) = vd.value_at(i).and_then(|val| val.as_f64()) {
                    if x > max_val {
                        max_val = x;
                    }
                    vals.push(x);
                }
            }
            if vals.is_empty() || (max_val.is_infinite() && max_val < 0.0) {
                return Ok(Value::F64(f64::NEG_INFINITY));
            }
            let sum_exp: f64 = vals.iter().map(|&x| (x - max_val).exp()).sum();
            Ok(Value::F64(max_val + sum_exp.ln()))
        }
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("`log_sum_exp()` requires a Vector, found `{}`", other.type_name()),
        )),
    }
}


// NEKO Native Invocations

fn native_fit_ols(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`fit()` requires Formula and DataFrame arguments: `fit(model, df)`",
        ));
    }

    let (response, terms) = match &args[0] {
        Value::Formula { response, terms } => (response.clone(), terms.clone()),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `fit()` must be a Formula, found `{}`", other.type_name()),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `fit()` must be a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };
    let blueprint = crate::neko::Blueprint::new(response, terms);
    let model = crate::neko::FittedModel::fit_ols(blueprint, frame, na_reasons)?;
    Ok(Value::ModelFit(Box::new(model)))
}

/// `fit_logistic(y ~ x1 + ... + xP, df)` -- IRLS-fit binary logistic regression
/// (TODO.md Fase 6, Caso 3.2). Same argument shape/validation as `native_fit_ols`
/// above, deliberately not unified into one function with a family argument: the two
/// share `Blueprint`/`Blueprint::bake()` already, but a `Value::GlmFit` result needs
/// its own diagnostics (`glm::FittedGlm`), not OLS's.
fn native_fit_logistic(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`fit_logistic()` requires Formula and DataFrame arguments: `fit_logistic(model, df)`",
        ));
    }

    let (response, terms) = match &args[0] {
        Value::Formula { response, terms } => (response.clone(), terms.clone()),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `fit_logistic()` must be a Formula, found `{}`", other.type_name()),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `fit_logistic()` must be a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };
    let blueprint = crate::neko::Blueprint::new(response, terms);
    let model = crate::glm::FittedGlm::fit_logistic(blueprint, frame, na_reasons)?;
    Ok(Value::GlmFit(Box::new(model)))
}

fn native_fit_gmm(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`fit_gmm(data, k, [max_iter], [tol])` requires at least 2 arguments: data (DataFrame or Matrix) and k (number of clusters)",
        ));
    }
    let k = match args.get(1) {
        Some(Value::I64(n)) if *n > 0 => *n as usize,
        Some(Value::F64(f)) if *f > 0.0 => *f as usize,
        _ => return Err(Diagnostic::statistical_error(
            "S0200",
            "Second argument of `fit_gmm` must be a positive integer k (number of clusters)",
        )),
    };
    let max_iter = match args.get(2) {
        Some(Value::I64(n)) if *n > 0 => *n as usize,
        Some(Value::F64(f)) if *f > 0.0 => *f as usize,
        _ => 100,
    };
    let tol = match args.get(3) {
        Some(Value::F64(f)) if *f > 0.0 => *f,
        _ => 1e-4,
    };

    let model = crate::gmm::FittedGmm::fit(&args[0], k, Some(max_iter), Some(tol))?;
    Ok(Value::GmmFit(Box::new(model)))
}

fn native_summary(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`summary()` requires a ModelFit or printable object")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            println!("{}", m);
            Ok(Value::Unit)
        }
        Value::GlmFit(m) => {
            println!("{}", m);
            Ok(Value::Unit)
        }
        Value::GmmFit(m) => {
            println!("{}", m);
            Ok(Value::Unit)
        }
        other => {
            println!("{}", other);
            Ok(Value::Unit)
        }
    }
}

fn native_tidy(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`tidy()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(m.tidy()),
        Value::GlmFit(m) => Ok(m.tidy()),
        Value::GmmFit(m) => Ok(m.tidy()),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`tidy()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_glance(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`glance()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(m.glance()),
        Value::GlmFit(m) => Ok(m.glance()),
        Value::GmmFit(m) => Ok(m.glance()),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`glance()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_augment(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`augment()` requires ModelFit and DataFrame arguments: `augment(model, df)`",
        ));
    }

    match &args[0] {
        Value::ModelFit(m) => m.augment(&args[1]),
        Value::GlmFit(m) => m.augment(&args[1]),
        Value::GmmFit(m) => m.augment(&args[1]),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("First argument of `augment()` must be a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_predict(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`predict()` requires ModelFit and DataFrame arguments: `predict(model, df)`",
        ));
    }

    match &args[0] {
        Value::ModelFit(m) => m.predict(&args[1]),
        Value::GlmFit(m) => m.predict(&args[1]),
        Value::GmmFit(m) => m.predict(&args[1]),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("First argument of `predict()` must be a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_residuals(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`residuals()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            Ok(Value::Vector(VectorData::from_f64(m.residuals.clone())))
        }
        Value::GlmFit(m) => {
            Ok(Value::Vector(VectorData::from_f64(m.residuals.clone())))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`residuals()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_coef(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`coef()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            Ok(Value::Vector(VectorData::from_f64(m.coefficients.clone())))
        }
        Value::GlmFit(m) => {
            Ok(Value::Vector(VectorData::from_f64(m.coefficients.clone())))
        }
        Value::GmmFit(m) => {
            Ok(Value::Matrix {
                rows: m.k,
                cols: m.dim,
                data: std::sync::Arc::new(m.means.clone()),
            })
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`coef()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn vcov_kind_from_arg(args: &[Value]) -> crate::neko::VcovKind {
    if let Some(k_str) = args.get(1).and_then(|v| v.as_str()) {
        match k_str.to_uppercase().as_str() {
            "HC0" => crate::neko::VcovKind::HC0,
            "HC1" => crate::neko::VcovKind::HC1,
            "HC2" => crate::neko::VcovKind::HC2,
            "HC3" => crate::neko::VcovKind::HC3,
            _ => crate::neko::VcovKind::Classical,
        }
    } else {
        crate::neko::VcovKind::Classical
    }
}

fn native_vcov(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`vcov()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            let kind = vcov_kind_from_arg(&args);
            let p = m.blueprint.term_names.len();
            let vcov_data = m.compute_vcov(kind)?;
            Ok(Value::Matrix { rows: p, cols: p, data: std::sync::Arc::new(vcov_data) })
        }
        Value::GlmFit(m) => {
            let kind = vcov_kind_from_arg(&args);
            let p = m.blueprint.term_names.len();
            let vcov_data = m.compute_vcov(kind)?;
            Ok(Value::Matrix { rows: p, cols: p, data: std::sync::Arc::new(vcov_data) })
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`vcov()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

// =========================================================================
// Dense linear algebra (TODO.md Fase 3, faer-backed)
// =========================================================================

/// `qr(m)` — thin QR decomposition: `m` (rows×cols) = Q (rows×cols) × R (cols×cols).
fn native_qr(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Matrix { rows, cols, data }) => {
            let (q_data, r_data) = crate::matrix::MatrixOps::qr(*rows, *cols, data)?;
            Ok(Value::QrDecomp {
                q: Box::new(Value::Matrix { rows: *rows, cols: *cols, data: std::sync::Arc::new(q_data) }),
                r: Box::new(Value::Matrix { rows: *cols, cols: *cols, data: std::sync::Arc::new(r_data) }),
            })
        }
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`qr()` requires a Matrix, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`qr()` requires a Matrix")),
    }
}

fn native_qr_q(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::QrDecomp { q, .. }) => Ok((**q).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`qr_q()` requires a QrDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`qr_q()` requires a QrDecomp")),
    }
}

fn native_qr_r(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::QrDecomp { r, .. }) => Ok((**r).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`qr_r()` requires a QrDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`qr_r()` requires a QrDecomp")),
    }
}

/// `cholesky(m)` — L such that `m` = L × Lᵀ. Requires a symmetric positive-definite
/// square matrix (checked in `MatrixOps::cholesky`, not silently assumed).
fn native_cholesky(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Matrix { rows, cols, data }) => {
            if rows != cols {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("`cholesky()` requires a square matrix, found ({rows}x{cols})"),
                ));
            }
            let l_data = crate::matrix::MatrixOps::cholesky(*rows, data)?;
            Ok(Value::Matrix { rows: *rows, cols: *cols, data: std::sync::Arc::new(l_data) })
        }
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`cholesky()` requires a Matrix, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`cholesky()` requires a Matrix")),
    }
}

/// `svd(m)` — thin SVD: `m` (rows×cols) = U (rows×k) × diag(S) × Vᵀ (k×cols), k = min(rows,cols).
fn native_svd(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Matrix { rows, cols, data }) => {
            let (u_data, s_values, v_data, k) = crate::matrix::MatrixOps::svd(*rows, *cols, data)?;
            Ok(Value::SvdDecomp {
                u: Box::new(Value::Matrix { rows: *rows, cols: k, data: std::sync::Arc::new(u_data) }),
                s: Box::new(Value::Vector(VectorData::from_f64(s_values))),
                v: Box::new(Value::Matrix { rows: *cols, cols: k, data: std::sync::Arc::new(v_data) }),
            })
        }
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`svd()` requires a Matrix, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`svd()` requires a Matrix")),
    }
}

fn native_svd_u(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::SvdDecomp { u, .. }) => Ok((**u).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`svd_u()` requires an SvdDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`svd_u()` requires an SvdDecomp")),
    }
}

fn native_svd_s(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::SvdDecomp { s, .. }) => Ok((**s).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`svd_s()` requires an SvdDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`svd_s()` requires an SvdDecomp")),
    }
}

fn native_svd_v(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::SvdDecomp { v, .. }) => Ok((**v).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`svd_v()` requires an SvdDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`svd_v()` requires an SvdDecomp")),
    }
}

/// `eigen(m)` — eigendecomposition of a symmetric matrix (see `MatrixOps::eigen_symmetric`
/// for why non-symmetric input is rejected rather than silently misread).
fn native_eigen(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Matrix { rows, cols, data }) => {
            if rows != cols {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("`eigen()` requires a square matrix, found ({rows}x{cols})"),
                ));
            }
            let (values, vectors_data) = crate::matrix::MatrixOps::eigen_symmetric(*rows, data)?;
            Ok(Value::EigenDecomp {
                values: Box::new(Value::Vector(VectorData::from_f64(values))),
                vectors: Box::new(Value::Matrix { rows: *rows, cols: *cols, data: std::sync::Arc::new(vectors_data) }),
            })
        }
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`eigen()` requires a Matrix, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`eigen()` requires a Matrix")),
    }
}

fn native_eigen_values(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::EigenDecomp { values, .. }) => Ok((**values).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`eigen_values()` requires an EigenDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`eigen_values()` requires an EigenDecomp")),
    }
}

fn native_eigen_vectors(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::EigenDecomp { vectors, .. }) => Ok((**vectors).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`eigen_vectors()` requires an EigenDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`eigen_vectors()` requires an EigenDecomp")),
    }
}

// =========================================================================
// Grammar of Graphics (std::plot - RFC 09) Native Functions
// =========================================================================

fn native_aes(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let extract_name = |v: &Value| -> String {
        match v {
            Value::ColRef(s) => s.clone(),
            Value::String(s) => s.clone(),
            other => format!("{other}"),
        }
    };

    let x = args.first().map(extract_name).ok_or_else(|| {
        Diagnostic::compute_error("C0301", "`aes()` requires at least an `x` aesthetic")
    })?;

    let y = args.get(1).map(extract_name);
    let color = args.get(2).map(extract_name);

    Ok(Value::Aesthetic(AestheticMap { x, y, color }))
}

fn native_plot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Ok(Value::Plot(Box::new(PlotSpec::new())));
    }

    let first = &args[0];
    match first {
        Value::DataFrame { frame, na_reasons } => {
            let mut plot_spec = PlotSpec::new();
            let col_f64 = |name: &str| -> Vec<f64> {
                crate::polars_bridge::pull_column_as_values(frame, na_reasons, name)
                    .map(|vals| vals.iter().filter_map(|v| v.as_f64()).collect())
                    .unwrap_or_default()
            };

            if let Some(Value::Aesthetic(aes)) = args.get(1) {
                plot_spec = plot_spec.with_mapping(aes.clone());
                plot_spec = plot_spec.with_x_data(col_f64(&aes.x));
                if let Some(ref y_name) = aes.y {
                    plot_spec.y_data = col_f64(y_name);
                    plot_spec.labels.y_label = Some(y_name.clone());
                    plot_spec.labels.title = Some(format!("Plot: {} vs {}", y_name, aes.x));
                } else {
                    plot_spec.labels.title = Some(format!("Distribution of {}", aes.x));
                }
                plot_spec.labels.x_label = Some(aes.x.clone());
            } else if let Some(x_arg) = args.get(1) {
                let x_name = match x_arg {
                    Value::ColRef(s) | Value::String(s) => s.clone(),
                    other => format!("{other}"),
                };
                plot_spec = plot_spec.with_x_data(col_f64(&x_name));
                plot_spec.labels.x_label = Some(x_name.clone());

                if let Some(y_arg) = args.get(2) {
                    let y_name = match y_arg {
                        Value::ColRef(s) | Value::String(s) => s.clone(),
                        other => format!("{other}"),
                    };
                    plot_spec.y_data = col_f64(&y_name);
                    plot_spec.labels.y_label = Some(y_name.clone());
                    plot_spec.labels.title = Some(format!("Plot: {} vs {}", y_name, x_name));
                } else {
                    plot_spec.labels.title = Some(format!("Distribution of {}", x_name));
                }
            }
            Ok(Value::Plot(Box::new(plot_spec)))
        }
        Value::Vector(items) => {
            let mut plot_spec = PlotSpec::new();
            let xs: Vec<f64> = items.iter().filter_map(|v| v.as_f64()).collect();
            plot_spec = plot_spec.with_x_data(xs);

            if let Some(Value::Vector(y_items)) = args.get(1) {
                let ys: Vec<f64> = y_items.iter().filter_map(|v| v.as_f64()).collect();
                plot_spec.y_data = ys;
                plot_spec.labels.title = Some("Scatter Plot (Y vs X)".into());
            } else {
                plot_spec.labels.title = Some("Vector Distribution".into());
            }
            Ok(Value::Plot(Box::new(plot_spec)))
        }
        Value::Plot(p) => Ok(Value::Plot(p.clone())),
        other => Err(Diagnostic::compute_error(
            "C0302",
            format!("Cannot create plot from `{}`", other.type_name()),
        )),
    }
}

fn native_geom_point(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::point());
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_line(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::line());
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_smooth(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::smooth());
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_histogram(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    let bins = args.get(1).and_then(|v| v.as_i64()).unwrap_or(8) as usize;
    p = p.add_layer(GeomLayer::histogram(bins));
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_boxplot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::boxplot());
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_bar(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::bar());
    Ok(Value::Plot(Box::new(p)))
}

fn native_labs(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => return Err(Diagnostic::compute_error("C0303", "`labs()` requires a Plot as first argument")),
    };

    if let Some(title) = args.get(1).and_then(|v| v.as_str()) {
        p.labels.title = Some(title.to_string());
    }
    if let Some(x_lab) = args.get(2).and_then(|v| v.as_str()) {
        p.labels.x_label = Some(x_lab.to_string());
    }
    if let Some(y_lab) = args.get(3).and_then(|v| v.as_str()) {
        p.labels.y_label = Some(y_lab.to_string());
    }

    Ok(Value::Plot(Box::new(p)))
}

fn native_theme_minimal(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.theme_minimal();
    Ok(Value::Plot(Box::new(p)))
}

fn native_theme_classic(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.theme_classic();
    Ok(Value::Plot(Box::new(p)))
}

fn native_theme_dark(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.theme_dark();
    Ok(Value::Plot(Box::new(p)))
}

fn native_factor(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`factor(x, [levels], [contrast])` requires at least 1 argument"));
    }
    let (items, given_levels) = match &args[0] {
        Value::Vector(v) => (v.clone(), None),
        Value::Factor { levels, indices, .. } => {
            let str_items: Vec<Value> = indices.iter().map(|&i| Value::String(levels[i].clone())).collect();
            (crate::vector_data::VectorData::from_values(str_items), Some(levels.clone()))
        }
        other => {
            return Err(Diagnostic::compute_error("C0201", format!("`factor()` argument must be a Vector, found `{}`", other.type_name())));
        }
    };

    let explicit_levels: Option<Vec<String>> = if let Some(l_arg) = args.get(1) {
        match l_arg {
            Value::Vector(v) => Some(v.iter().map(|val| match val {
                Value::String(s) => s.clone(),
                other => format!("{other}"),
            }).collect()),
            _ => None,
        }
    } else {
        given_levels
    };

    let levels: Vec<String> = if let Some(expl) = explicit_levels {
        expl
    } else {
        let mut set = std::collections::BTreeSet::new();
        for v in items.iter() {
            match v {
                Value::String(s) => { set.insert(s.clone()); }
                Value::NA(_) => {}
                other => { set.insert(format!("{other}")); }
            }
        }
        set.into_iter().collect()
    };

    let mut indices = Vec::with_capacity(items.len());
    for v in items.iter() {
        let s = match v {
            Value::String(s) => s.clone(),
            other => format!("{other}"),
        };
        let idx = levels.iter().position(|l| l == &s).unwrap_or(0);
        indices.push(idx);
    }

    let contrast = if let Some(c_arg) = args.get(2).and_then(|v| v.as_str()) {
        match c_arg.to_lowercase().as_str() {
            "sum" => ContrastScheme::Sum,
            "helmert" => ContrastScheme::Helmert,
            "poly" | "polynomial" => ContrastScheme::Polynomial,
            _ => ContrastScheme::Treatment,
        }
    } else {
        ContrastScheme::Treatment
    };

    Ok(Value::Factor {
        levels,
        indices,
        ordered: false,
        contrast,
    })
}

fn native_ordered_factor(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut factor_val = native_factor(args)?;
    if let Value::Factor { ordered, contrast, .. } = &mut factor_val {
        *ordered = true;
        if *contrast == ContrastScheme::Treatment {
            *contrast = ContrastScheme::Polynomial;
        }
    }
    Ok(factor_val)
}

fn native_levels(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`levels(x)` requires a Factor argument")
    })?;
    match target {
        Value::Factor { levels, .. } => {
            let vals = levels.iter().map(|s| Value::String(s.clone())).collect();
            Ok(Value::Vector(crate::vector_data::VectorData::from_values(vals)))
        }
        other => Err(Diagnostic::compute_error("C0201", format!("`levels()` expects a Factor, found `{}`", other.type_name()))),
    }
}

fn native_show(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0305", "`show()` requires a Plot or Value to render")
    })?;

    let caps = RenderCaps::detect();
    match target {
        Value::Plot(p) => {
            println!("{}\n", p.render(&caps));

            // Check if Positron or GHL plots directory is configured
            if let Some(plots_dir) = std::env::var("POSITRON_PLOTS_DIR")
                .ok()
                .or_else(|| std::env::var("GHL_PLOTS_DIR").ok())
            {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let plot_path = std::path::Path::new(&plots_dir).join(format!("ghl_plot_{}_{}.svg", std::process::id(), timestamp));
                if let Err(e) = p.save_file(&plot_path.to_string_lossy()) {
                    eprintln!("(=^･ω･^=) [Plots Pane] Warning: failed to save SVG plot: {e}");
                }
            }
        }
        other => {
            println!("{}\n", other.render_styled(&caps));
        }
    }
    Ok(Value::Unit)
}

fn native_view(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`view()` requires a DataFrame: `view(df)` or `df |> view()`")
    })?;

    let (nrow, ncol) = match df_val {
        Value::DataFrame { frame, .. } => (frame.height(), frame.width()),
        other => {
            return Err(Diagnostic::compute_error(
                "C0201",
                format!("`view()` requires a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };

    let temp_dir = std::env::temp_dir();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let filename = format!("ghl_data_{}_{}.parquet", std::process::id(), timestamp);
    let file_path = temp_dir.join(&filename);
    let path_str = file_path.to_string_lossy().to_string();

    crate::io::write_parquet_file(df_val, &path_str)?;

    let opened = if let Ok(mut child) = std::process::Command::new("positron").arg(&path_str).spawn() {
        let _ = child.wait();
        true
    } else if let Ok(mut child) = std::process::Command::new("code").arg(&path_str).spawn() {
        let _ = child.wait();
        true
    } else {
        false
    };

    if opened {
        println!("(=^･ω･^=) [Data Explorer] Opened {} ({} rows, {} cols) in Positron Data Explorer", filename, nrow, ncol);
    } else {
        println!("(=^･ω･^=) [Data Explorer] Exported {} ({} rows, {} cols) to:\n   {}", filename, nrow, ncol, path_str);
        println!("   (U・ᴥ・U) Haru ready! Open this Parquet file in Positron Data Explorer");
    }

    Ok(Value::String(path_str))
}

fn native_scatter(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_point(vec![plot_val])
}

fn native_hist(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_histogram(vec![plot_val])
}

fn native_boxplot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_boxplot(vec![plot_val])
}

fn native_save(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0310", "`save()` requires a Plot as first argument")
    })?;

    let path_val = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0311",
            "`save()` requires a destination file path string as second argument (e.g. `save(p, \"output.png\")`)"
        )
    })?;

    match plot_val {
        Value::Plot(p) => {
            p.save_file(path_val).map_err(|e| {
                Diagnostic::compute_error("C0312", format!("Failed to export plot to `{path_val}`: {e}"))
            })?;
            println!("(U・ᴥ・U) Haru successfully exported plot to `{}`", path_val);
            Ok(Value::Unit)
        }
        other => Err(Diagnostic::compute_error(
            "C0310",
            format!("`save()` requires a Plot, found `{}`", other.type_name()),
        )),
    }
}

// =========================================================================
// Primitive and Tabular I/O Native Functions
// =========================================================================

fn native_read_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`read_file()` requires a file path string")
    })?;
    let content = crate::io::read_file(path)?;
    Ok(Value::String(content))
}

fn native_read_lines(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`read_lines()` requires a file path string")
    })?;
    let lines = crate::io::read_lines(path)?;
    let vec_vals = lines.into_iter().map(Value::String).collect();
    Ok(Value::Vector(VectorData::from_values(vec_vals)))
}

fn resolve_path_and_content(s1: &str, s2: &str) -> (String, String) {
    if s1.contains('\n') || s1.contains('\r') || s1.contains(',') {
        (s2.to_string(), s1.to_string())
    } else if s2.contains('\n') || s2.contains('\r') || s2.contains(',') {
        (s1.to_string(), s2.to_string())
    } else if s2.starts_with('/')
        || s2.starts_with("./")
        || s2.starts_with("../")
        || s2.ends_with(".txt")
        || s2.ends_with(".csv")
        || s2.ends_with(".tsv")
        || s2.ends_with(".json")
        || s2.ends_with(".gh")
        || s2.ends_with(".log")
    {
        (s2.to_string(), s1.to_string())
    } else {
        (s1.to_string(), s2.to_string())
    }
}

fn native_write_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0402",
            "`write_file()` requires destination path and content: `write_file(path, content)` or `content |> write_file(path)`",
        ));
    }

    let (path, content): (String, String) = if let (Some(s1), Some(s2)) = (args[0].as_str(), args[1].as_str()) {
        resolve_path_and_content(s1, s2)
    } else if let Some(p) = args[1].as_str() {
        (p.to_string(), args[0].to_string())
    } else if let Some(p) = args[0].as_str() {
        (p.to_string(), args[1].to_string())
    } else {
        return Err(Diagnostic::compute_error("C0402", "`write_file()` requires string arguments"));
    };

    crate::io::write_file(&path, &content)?;
    Ok(Value::Unit)
}

fn native_append_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0402",
            "`append_file()` requires destination path and content: `append_file(path, content)` or `content |> append_file(path)`",
        ));
    }

    let (path, content): (String, String) = if let (Some(s1), Some(s2)) = (args[0].as_str(), args[1].as_str()) {
        resolve_path_and_content(s1, s2)
    } else if let Some(p) = args[1].as_str() {
        (p.to_string(), args[0].to_string())
    } else if let Some(p) = args[0].as_str() {
        (p.to_string(), args[1].to_string())
    } else {
        return Err(Diagnostic::compute_error("C0402", "`append_file()` requires string arguments"));
    };

    crate::io::append_file(&path, &content)?;
    Ok(Value::Unit)
}

fn native_file_exists(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`file_exists()` requires a file path string")
    })?;
    Ok(Value::Bool(crate::io::file_exists(path)))
}

fn native_read_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0403", "`read_csv()` requires a file path string")
    })?;
    let delim = args.get(1).and_then(|v| v.as_str()).and_then(|s| s.chars().next());
    crate::io::read_csv_file(path, delim)
}

fn native_parse_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let content = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0403", "`parse_csv()` requires a CSV text string")
    })?;
    let delim = args.get(1).and_then(|v| v.as_str()).and_then(|s| s.chars().next());
    crate::io::parse_csv_string(content, delim)
}

fn native_write_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0404",
            "`write_csv()` requires DataFrame and path arguments: `write_csv(df, \"out.csv\")` or `df |> write_csv(\"out.csv\")`",
        ));
    }

    let (df, path, delim_idx) = match (&args[0], &args[1]) {
        (Value::DataFrame { .. }, _) => (&args[0], args[1].as_str(), 2),
        (_, Value::DataFrame { .. }) => (&args[1], args[0].as_str(), 2),
        _ => {
            return Err(Diagnostic::compute_error(
                "C0404",
                "`write_csv()` requires a DataFrame argument",
            ));
        }
    };

    let p = path.ok_or_else(|| {
        Diagnostic::compute_error("C0404", "`write_csv()` requires a string destination path")
    })?;

    let delim = args.get(delim_idx).and_then(|v| v.as_str()).and_then(|s| s.chars().next());
    crate::io::write_csv_file(df, p, delim)?;
    Ok(Value::Unit)
}

fn native_read_parquet(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0405", "`read_parquet()` requires a file path string")
    })?;
    crate::io::read_parquet_file(path)
}

fn native_write_parquet(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0405",
            "`write_parquet()` requires DataFrame and path arguments: `write_parquet(df, \"out.parquet\")` or `df |> write_parquet(\"out.parquet\")`",
        ));
    }

    let (df, path) = match (&args[0], &args[1]) {
        (Value::DataFrame { .. }, _) => (&args[0], args[1].as_str()),
        (_, Value::DataFrame { .. }) => (&args[1], args[0].as_str()),
        _ => {
            return Err(Diagnostic::compute_error("C0405", "`write_parquet()` requires a DataFrame argument"));
        }
    };
    let p = path.ok_or_else(|| {
        Diagnostic::compute_error("C0405", "`write_parquet()` requires a string destination path")
    })?;

    crate::io::write_parquet_file(df, p)?;
    Ok(Value::Unit)
}

fn native_lazy(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`lazy()` requires a DataFrame")
    })?;
    crate::io::df_lazy(df)
}

fn native_collect(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let lf = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`collect()` requires a LazyFrame")
    })?;
    crate::io::df_collect(lf)
}

fn native_explain(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let lf = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`explain()` requires a LazyFrame")
    })?;
    let opt = args.get(1).and_then(|v| v.as_bool()).unwrap_or(true);
    crate::io::df_explain(lf, opt)
}

fn native_scan_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0403", "`scan_csv()` requires a file path string")
    })?;
    crate::io::scan_csv_file(path)
}

fn native_scan_parquet(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0405", "`scan_parquet()` requires a file path string")
    })?;
    crate::io::scan_parquet_file(path)
}

fn native_select(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`select()` requires a DataFrame"));
    }

    let df = &args[0];
    let mut cols = Vec::new();

    for arg in &args[1..] {
        match arg {
            Value::Vector(items) => {
                for it in items.iter() {
                    match it {
                        Value::ColRef(s) | Value::String(s) => cols.push(s.clone()),
                        other => cols.push(format!("{other}")),
                    }
                }
            }
            Value::ColRef(s) | Value::String(s) => cols.push(s.clone()),
            other => cols.push(format!("{other}")),
        }
    }

    crate::io::df_select(df, &cols)
}

fn native_head(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`head()` requires a DataFrame")
    })?;
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    crate::io::df_head(df, n)
}

fn native_tail(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`tail()` requires a DataFrame")
    })?;
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    crate::io::df_tail(df, n)
}

// =========================================================================
// Extended DataFrame Verb Native Functions
// =========================================================================

/// `mutate(df, "col_name", values)` or `df |> mutate("col_name", values)`
///
/// Adds or replaces a column. `values` can be a `Vector` or a scalar broadcast.
fn native_mutate(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`mutate()` requires 3 args: `mutate(df, \"col_name\", values)` or `df |> mutate(\"col_name\", values)`",
        ));
    }

    let df = &args[0];
    let col_name = args[1].as_str().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`mutate()` second argument must be a column name string")
    })?;

    // args[2] is the new column values — can be a Vector or a scalar
    let new_values: Vec<Value> = match &args[2] {
        Value::Vector(items) => items.iter().cloned().collect(),
        scalar => {
            // Broadcast scalar to match nrow
            let n = match df {
                Value::DataFrame { frame, .. } => frame.height().max(1),
                _ => 1,
            };
            vec![scalar.clone(); n]
        }
    };

    crate::io::df_mutate(df, col_name, new_values)
}

/// `rename(df, "old", "new")` or `df |> rename("old", "new")`
fn native_rename(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`rename()` requires 3 arguments: `rename(df, \"old_name\", \"new_name\")`",
        ));
    }

    let df = &args[0];
    let old_name = args[1].as_str().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`rename()`: second argument must be the old column name string")
    })?;
    let new_name = args[2].as_str().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`rename()`: third argument must be the new column name string")
    })?;

    crate::io::df_rename(df, old_name, new_name)
}

/// `drop(df, ["col1", "col2"])` or `df |> drop(["col1", "col2"])`
fn native_drop(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`drop()` requires a DataFrame as first argument")
    })?;

    let mut cols = Vec::new();
    for arg in &args[1..] {
        match arg {
            Value::Vector(items) => {
                for it in items.iter() {
                    match it {
                        Value::ColRef(s) | Value::String(s) => cols.push(s.clone()),
                        other => cols.push(format!("{other}")),
                    }
                }
            }
            Value::ColRef(s) | Value::String(s) => cols.push(s.clone()),
            other => cols.push(format!("{other}")),
        }
    }

    crate::io::df_drop(df, &cols)
}

/// `distinct(df)` — deduplicate all rows.
/// `distinct(df, ["col"])` — deduplicate by key column subset.
fn native_distinct(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`distinct()` requires a DataFrame")
    })?;

    let key_cols: Option<Vec<String>> = if args.len() > 1 {
        let mut cols = Vec::new();
        for arg in &args[1..] {
            match arg {
                Value::Vector(items) => {
                    for it in items.iter() {
                        match it {
                            Value::ColRef(s) | Value::String(s) => cols.push(s.clone()),
                            other => cols.push(format!("{other}")),
                        }
                    }
                }
                Value::ColRef(s) | Value::String(s) => cols.push(s.clone()),
                _ => {}
            }
        }
        if cols.is_empty() { None } else { Some(cols) }
    } else {
        None
    };

    crate::io::df_distinct(df, key_cols.as_deref())
}

fn native_nrow(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`nrow()` requires a DataFrame")
    })?;
    crate::io::df_nrow(df)
}

fn native_ncol(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`ncol()` requires a DataFrame")
    })?;
    crate::io::df_ncol(df)
}

fn native_colnames(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`colnames()` requires a DataFrame")
    })?;
    crate::io::df_colnames(df)
}

/// `slice(df, from, to)` — 0-based inclusive [from, to) row slice.
fn native_slice(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`slice()` requires a DataFrame")
    })?;
    let from = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let to   = args.get(2).and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    crate::io::df_slice(df, from, to)
}

// =========================================================================
// Regional Memory Arenas (RFC 03 §2.2)
// =========================================================================

fn native_arena_scope(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    let callable = args.first().cloned().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`arena::scope()` requires a callback function `\\a -> ...`")
    })?;
    let arena_state = std::sync::Arc::new(std::sync::Mutex::new(crate::arena::ArenaState::new()));
    let arena_val = Value::Arena(arena_state.clone());
    let res = interp.call_value(callable, vec![arena_val]);
    if let Ok(mut st) = arena_state.lock() {
        st.reset();
    }
    res
}

fn native_alloc_vector(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`alloc_vector(arena, len, [default])` requires at least an arena and length"));
    }
    let arena_val = &args[0];
    let len = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`alloc_vector(arena, len)` requires an integer length")
    })?;
    if len < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`alloc_vector()` length must be non-negative, found {len}")));
    }
    let default_val = args.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
    match arena_val {
        Value::Arena(st_arc) => {
            let mut st = st_arc.lock().map_err(|e| Diagnostic::compute_error("C0210", format!("Arena lock error: {e}")))?;
            Ok(st.alloc_vector(len as usize, default_val))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`alloc_vector()` requires an Arena as first argument, found `{}`", other.type_name()))),
    }
}

fn native_alloc_matrix(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error("C0201", "`alloc_matrix(arena, rows, cols, [default])` requires arena, rows, and cols"));
    }
    let arena_val = &args[0];
    let r = args[1].as_i64().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`alloc_matrix()` requires integer row count")
    })?;
    let c = args[2].as_i64().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`alloc_matrix()` requires integer col count")
    })?;
    if r < 0 || c < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`alloc_matrix()` dimensions must be non-negative, found ({r}, {c})")));
    }
    let default_val = args.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
    match arena_val {
        Value::Arena(st_arc) => {
            let mut st = st_arc.lock().map_err(|e| Diagnostic::compute_error("C0210", format!("Arena lock error: {e}")))?;
            Ok(st.alloc_matrix(r as usize, c as usize, default_val))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`alloc_matrix()` requires an Arena as first argument, found `{}`", other.type_name()))),
    }
}

fn native_arena_reset(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let arena_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`reset(arena)` requires an Arena argument")
    })?;
    match arena_val {
        Value::Arena(st_arc) => {
            let mut st = st_arc.lock().map_err(|e| Diagnostic::compute_error("C0210", format!("Arena lock error: {e}")))?;
            st.reset();
            Ok(Value::Unit)
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`reset()` requires an Arena, found `{}`", other.type_name()))),
    }
}

fn native_arena_allocated_bytes(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let arena_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`allocated_bytes(arena)` requires an Arena argument")
    })?;
    match arena_val {
        Value::Arena(st_arc) => {
            let st = st_arc.lock().map_err(|e| Diagnostic::compute_error("C0210", format!("Arena lock error: {e}")))?;
            Ok(Value::I64(st.allocated_bytes() as i64))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`allocated_bytes()` requires an Arena, found `{}`", other.type_name()))),
    }
}

/// `pivot_wider(df, names_from: "visit", values_from: "score", id_cols: ["id"])`
/// or `df |> pivot_wider(names_from: "visit", values_from: "score")`
fn native_pivot_wider(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`pivot_wider()` requires a DataFrame as first argument",
        ));
    }
    let df = &args[0];

    let mut names_from: Option<String> = None;
    let mut values_from: Option<String> = None;
    let mut id_cols: Option<Vec<String>> = None;
    let mut positional = Vec::new();

    for arg in &args[1..] {
        match arg {
            Value::NamedArg(name, val) => match name.as_str() {
                "names_from" => names_from = col_name_of(val),
                "values_from" => values_from = col_name_of(val),
                "id_cols" | "index" => {
                    let mut cols = Vec::new();
                    match val.as_ref() {
                        Value::Vector(items) => {
                            for it in items.iter() {
                                if let Some(c) = col_name_of(it) {
                                    cols.push(c);
                                }
                            }
                        }
                        other => {
                            if let Some(c) = col_name_of(other) {
                                cols.push(c);
                            }
                        }
                    }
                    id_cols = Some(cols);
                }
                other => {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("Unknown argument `{other}` in `pivot_wider()`"),
                    ));
                }
            },
            other => positional.push(other),
        }
    }

    if names_from.is_none() && !positional.is_empty() {
        names_from = col_name_of(positional[0]);
    }
    if values_from.is_none() && positional.len() > 1 {
        values_from = col_name_of(positional[1]);
    }
    if id_cols.is_none() && positional.len() > 2 {
        let mut cols = Vec::new();
        match positional[2] {
            Value::Vector(items) => {
                for it in items.iter() {
                    if let Some(c) = col_name_of(it) {
                        cols.push(c);
                    }
                }
            }
            other => {
                if let Some(c) = col_name_of(other) {
                    cols.push(c);
                }
            }
        }
        id_cols = Some(cols);
    }

    let nf = names_from.ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`pivot_wider()` requires `names_from`")
    })?;
    let vf = values_from.ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`pivot_wider()` requires `values_from`")
    })?;

    crate::io::df_pivot_wider(df, &nf, &vf, id_cols.as_deref())
}

/// `pivot_longer(df, cols: ["v1", "v2"], names_to: "visit", values_to: "score")`
/// or `df |> pivot_longer(cols: ["v1", "v2"])`
fn native_pivot_longer(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`pivot_longer()` requires a DataFrame as first argument",
        ));
    }
    let df = &args[0];

    let mut cols: Option<Vec<String>> = None;
    let mut names_to = "name".to_string();
    let mut values_to = "value".to_string();
    let mut id_cols: Option<Vec<String>> = None;
    let mut positional = Vec::new();

    for arg in &args[1..] {
        match arg {
            Value::NamedArg(name, val) => match name.as_str() {
                "cols" => {
                    let mut c_list = Vec::new();
                    match val.as_ref() {
                        Value::Vector(items) => {
                            for it in items.iter() {
                                if let Some(c) = col_name_of(it) {
                                    c_list.push(c);
                                }
                            }
                        }
                        other => {
                            if let Some(c) = col_name_of(other) {
                                c_list.push(c);
                            }
                        }
                    }
                    cols = Some(c_list);
                }
                "names_to" => {
                    if let Some(s) = col_name_of(val) {
                        names_to = s;
                    }
                }
                "values_to" => {
                    if let Some(s) = col_name_of(val) {
                        values_to = s;
                    }
                }
                "id_cols" | "index" => {
                    let mut c_list = Vec::new();
                    match val.as_ref() {
                        Value::Vector(items) => {
                            for it in items.iter() {
                                if let Some(c) = col_name_of(it) {
                                    c_list.push(c);
                                }
                            }
                        }
                        other => {
                            if let Some(c) = col_name_of(other) {
                                c_list.push(c);
                            }
                        }
                    }
                    id_cols = Some(c_list);
                }
                other => {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("Unknown argument `{other}` in `pivot_longer()`"),
                    ));
                }
            },
            other => positional.push(other),
        }
    }

    if cols.is_none() && !positional.is_empty() {
        let mut c_list = Vec::new();
        match positional[0] {
            Value::Vector(items) => {
                for it in items.iter() {
                    if let Some(c) = col_name_of(it) {
                        c_list.push(c);
                    }
                }
            }
            other => {
                if let Some(c) = col_name_of(other) {
                    c_list.push(c);
                }
            }
        }
        cols = Some(c_list);
    }
    if positional.len() > 1 {
        if let Some(s) = col_name_of(positional[1]) {
            names_to = s;
        }
    }
    if positional.len() > 2 {
        if let Some(s) = col_name_of(positional[2]) {
            values_to = s;
        }
    }

    crate::io::df_pivot_longer(df, cols.as_deref(), &names_to, &values_to, id_cols.as_deref())
}

fn extract_reason_str(v: &Value) -> Option<String> {
    match v {
        Value::NA(Some(r)) => Some(r.clone()),
        Value::String(s) => Some(s.clone()),
        Value::ColRef(c) => Some(c.clone()),
        _ => None,
    }
}

fn extract_reasons_list(val: &Value) -> Vec<String> {
    match val {
        Value::Vector(items) => {
            let mut list = Vec::new();
            for it in items.iter() {
                if let Some(r) = extract_reason_str(it) {
                    list.push(r);
                }
            }
            list
        }
        other => {
            if let Some(r) = extract_reason_str(other) {
                vec![r]
            } else {
                Vec::new()
            }
        }
    }
}

/// `impute(df, col, strategy: Mean, only_for: [NAReason::SensorDropout], value: ...)`
/// or `df |> impute(col("x"), strategy: Mean, only_for: [NAReason::SensorDropout])`
fn native_impute(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`impute()` requires a DataFrame as first argument",
        ));
    }
    let df = &args[0];

    let mut col: Option<String> = None;
    let mut strategy: Option<String> = None;
    let mut only_for: Option<Vec<String>> = None;
    let mut const_val: Option<Value> = None;
    let mut positional = Vec::new();

    for arg in &args[1..] {
        match arg {
            Value::NamedArg(name, val) => match name.as_str() {
                "col" | "column" => col = col_name_of(val),
                "strategy" | "method" => {
                    strategy = match val.as_ref() {
                        Value::String(s) => Some(s.clone()),
                        Value::ColRef(s) => Some(s.clone()),
                        other => col_name_of(other),
                    };
                }
                "only_for" | "for_reasons" | "reasons" => {
                    only_for = Some(extract_reasons_list(val));
                }
                "value" | "const" | "constant" | "default" => {
                    const_val = Some(*val.clone());
                }
                other => {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("Unknown argument `{other}` in `impute()`"),
                    ));
                }
            },
            other => positional.push(other),
        }
    }

    if col.is_none() && !positional.is_empty() {
        col = col_name_of(positional[0]);
    }
    if strategy.is_none() && positional.len() > 1 {
        strategy = match positional[1] {
            Value::String(s) => Some(s.clone()),
            Value::ColRef(s) => Some(s.clone()),
            other => col_name_of(other),
        };
    }
    if only_for.is_none() && positional.len() > 2 {
        only_for = Some(extract_reasons_list(positional[2]));
    }
    if const_val.is_none() && positional.len() > 3 {
        const_val = Some(positional[3].clone());
    }

    let col_name = col.ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`impute()` requires a column name")
    })?;
    let strat = strategy.unwrap_or_else(|| "mean".to_string());

    crate::io::df_impute(
        df,
        &col_name,
        &strat,
        only_for.as_deref(),
        const_val.as_ref(),
    )
}

/// `filter_na_reason(df, col, drop: [NAReason::NoResponse])`
/// or `df |> filter_na_reason(salary, drop: [NAReason::NoResponse])`
fn native_filter_na_reason(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`filter_na_reason()` requires a DataFrame as first argument",
        ));
    }
    let df = &args[0];

    let mut col: Option<String> = None;
    let mut drop_reasons: Option<Vec<String>> = None;
    let mut keep_reasons: Option<Vec<String>> = None;
    let mut positional = Vec::new();

    for arg in &args[1..] {
        match arg {
            Value::NamedArg(name, val) => match name.as_str() {
                "col" | "column" => col = col_name_of(val),
                "drop" | "exclude" => {
                    drop_reasons = Some(extract_reasons_list(val));
                }
                "keep" | "include" => {
                    keep_reasons = Some(extract_reasons_list(val));
                }
                other => {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("Unknown argument `{other}` in `filter_na_reason()`"),
                    ));
                }
            },
            other => positional.push(other),
        }
    }

    if col.is_none() && !positional.is_empty() {
        col = col_name_of(positional[0]);
    }
    if drop_reasons.is_none() && keep_reasons.is_none() && positional.len() > 1 {
        drop_reasons = Some(extract_reasons_list(positional[1]));
    }

    let col_name = col.ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`filter_na_reason()` requires a column name")
    })?;

    crate::io::df_filter_na_reason(
        df,
        &col_name,
        drop_reasons.as_deref(),
        keep_reasons.as_deref(),
    )
}
