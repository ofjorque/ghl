//! Standard Library Module Registry for GHL Runtime.
//!
//! The canonical list of which items live in which `std::*` module is
//! `ghl_types::modules` (also used by the type checker) - this crate derives
//! its own module resolution from it instead of hand-maintaining a second
//! copy. The two used to be independent, hand-written lists and had already
//! drifted: `std::stats::rng` declared different items on each side, and
//! several `std::stats` items (`quantile`, `iqr`, `skewness`, `kurtosis`,
//! `cov`, `cor`, and the old `rng` trio) type-checked fine but had no
//! `native_*` implementation at all, so `ghl check` passed and `ghl run`
//! crashed with "Cannot find item". Once a name is confirmed to belong to a
//! module here, it's resolved to its actual implementation by looking it up
//! in the prelude environment by name.

use std::sync::LazyLock;
use crate::env::RuntimeEnv;
use crate::value::Value;

static PRELUDE_ENV: LazyLock<RuntimeEnv> = LazyLock::new(RuntimeEnv::with_prelude);

/// Returns the item names belonging to a standard library module path, per
/// `ghl_types::modules`'s canonical registry.
pub fn get_module_item_names(path: &[String]) -> Option<Vec<String>> {
    ghl_types::modules::get_module_items(path).map(|items| items.into_iter().map(|(name, _ty)| name).collect())
}

/// Checks if a module path is recognized in the standard library.
pub fn is_valid_module_path(path: &[String]) -> bool {
    ghl_types::modules::is_valid_module_path(path)
}

/// Checks if an item is present in a module path.
pub fn is_item_in_module(mod_path: &[String], item_name: &str) -> bool {
    match get_module_item_names(mod_path) {
        Some(names) => names.iter().any(|n| n == item_name),
        None => false,
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
        let full_name = path.join("::");
        let short_name = if path.len() >= 2 && path[0] == "std" {
            path[1..].join("::")
        } else {
            full_name.clone()
        };
        PRELUDE_ENV
            .get(&full_name)
            .or_else(|| PRELUDE_ENV.get(&short_name))
            .or_else(|| PRELUDE_ENV.get(name))
    } else {
        None
    }
}

/// Returns all `(name, Value)` pairs for a given module path.
pub fn get_module_items(path: &[String]) -> Option<Vec<(String, Value)>> {
    let names = get_module_item_names(path)?;
    let mut items = Vec::with_capacity(names.len());
    for name in names {
        let full_name = format!("{}::{}", path.join("::"), name);
        if let Some(val) = PRELUDE_ENV.get(&full_name).or_else(|| PRELUDE_ENV.get(&name)) {
            items.push((name, val));
        }
    }
    Some(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard for the exact bug this unification fixes: every name
    /// `ghl_types::modules` declares importable from a `std::*` module must
    /// actually resolve to a runtime value here. If this fails, someone added
    /// a name to the type checker's registry without implementing (or without
    /// registering into the prelude) the matching native function - it would
    /// type-check and then crash at runtime with "Cannot find item".
    #[test]
    fn test_every_declared_module_item_has_a_runtime_value() {
        let module_paths: &[&[&str]] = &[
            &["std", "dataframe"],
            &["std", "linalg"],
            &["std", "stats"],
            &["std", "stats", "distributions"],
            &["std", "stats", "rng"],
            &["std", "stats", "models"],
            &["std", "prob"],
            &["std", "math"],
            &["std", "io"],
            &["std", "plot"],
            &["std", "arena"],
            &["std", "autodiff"],
            &["std", "http"],
            &["std", "net"],
            &["std", "concurrency"],
            &["std", "gpu"],
        ];

        let mut missing = Vec::new();
        for path in module_paths {
            let path: Vec<String> = path.iter().map(|s| s.to_string()).collect();
            let Some(names) = get_module_item_names(&path) else {
                continue;
            };
            for name in names {
                let mut full_path = path.clone();
                full_path.push(name.clone());
                if lookup_module_item(&full_path).is_none() {
                    missing.push(format!("{}::{}", path.join("::"), name));
                }
            }
        }

        assert!(
            missing.is_empty(),
            "these std::* items are declared but have no runtime implementation: {missing:?}"
        );
    }
}
