//! Extended DataFrame verbs: mutate/rename/drop/distinct/slice plus pivot_wider/pivot_longer/impute/filter_na_reason.
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use crate::native_dataframe_agg::*;
use crate::value::Value;
use ghl_diagnostics::Diagnostic;

// =========================================================================
// Extended DataFrame Verb Native Functions
// =========================================================================

/// `mutate(df, "col_name", values)` or `df |> mutate("col_name", values)`
///
/// Adds or replaces a column. `values` can be a `Vector` or a scalar broadcast.
pub(crate) fn native_mutate(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`mutate()` requires 3 args: `mutate(df, \"col_name\", values)` or `df |> mutate(\"col_name\", values)`",
        ));
    }

    let df = &args[0];
    let col_name = args[1].as_str().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`mutate()` second argument must be a column name string",
        )
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
pub(crate) fn native_rename(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`rename()` requires 3 arguments: `rename(df, \"old_name\", \"new_name\")`",
        ));
    }

    let df = &args[0];
    let old_name = args[1].as_str().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`rename()`: second argument must be the old column name string",
        )
    })?;
    let new_name = args[2].as_str().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`rename()`: third argument must be the new column name string",
        )
    })?;

    crate::io::df_rename(df, old_name, new_name)
}

/// `drop(df, ["col1", "col2"])` or `df |> drop(["col1", "col2"])`
pub(crate) fn native_drop(args: Vec<Value>) -> Result<Value, Diagnostic> {
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
pub(crate) fn native_distinct(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`distinct()` requires a DataFrame"))?;

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

pub(crate) fn native_nrow(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`nrow()` requires a DataFrame"))?;
    crate::io::df_nrow(df)
}

pub(crate) fn native_ncol(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`ncol()` requires a DataFrame"))?;
    crate::io::df_ncol(df)
}

pub(crate) fn native_colnames(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`colnames()` requires a DataFrame"))?;
    crate::io::df_colnames(df)
}

/// `slice(df, from, to)` — 0-based inclusive [from, to) row slice.
pub(crate) fn native_slice(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`slice()` requires a DataFrame"))?;
    let from = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let to = args.get(2).and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    crate::io::df_slice(df, from, to)
}

/// `pivot_wider(df, names_from: "visit", values_from: "score", id_cols: ["id"])`
/// or `df |> pivot_wider(names_from: "visit", values_from: "score")`
pub(crate) fn native_pivot_wider(args: Vec<Value>) -> Result<Value, Diagnostic> {
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
pub(crate) fn native_pivot_longer(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

    crate::io::df_pivot_longer(
        df,
        cols.as_deref(),
        &names_to,
        &values_to,
        id_cols.as_deref(),
    )
}

pub(crate) fn extract_reason_str(v: &Value) -> Option<String> {
    match v {
        Value::NA(Some(r)) => Some(r.clone()),
        Value::String(s) => Some(s.clone()),
        Value::ColRef(c) => Some(c.clone()),
        _ => None,
    }
}

pub(crate) fn extract_reasons_list(val: &Value) -> Vec<String> {
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
pub(crate) fn native_impute(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

    let col_name =
        col.ok_or_else(|| Diagnostic::compute_error("C0201", "`impute()` requires a column name"))?;
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
pub(crate) fn native_filter_na_reason(args: Vec<Value>) -> Result<Value, Diagnostic> {
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
