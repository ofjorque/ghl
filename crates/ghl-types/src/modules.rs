//! Standard Library Module Registry for GHL Type Checker.
//!
//! Provides static symbol tables and module path resolution for `std::*` modules:
//! - `std::dataframe`
//! - `std::linalg`
//! - `std::stats` (and submodules `distributions`, `rng`, `models`)
//! - `std::math`
//! - `std::io`
//! - `std::plot`

use crate::types::Type;

fn any_fn() -> Type {
    Type::Function {
        params: vec![Type::Any],
        ret: Box::new(Type::Any),
    }
}

fn any_fn_multi(n: usize) -> Type {
    Type::Function {
        params: vec![Type::Any; n],
        ret: Box::new(Type::Any),
    }
}

/// Returns the items in a given module path if valid.
pub fn get_module_items(path: &[String]) -> Option<Vec<(String, Type)>> {
    let p: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
    match p.as_slice() {
        ["std", "dataframe"] => Some(vec![
            ("read_csv".into(), any_fn()),
            ("read_parquet".into(), any_fn()),
            ("write_csv".into(), any_fn_multi(2)),
            ("write_parquet".into(), any_fn_multi(2)),
            ("parse_csv".into(), any_fn()),
            ("col".into(), any_fn()),
            ("filter".into(), any_fn_multi(2)),
            ("select".into(), any_fn_multi(2)),
            ("mutate".into(), any_fn_multi(2)),
            ("arrange".into(), any_fn_multi(2)),
            ("rename".into(), any_fn_multi(2)),
            ("drop".into(), any_fn_multi(2)),
            ("distinct".into(), any_fn()),
            ("head".into(), any_fn_multi(2)),
            ("tail".into(), any_fn_multi(2)),
            ("slice".into(), any_fn_multi(3)),
            ("nrow".into(), any_fn()),
            ("ncol".into(), any_fn()),
            ("colnames".into(), any_fn()),
            ("group_by".into(), any_fn_multi(2)),
            ("summarize".into(), any_fn_multi(2)),
            ("ungroup".into(), any_fn()),
            ("first".into(), any_fn()),
            ("last".into(), any_fn()),
            ("n_distinct".into(), any_fn()),
            ("count".into(), any_fn()),
            ("coalesce".into(), any_fn_multi(2)),
            ("desc".into(), any_fn()),
            ("pull".into(), any_fn_multi(2)),
            ("slice_min".into(), any_fn_multi(3)),
            ("slice_max".into(), any_fn_multi(3)),
            ("sample_n".into(), any_fn_multi(2)),
            ("sample_frac".into(), any_fn_multi(2)),
            ("inner_join".into(), any_fn_multi(3)),
            ("left_join".into(), any_fn_multi(3)),
            ("fill_na".into(), any_fn_multi(2)),
            ("fill_na_all".into(), any_fn_multi(2)),
            ("glimpse".into(), any_fn()),
            ("na_reason".into(), any_fn()),
            ("na_reasons".into(), any_fn()),
            ("is_na".into(), any_fn()),
            ("lazy".into(), any_fn()),
            ("collect".into(), any_fn()),
            ("explain".into(), any_fn_multi(2)),
            ("scan_csv".into(), any_fn()),
            ("scan_parquet".into(), any_fn()),
        ]),

        ["std", "linalg"] => Some(vec![
            ("dot".into(), any_fn_multi(2)),
            ("transpose".into(), any_fn()),
            ("t".into(), any_fn()),
            ("identity".into(), any_fn()),
            ("eye".into(), any_fn()),
            ("diag".into(), any_fn()),
            ("zeros".into(), any_fn_multi(2)),
            ("len".into(), any_fn()),
            ("get".into(), any_fn_multi(3)),
            ("set".into(), any_fn_multi(4)),
            ("get_row".into(), any_fn_multi(2)),
            ("set_row".into(), any_fn_multi(3)),
            ("get_col".into(), any_fn_multi(2)),
            ("qr".into(), any_fn()),
            ("qr_q".into(), any_fn()),
            ("qr_r".into(), any_fn()),
            ("cholesky".into(), any_fn()),
            ("svd".into(), any_fn()),
            ("svd_u".into(), any_fn()),
            ("svd_s".into(), any_fn()),
            ("svd_v".into(), any_fn()),
            ("eigen".into(), any_fn()),
            ("eigen_values".into(), any_fn()),
            ("eigen_vectors".into(), any_fn()),
        ]),

        ["std", "stats", "distributions"] => Some(vec![
            ("normal_pdf".into(), any_fn_multi(3)),
            ("normal_cdf".into(), any_fn_multi(3)),
            ("random_normal".into(), any_fn_multi(3)),
            ("random_gamma".into(), any_fn_multi(3)),
            ("gamma_pdf".into(), any_fn_multi(3)),
            ("gamma_cdf".into(), any_fn_multi(3)),
            ("random_uniform".into(), any_fn_multi(3)),
        ]),

        ["std", "stats", "rng"] => Some(vec![
            ("set_seed".into(), any_fn()),
            ("split_rng".into(), any_fn()),
            ("get_seed".into(), any_fn()),
        ]),

        ["std", "stats", "models"] => Some(vec![
            ("fit".into(), any_fn_multi(2)),
            ("ols".into(), any_fn_multi(2)),
            ("fit_logistic".into(), any_fn_multi(2)),
            ("fit_gmm".into(), any_fn_multi(2)),
            ("gmm".into(), any_fn_multi(2)),
            ("summary".into(), any_fn()),
            ("tidy".into(), any_fn()),
            ("glance".into(), any_fn()),
            ("augment".into(), any_fn()),
            ("predict".into(), any_fn_multi(2)),
            ("residuals".into(), any_fn()),
            ("coef".into(), any_fn()),
            ("vcov".into(), any_fn()),
        ]),

        ["std", "stats"] => {
            let mut items = vec![
                ("mean".into(), Type::Function { params: vec![Type::Vector(Box::new(Type::Any))], ret: Box::new(Type::F64) }),
                ("median".into(), Type::Function { params: vec![Type::Vector(Box::new(Type::Any))], ret: Box::new(Type::F64) }),
                ("var".into(), Type::Function { params: vec![Type::Vector(Box::new(Type::Any))], ret: Box::new(Type::F64) }),
                ("std_dev".into(), Type::Function { params: vec![Type::Vector(Box::new(Type::Any))], ret: Box::new(Type::F64) }),
                ("min".into(), any_fn()),
                ("max".into(), any_fn()),
                ("quantile".into(), any_fn_multi(2)),
                ("iqr".into(), any_fn()),
                ("skewness".into(), any_fn()),
                ("kurtosis".into(), any_fn()),
                ("cov".into(), any_fn_multi(2)),
                ("cor".into(), any_fn_multi(2)),
                ("bootstrap_mean".into(), any_fn_multi(3)),
            ];
            if let Some(dist) = get_module_items(&["std".into(), "stats".into(), "distributions".into()]) {
                items.extend(dist);
            }
            if let Some(rng) = get_module_items(&["std".into(), "stats".into(), "rng".into()]) {
                items.extend(rng);
            }
            if let Some(models) = get_module_items(&["std".into(), "stats".into(), "models".into()]) {
                items.extend(models);
            }
            Some(items)
        }

        ["std", "math"] => Some(vec![
            ("log".into(), any_fn()),
            ("log2".into(), any_fn()),
            ("log10".into(), any_fn()),
            ("exp".into(), any_fn()),
            ("sqrt".into(), any_fn()),
            ("abs".into(), any_fn()),
            ("floor".into(), any_fn()),
            ("ceil".into(), any_fn()),
            ("round".into(), any_fn_multi(2)),
            ("pow".into(), any_fn_multi(2)),
            ("clamp".into(), any_fn_multi(3)),
            ("sin".into(), any_fn()),
            ("cos".into(), any_fn()),
            ("log_sum_exp".into(), any_fn()),
            ("pi".into(), Type::F64),
            ("e".into(), Type::F64),
        ]),

        ["std", "io"] => Some(vec![
            ("print".into(), any_fn()),
            ("println".into(), any_fn()),
            ("read_file".into(), any_fn()),
            ("read_lines".into(), any_fn()),
            ("write_file".into(), any_fn_multi(2)),
            ("append_file".into(), any_fn_multi(2)),
            ("file_exists".into(), any_fn()),
            ("read_csv".into(), any_fn()),
            ("write_csv".into(), any_fn_multi(2)),
            ("read_parquet".into(), any_fn()),
            ("write_parquet".into(), any_fn_multi(2)),
            ("scan_csv".into(), any_fn()),
            ("scan_parquet".into(), any_fn()),
        ]),

        ["std", "plot"] => Some(vec![
            ("plot".into(), any_fn_multi(2)),
            ("aes".into(), any_fn()),
            ("geom_point".into(), any_fn()),
            ("geom_line".into(), any_fn()),
            ("geom_smooth".into(), any_fn()),
            ("geom_histogram".into(), any_fn()),
            ("geom_boxplot".into(), any_fn()),
            ("geom_bar".into(), any_fn()),
            ("labs".into(), any_fn()),
            ("show".into(), any_fn()),
            ("save".into(), any_fn_multi(2)),
            ("scatter".into(), any_fn_multi(3)),
            ("hist".into(), any_fn_multi(2)),
            ("histogram".into(), any_fn_multi(2)),
            ("boxplot".into(), any_fn_multi(2)),
        ]),

        ["std", "arena"] => Some(vec![
            ("scope".into(), any_fn()),
            ("alloc_vector".into(), any_fn_multi(2)),
            ("alloc_matrix".into(), any_fn_multi(3)),
            ("reset".into(), any_fn()),
            ("allocated_bytes".into(), any_fn()),
        ]),

        _ => None,
    }
}

/// Checks if a module path is a recognized module in the standard library.
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

/// Look up a specific item given its full path, e.g. `["std", "math", "sqrt"]`.
pub fn lookup_module_item(path: &[String]) -> Option<Type> {
    if path.len() < 2 {
        return None;
    }
    let (mod_path, item_name) = path.split_at(path.len() - 1);
    let name = &item_name[0];
    if let Some(items) = get_module_items(mod_path) {
        for (item, ty) in items {
            if &item == name {
                return Some(ty);
            }
        }
    }
    None
}
