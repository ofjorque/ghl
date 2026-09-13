//! Canonical Standard Library Module Registry for GHL Runtime.
//!
//! Organizes built-in functions, statistical models, and constants into clean,
//! isolated namespaces under `std::*`:
//! - `std::dataframe` (I/O, wrangling verbs, joins, NA semantics)
//! - `std::linalg` (faer-backed dense linear algebra, matrix operations, decompositions)
//! - `std::stats` (summary stats, plus `distributions`, `rng`, `models`)
//! - `std::math` (scalar + vector mathematical functions, constants pi/e)
//! - `std::io` (file I/O and printing)
//! - `std::plot` (Grammar of Graphics statistical plotting)

use std::sync::LazyLock;
use crate::env::RuntimeEnv;
use crate::value::Value;

static PRELUDE_ENV: LazyLock<RuntimeEnv> = LazyLock::new(RuntimeEnv::with_prelude);

/// Returns the item names belonging to a standard library module path.
pub fn get_module_item_names(path: &[String]) -> Option<Vec<&'static str>> {
    let p: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
    match p.as_slice() {
        ["std", "dataframe"] => Some(vec![
            "read_csv", "read_parquet", "write_csv", "write_parquet", "parse_csv",
            "col", "filter", "select", "mutate", "arrange", "rename", "drop",
            "distinct", "head", "tail", "slice", "nrow", "ncol", "colnames",
            "group_by", "summarize", "ungroup", "first", "last", "n_distinct",
            "count", "coalesce", "desc", "pull", "slice_min", "slice_max",
            "sample_n", "sample_frac", "inner_join", "left_join",
            "fill_na", "fill_na_all", "pivot_wider", "pivot_longer", "impute", "filter_na_reason",
            "glimpse", "na_reason", "na_reasons", "is_na",
            "lazy", "collect", "explain", "scan_csv", "scan_parquet",
        ]),

        ["std", "linalg"] => Some(vec![
            "dot", "transpose", "t", "identity", "eye", "diag", "zeros",
            "len", "get", "set", "get_row", "set_row", "get_col",
            "qr", "qr_q", "qr_r", "cholesky", "svd", "svd_u", "svd_s", "svd_v",
            "eigen", "eigen_values", "eigen_vectors",
        ]),

        ["std", "stats", "distributions"] => Some(vec![
            "normal_pdf", "normal_cdf", "random_normal", "random_gamma",
            "gamma_pdf", "gamma_cdf", "random_uniform",
        ]),

        ["std", "stats", "rng"] => Some(vec![
            "random_uniform", "random_normal", "random_gamma",
        ]),

        ["std", "stats", "models"] => Some(vec![
            "fit", "ols", "fit_logistic", "fit_gmm", "gmm",
            "summary", "tidy", "glance", "augment", "predict",
            "residuals", "coef", "vcov",
        ]),

        ["std", "stats"] => {
            let mut items = vec![
                "mean", "median", "var", "std_dev", "min", "max",
                "bootstrap_mean",
            ];
            if let Some(dist) = get_module_item_names(&["std".into(), "stats".into(), "distributions".into()]) {
                items.extend(dist);
            }
            if let Some(models) = get_module_item_names(&["std".into(), "stats".into(), "models".into()]) {
                items.extend(models);
            }
            Some(items)
        }

        ["std", "math"] => Some(vec![
            "log", "log2", "log10", "exp", "sqrt", "abs", "floor", "ceil",
            "sin", "cos", "round", "pow", "clamp", "log_sum_exp", "pi", "e",
        ]),

        ["std", "io"] => Some(vec![
            "print", "println", "read_file", "read_lines", "write_file",
            "append_file", "file_exists", "read_csv", "write_csv",
            "read_parquet", "write_parquet", "scan_csv", "scan_parquet",
        ]),

        ["std", "plot"] => Some(vec![
            "plot", "aes", "geom_point", "geom_line", "geom_smooth",
            "geom_histogram", "geom_boxplot", "geom_bar", "labs",
            "show", "save", "scatter", "hist", "histogram", "boxplot",
        ]),

        ["std", "arena"] => Some(vec![
            "scope", "alloc_vector", "alloc_matrix", "reset", "allocated_bytes",
        ]),

        _ => None,
    }
}

/// Checks if a module path is recognized in the standard library.
pub fn is_valid_module_path(path: &[String]) -> bool {
    let p: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
    matches!(
        p.as_slice(),
        ["std"]
            | ["std", "dataframe"]
            | ["std", "linalg"]
            | ["std", "stats"]
            | ["std", "stats", "distributions"]
            | ["std", "stats", "rng"]
            | ["std", "stats", "models"]
            | ["std", "math"]
            | ["std", "io"]
            | ["std", "plot"]
            | ["std", "arena"]
    )
}

/// Checks if an item is present in a module path.
pub fn is_item_in_module(mod_path: &[String], item_name: &str) -> bool {
    if let Some(names) = get_module_item_names(mod_path) {
        names.contains(&item_name)
    } else {
        false
    }
}

/// Look up a specific item given its full path (e.g. `["std", "math", "sqrt"]`).
pub fn lookup_module_item(path: &[String]) -> Option<Value> {
    if path.len() < 2 {
        return None;
    }
    let (mod_path, item_name) = path.split_at(path.len() - 1);
    let name = &item_name[0];
    if is_item_in_module(mod_path, name) {
        PRELUDE_ENV.get(name)
    } else {
        None
    }
}

/// Returns all `(name, Value)` pairs for a given module path.
pub fn get_module_items(path: &[String]) -> Option<Vec<(String, Value)>> {
    let names = get_module_item_names(path)?;
    let mut items = Vec::with_capacity(names.len());
    for name in names {
        if let Some(val) = PRELUDE_ENV.get(name) {
            items.push((name.to_string(), val));
        }
    }
    Some(items)
}
