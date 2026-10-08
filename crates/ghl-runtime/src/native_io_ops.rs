//! Primitive and tabular file I/O native functions.
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use crate::value::Value;
use crate::vector_data::VectorData;
use ghl_diagnostics::Diagnostic;

// =========================================================================
// Primitive and Tabular I/O Native Functions
// =========================================================================

pub(crate) fn native_read_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`read_file()` requires a file path string")
    })?;
    let content = crate::io::read_file(path)?;
    Ok(Value::String(content))
}

pub(crate) fn native_read_lines(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`read_lines()` requires a file path string")
    })?;
    let lines = crate::io::read_lines(path)?;
    let vec_vals = lines.into_iter().map(Value::String).collect();
    Ok(Value::Vector(VectorData::from_values(vec_vals)))
}

pub(crate) fn resolve_path_and_content(s1: &str, s2: &str) -> (String, String) {
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

pub(crate) fn native_write_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0402",
            "`write_file()` requires destination path and content: `write_file(path, content)` or `content |> write_file(path)`",
        ));
    }

    let (path, content): (String, String) =
        if let (Some(s1), Some(s2)) = (args[0].as_str(), args[1].as_str()) {
            resolve_path_and_content(s1, s2)
        } else if let Some(p) = args[1].as_str() {
            (p.to_string(), args[0].to_string())
        } else if let Some(p) = args[0].as_str() {
            (p.to_string(), args[1].to_string())
        } else {
            return Err(Diagnostic::compute_error(
                "C0402",
                "`write_file()` requires string arguments",
            ));
        };

    crate::io::write_file(&path, &content)?;
    Ok(Value::Unit)
}

pub(crate) fn native_append_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0402",
            "`append_file()` requires destination path and content: `append_file(path, content)` or `content |> append_file(path)`",
        ));
    }

    let (path, content): (String, String) =
        if let (Some(s1), Some(s2)) = (args[0].as_str(), args[1].as_str()) {
            resolve_path_and_content(s1, s2)
        } else if let Some(p) = args[1].as_str() {
            (p.to_string(), args[0].to_string())
        } else if let Some(p) = args[0].as_str() {
            (p.to_string(), args[1].to_string())
        } else {
            return Err(Diagnostic::compute_error(
                "C0402",
                "`append_file()` requires string arguments",
            ));
        };

    crate::io::append_file(&path, &content)?;
    Ok(Value::Unit)
}

pub(crate) fn native_file_exists(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`file_exists()` requires a file path string")
    })?;
    Ok(Value::Bool(crate::io::file_exists(path)))
}

pub(crate) fn native_read_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0403", "`read_csv()` requires a file path string")
    })?;
    let delim = args
        .get(1)
        .and_then(|v| v.as_str())
        .and_then(|s| s.chars().next());
    crate::io::read_csv_file(path, delim)
}

pub(crate) fn native_parse_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let content = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0403", "`parse_csv()` requires a CSV text string")
    })?;
    let delim = args
        .get(1)
        .and_then(|v| v.as_str())
        .and_then(|s| s.chars().next());
    crate::io::parse_csv_string(content, delim)
}

pub(crate) fn native_write_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

    let delim = args
        .get(delim_idx)
        .and_then(|v| v.as_str())
        .and_then(|s| s.chars().next());
    crate::io::write_csv_file(df, p, delim)?;
    Ok(Value::Unit)
}

pub(crate) fn native_read_parquet(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0405", "`read_parquet()` requires a file path string")
    })?;
    crate::io::read_parquet_file(path)
}

pub(crate) fn native_write_parquet(args: Vec<Value>) -> Result<Value, Diagnostic> {
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
            return Err(Diagnostic::compute_error(
                "C0405",
                "`write_parquet()` requires a DataFrame argument",
            ));
        }
    };
    let p = path.ok_or_else(|| {
        Diagnostic::compute_error(
            "C0405",
            "`write_parquet()` requires a string destination path",
        )
    })?;

    crate::io::write_parquet_file(df, p)?;
    Ok(Value::Unit)
}

pub(crate) fn native_lazy(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`lazy()` requires a DataFrame"))?;
    crate::io::df_lazy(df)
}

pub(crate) fn native_collect(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let lf = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`collect()` requires a LazyFrame"))?;
    crate::io::df_collect(lf)
}

pub(crate) fn native_explain(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let lf = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`explain()` requires a LazyFrame"))?;
    let opt = args.get(1).and_then(|v| v.as_bool()).unwrap_or(true);
    crate::io::df_explain(lf, opt)
}

pub(crate) fn native_scan_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0403", "`scan_csv()` requires a file path string")
    })?;
    crate::io::scan_csv_file(path)
}

pub(crate) fn native_scan_parquet(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0405", "`scan_parquet()` requires a file path string")
    })?;
    crate::io::scan_parquet_file(path)
}

pub(crate) fn native_select(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`select()` requires a DataFrame",
        ));
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

pub(crate) fn native_head(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`head()` requires a DataFrame"))?;
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    crate::io::df_head(df, n)
}

pub(crate) fn native_tail(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args
        .first()
        .ok_or_else(|| Diagnostic::compute_error("C0201", "`tail()` requires a DataFrame"))?;
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    crate::io::df_tail(df, n)
}
