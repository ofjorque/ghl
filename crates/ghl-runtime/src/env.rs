use std::collections::HashMap;
use ghl_diagnostics::{AestheticMap, Diagnostic, GeomLayer, PlotSpec, RenderCaps};
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEnv {
    pub scopes: Vec<HashMap<String, Value>>,
}

impl RuntimeEnv {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
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
        env.set("round".into(), Value::NativeFn(native_round));
        env.set("pow".into(),   Value::NativeFn(native_pow));
        env.set("clamp".into(), Value::NativeFn(native_clamp));
        env.set("pi".into(), Value::F64(std::f64::consts::PI));
        env.set("e".into(),  Value::F64(std::f64::consts::E));

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

        env
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
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

pub(crate) fn native_mean(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`mean()` requires at least 1 argument")
    })?;

    match vec_val {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "mean".into(), col: Some(name.clone()) }),
        Value::Vector(items) => {
            if items.is_empty() {
                return Ok(Value::NA(Some("EmptyVector".into())));
            }

            let mut sum = 0.0;
            let mut count = 0;

            for item in items {
                if let Value::NA(r) = item {
                    // Under GHL Kleene semantics: NA propagates through mean unless skip_na is active
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    sum += x;
                    count += 1;
                }
            }

            if count == 0 {
                Ok(Value::NA(None))
            } else {
                Ok(Value::F64(sum / count as f64))
            }
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
        Value::Vector(items) => {
            let mut sum = 0.0;
            let mut has_float = false;

            for item in items {
                if let Value::NA(r) = item {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    if matches!(item, Value::F64(_)) {
                        has_float = true;
                    }
                    sum += x;
                }
            }

            if has_float {
                Ok(Value::F64(sum))
            } else {
                Ok(Value::I64(sum as i64))
            }
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
        Value::Vector(items) => {
            if items.len() < 2 {
                return Err(Diagnostic::statistical_warning(
                    "SW0002",
                    "Sample variance requires at least N=2 observations (N-1 degrees of freedom)",
                ));
            }

            let mut numbers = Vec::with_capacity(items.len());
            for item in items {
                if let Value::NA(r) = item {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    numbers.push(x);
                }
            }

            let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
            let sum_sq = numbers.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
            let var = sum_sq / (numbers.len() - 1) as f64;

            Ok(Value::F64(var))
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
        Value::Vector(items) => {
            let mut min_val = f64::INFINITY;
            let mut found = false;

            for item in items {
                if let Value::NA(r) = item {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    if x < min_val {
                        min_val = x;
                        found = true;
                    }
                }
            }

            if found {
                Ok(Value::F64(min_val))
            } else {
                Ok(Value::NA(None))
            }
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
        Value::Vector(items) => {
            let mut max_val = f64::NEG_INFINITY;
            let mut found = false;

            for item in items {
                if let Value::NA(r) = item {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    if x > max_val {
                        max_val = x;
                        found = true;
                    }
                }
            }

            if found {
                Ok(Value::F64(max_val))
            } else {
                Ok(Value::NA(None))
            }
        }
        _ => Err(Diagnostic::compute_error("C0202", "`max()` expects a Vector")),
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
        Value::Vector(items) => Ok(items.first().cloned().unwrap_or(Value::NA(None))),
        other => Err(Diagnostic::compute_error("C0202", format!("`first()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_last(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`last()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "last".into(), col: Some(name.clone()) }),
        Value::Vector(items) => Ok(items.last().cloned().unwrap_or(Value::NA(None))),
        other => Err(Diagnostic::compute_error("C0202", format!("`last()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_median(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`median()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "median".into(), col: Some(name.clone()) }),
        Value::Vector(items) => {
            let mut nums: Vec<f64> = Vec::with_capacity(items.len());
            for it in items {
                if let Value::NA(r) = it {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = it.as_f64() {
                    nums.push(x);
                }
            }
            if nums.is_empty() {
                return Ok(Value::NA(None));
            }
            nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = nums.len() / 2;
            let med = if nums.len() % 2 == 0 { (nums[mid - 1] + nums[mid]) / 2.0 } else { nums[mid] };
            Ok(Value::F64(med))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`median()` expects a Vector, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_n_distinct(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`n_distinct()` requires 1 argument"))?;
    match v {
        Value::ColRef(name) => Ok(Value::AggSpec { kind: "n_distinct".into(), col: Some(name.clone()) }),
        Value::Vector(items) => {
            let seen: std::collections::HashSet<String> = items.iter().map(|it| format!("{:?}", it)).collect();
            Ok(Value::I64(seen.len() as i64))
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
                for it in items {
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
                for it in items {
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
        Some(df @ Value::DataFrame { .. }) => Ok(df.clone()),
        Some(other) => Err(Diagnostic::compute_error(
            "C0201",
            format!("`ungroup()` requires a GroupedDataFrame, found `{}`", other.type_name()),
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
        Value::Vector(items) => Ok(Value::Vector(items.iter().map(|it| Value::Bool(it.is_na())).collect())),
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

fn map_numeric_fn(v: &Value, f: impl Fn(f64) -> f64 + Clone) -> Value {
    match v {
        Value::NA(r) => Value::NA(r.clone()),
        Value::Vector(items) => Value::Vector(items.iter().map(|it| map_numeric_fn(it, f.clone())).collect()),
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

fn as_vector(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::Vector(items) => Some(items),
        _ => None,
    }
}

fn cumulative(items: &[Value], init: f64, combine: impl Fn(f64, f64) -> f64) -> Vec<Value> {
    let mut acc = init;
    let mut out = Vec::with_capacity(items.len());
    let mut poisoned: Option<Option<String>> = None;
    for it in items {
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

fn native_cumsum(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let items = args.first().and_then(as_vector).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cumsum()` requires a Vector argument")
    })?;
    Ok(Value::Vector(cumulative(items, 0.0, |a, b| a + b)))
}

fn native_cumprod(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let items = args.first().and_then(as_vector).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cumprod()` requires a Vector argument")
    })?;
    Ok(Value::Vector(cumulative(items, 1.0, |a, b| a * b)))
}

fn native_cummax(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let items = args.first().and_then(as_vector).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cummax()` requires a Vector argument")
    })?;
    Ok(Value::Vector(cumulative(items, f64::NEG_INFINITY, f64::max)))
}

fn native_cummin(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let items = args.first().and_then(as_vector).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`cummin()` requires a Vector argument")
    })?;
    Ok(Value::Vector(cumulative(items, f64::INFINITY, f64::min)))
}

fn native_lag(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let items = args.first().and_then(as_vector).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`lag()` requires a Vector argument")
    })?;
    let len = items.len();
    let n = (args.get(1).and_then(|v| v.as_i64()).unwrap_or(1).max(0) as usize).min(len);
    let mut out = Vec::with_capacity(len);
    out.extend(std::iter::repeat(Value::NA(None)).take(n));
    out.extend(items[..len - n].iter().cloned());
    Ok(Value::Vector(out))
}

fn native_lead(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let items = args.first().and_then(as_vector).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`lead()` requires a Vector argument")
    })?;
    let len = items.len();
    let n = (args.get(1).and_then(|v| v.as_i64()).unwrap_or(1).max(0) as usize).min(len);
    let mut out = Vec::with_capacity(len);
    out.extend(items[n..].iter().cloned());
    out.extend(std::iter::repeat(Value::NA(None)).take(n));
    Ok(Value::Vector(out))
}

fn broadcast_get(v: &Value, i: usize) -> Value {
    match v {
        Value::Vector(items) => items.get(i).cloned().unwrap_or(Value::NA(None)),
        scalar => scalar.clone(),
    }
}

fn native_if_else(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let cond = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;
    let yes = args.get(1).ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;
    let no = args.get(2).ok_or_else(|| Diagnostic::compute_error("C0201", "`if_else()` requires 3 arguments"))?;

    match cond {
        Value::Bool(b) => Ok(if *b { yes.clone() } else { no.clone() }),
        Value::NA(r) => Ok(Value::NA(r.clone())),
        Value::Vector(conds) => {
            let mut out = Vec::with_capacity(conds.len());
            for (i, c) in conds.iter().enumerate() {
                out.push(match c {
                    Value::Bool(true) => broadcast_get(yes, i),
                    Value::Bool(false) => broadcast_get(no, i),
                    Value::NA(r) => Value::NA(r.clone()),
                    _ => Value::NA(None),
                });
            }
            Ok(Value::Vector(out))
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
        Value::Vector(items) => Ok(Value::Vector(items.iter().map(|it| check(it, lo, hi)).collect())),
        other => Ok(check(other, lo, hi)),
    }
}

fn sort_vector(args: Vec<Value>, desc: bool, fn_name: &str) -> Result<Value, Diagnostic> {
    let items = args.first().and_then(as_vector).ok_or_else(|| {
        Diagnostic::compute_error("C0201", format!("`{}()` requires a Vector argument", fn_name))
    })?;
    let mut sorted = items.clone();
    sorted.sort_by(|a, b| {
        let ord = crate::io::compare_values(Some(a), Some(b));
        if desc { ord.reverse() } else { ord }
    });
    Ok(Value::Vector(sorted))
}

fn native_sort_asc(args: Vec<Value>) -> Result<Value, Diagnostic> {
    sort_vector(args, false, "sort_asc")
}

fn native_sort_desc(args: Vec<Value>) -> Result<Value, Diagnostic> {
    sort_vector(args, true, "sort_desc")
}

/// Integer rank with ties resolved by averaging (matches R's default `rank()`).
fn native_rank(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let items = args.first().and_then(as_vector).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`rank()` requires a Vector argument")
    })?;
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
    Ok(Value::Vector(ranks.into_iter().map(Value::F64).collect()))
}

// =========================================================================
// String helpers — scalar `String` + `Vector[String]`, both vectorized
// =========================================================================

fn map_string_fn(v: &Value, f: impl Fn(&str) -> Value + Clone) -> Value {
    match v {
        Value::String(s) => f(s),
        Value::Vector(items) => Value::Vector(items.iter().map(|it| map_string_fn(it, f.clone())).collect()),
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
        Value::Vector(s.split(sep.as_str()).map(|p| Value::String(p.to_string())).collect())
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
        return Err(Diagnostic::compute_error("C0201", "`filter()` requires a DataFrame"));
    }

    let df = args[0].clone();
    match df {
        Value::DataFrame { frame, na_reasons } => {
            // If condition was passed as second argument:
            if args.len() > 1 {
                // If the second argument is a ColPredicate: vectorized comparison
                // straight on the polars column (Suite 02, Caso 2.3) -- no boxing the
                // whole column to Vec<Value> and comparing scalar-by-scalar in Rust.
                if let Value::ColPredicate { col, op, rhs } = &args[1] {
                    return crate::io::df_filter_by_col_predicate(&Value::DataFrame { frame, na_reasons }, col, *op, rhs);
                }

                // If the second argument is a boolean Vector (mask)
                if let Value::Vector(mask) = &args[1] {
                    let keep_indices: Vec<usize> = mask.iter().enumerate()
                        .filter(|(_, m)| m.as_bool() == Some(true))
                        .map(|(i, _)| i)
                        .collect();
                    let (new_frame, new_reasons) = crate::io::take_rows(&frame, &na_reasons, &keep_indices)?;
                    return Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons });
                }
            }

            Ok(Value::DataFrame { frame, na_reasons })
        }
        other => Ok(other),
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
    let (columns, data) = crate::polars_bridge::dataframe_to_columns_and_data(frame, na_reasons)?;

    let blueprint = crate::neko::Blueprint::new(response, terms);
    let model = crate::neko::FittedModel::fit_ols(blueprint, &columns, &data)?;
    Ok(Value::ModelFit(Box::new(model)))
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
            let res_vals = m.residuals.iter().map(|&x| Value::F64(x)).collect();
            Ok(Value::Vector(res_vals))
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
            let coef_vals = m.coefficients.iter().map(|&x| Value::F64(x)).collect();
            Ok(Value::Vector(coef_vals))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`coef()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_vcov(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`vcov()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            let kind = if let Some(k_str) = args.get(1).and_then(|v| v.as_str()) {
                match k_str.to_uppercase().as_str() {
                    "HC0" => crate::neko::VcovKind::HC0,
                    "HC1" => crate::neko::VcovKind::HC1,
                    "HC2" => crate::neko::VcovKind::HC2,
                    "HC3" => crate::neko::VcovKind::HC3,
                    _ => crate::neko::VcovKind::Classical,
                }
            } else {
                crate::neko::VcovKind::Classical
            };
            let p = m.blueprint.term_names.len();
            let vcov_data = m.compute_vcov(kind);
            Ok(Value::Matrix {
                rows: p,
                cols: p,
                data: vcov_data,
            })
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`vcov()` requires a ModelFit, found `{}`", other.type_name()),
        )),
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

fn native_show(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0305", "`show()` requires a Plot or Value to render")
    })?;

    let caps = RenderCaps::detect();
    match target {
        Value::Plot(p) => {
            println!("{}\n", p.render(&caps));
        }
        other => {
            println!("{}\n", other.render_styled(&caps));
        }
    }
    Ok(Value::Unit)
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
    Ok(Value::Vector(vec_vals))
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

fn native_select(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`select()` requires a DataFrame"));
    }

    let df = &args[0];
    let mut cols = Vec::new();

    for arg in &args[1..] {
        match arg {
            Value::Vector(items) => {
                for it in items {
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
        Value::Vector(items) => items.clone(),
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
                for it in items {
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
                    for it in items {
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
