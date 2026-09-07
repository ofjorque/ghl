//! Primitive and Tabular I/O Subsystem for GHL.
//!
//! Provides:
//! - Primitive file operations: `read_file`, `read_lines`, `write_file`, `append_file`, `file_exists`
//! - Tabular CSV operations: `read_csv`, `write_csv`, `parse_csv` with automatic type inference and NA reasoning
//! - Core DataFrame wrangling verbs: `select`, `head`, `tail`

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use ghl_diagnostics::Diagnostic;
use crate::value::Value;

// =========================================================================
// 1. Primitive File I/O
// =========================================================================

pub fn read_file(path: &str) -> Result<String, Diagnostic> {
    fs::read_to_string(path).map_err(|e| {
        Diagnostic::compute_error(
            "C0401",
            format!("Failed to read file `{}`: {}", path, e),
        )
    })
}

pub fn read_lines(path: &str) -> Result<Vec<String>, Diagnostic> {
    let file = fs::File::open(path).map_err(|e| {
        Diagnostic::compute_error(
            "C0401",
            format!("Failed to open file `{}`: {}", path, e),
        )
    })?;

    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for line_res in reader.lines() {
        let line = line_res.map_err(|e| {
            Diagnostic::compute_error(
                "C0401",
                format!("Failed to read lines from `{}`: {}", path, e),
            )
        })?;
        lines.push(line);
    }
    Ok(lines)
}

pub fn write_file(path: &str, content: &str) -> Result<(), Diagnostic> {
    let path_ref = Path::new(path);
    if let Some(parent) = path_ref.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                Diagnostic::compute_error(
                    "C0402",
                    format!("Failed to create directories for `{}`: {}", path, e),
                )
            })?;
        }
    }

    fs::write(path, content).map_err(|e| {
        Diagnostic::compute_error(
            "C0402",
            format!("Failed to write file `{}`: {}", path, e),
        )
    })
}

pub fn append_file(path: &str, content: &str) -> Result<(), Diagnostic> {
    let path_ref = Path::new(path);
    if let Some(parent) = path_ref.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                Diagnostic::compute_error(
                    "C0402",
                    format!("Failed to create directories for `{}`: {}", path, e),
                )
            })?;
        }
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| {
            Diagnostic::compute_error(
                "C0402",
                format!("Failed to open `{}` for appending: {}", path, e),
            )
        })?;

    file.write_all(content.as_bytes()).map_err(|e| {
        Diagnostic::compute_error(
            "C0402",
            format!("Failed to append to file `{}`: {}", path, e),
        )
    })
}

pub fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}

// =========================================================================
// 2. Tabular CSV I/O
// =========================================================================

pub fn read_csv_file(path: &str, delim: Option<char>) -> Result<Value, Diagnostic> {
    let content = read_file(path)?;
    parse_csv_string(&content, delim)
}

pub fn parse_csv_string(content: &str, delim: Option<char>) -> Result<Value, Diagnostic> {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());
    let header_line = match lines.next() {
        Some(h) => h.trim_start_matches('\u{feff}'), // strip UTF-8 BOM if present
        None => {
            return Err(Diagnostic::compute_error(
                "C0403",
                "Cannot parse empty CSV content (no header found)",
            ));
        }
    };

    // Auto-detect delimiter if not specified
    let sep = delim.unwrap_or_else(|| detect_delimiter(header_line));

    let header_fields = parse_csv_row(header_line, sep);
    if header_fields.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0403",
            "CSV header row contains zero columns",
        ));
    }

    let num_cols = header_fields.len();
    let mut raw_columns: Vec<Vec<String>> = vec![Vec::new(); num_cols];

    for (row_idx, line) in lines.enumerate() {
        let fields = parse_csv_row(line, sep);
        if fields.len() != num_cols {
            return Err(Diagnostic::compute_error(
                "C0403",
                format!(
                    "CSV Row {} has {} fields, expected {} (columns: {:?})",
                    row_idx + 2,
                    fields.len(),
                    num_cols,
                    header_fields
                ),
            ));
        }

        for (col_idx, field) in fields.into_iter().enumerate() {
            raw_columns[col_idx].push(field);
        }
    }

    // Infer typed column vectors
    let mut columns = Vec::with_capacity(num_cols);
    let mut data: HashMap<String, Vec<Value>> = HashMap::with_capacity(num_cols);

    for (col_idx, col_name) in header_fields.into_iter().enumerate() {
        let raw_vals = &raw_columns[col_idx];
        let typed_vals = infer_and_convert_column(raw_vals);
        columns.push(col_name.clone());
        data.insert(col_name, typed_vals);
    }

    Ok(Value::DataFrame { columns, data })
}

pub fn write_csv_file(df: &Value, path: &str, delim: Option<char>) -> Result<(), Diagnostic> {
    let sep = delim.unwrap_or(',');
    match df {
        Value::DataFrame { columns, data } => {
            let num_rows = columns
                .first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);

            let mut out = String::new();
            // Header
            let quoted_headers: Vec<String> = columns.iter().map(|c| escape_csv_field(c, sep)).collect();
            out.push_str(&quoted_headers.join(&sep.to_string()));
            out.push('\n');

            // Rows
            for r in 0..num_rows {
                let mut row_fields = Vec::with_capacity(columns.len());
                for col in columns {
                    if let Some(col_data) = data.get(col) {
                        if let Some(val) = col_data.get(r) {
                            let s = match val {
                                Value::String(s) => s.clone(),
                                Value::I64(n) => n.to_string(),
                                Value::F64(x) => x.to_string(),
                                Value::Bool(b) => b.to_string(),
                                Value::NA(None) => "NA".to_string(),
                                Value::NA(Some(reason)) => format!("NA:{}", reason),
                                other => format!("{other}"),
                            };
                            row_fields.push(escape_csv_field(&s, sep));
                        } else {
                            row_fields.push("NA".to_string());
                        }
                    }
                }
                out.push_str(&row_fields.join(&sep.to_string()));
                out.push('\n');
            }

            write_file(path, &out)
        }
        other => Err(Diagnostic::compute_error(
            "C0404",
            format!("`write_csv()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

// =========================================================================
// 3. DataFrame Wrangling Verbs (select, head, tail, mutate, arrange,
//    rename, drop, distinct, nrow, ncol, colnames, slice)
// =========================================================================

pub fn df_select(df: &Value, cols_to_keep: &[String]) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns: _, data } => {
            let mut new_columns = Vec::new();
            let mut new_data = HashMap::new();

            for col in cols_to_keep {
                if let Some(col_vals) = data.get(col) {
                    new_columns.push(col.clone());
                    new_data.insert(col.clone(), col_vals.clone());
                } else {
                    return Err(Diagnostic::statistical_error(
                        "S0201",
                        format!("Column `{}` not found in DataFrame for `select()`", col),
                    ));
                }
            }

            Ok(Value::DataFrame {
                columns: new_columns,
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`select()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

pub fn df_head(df: &Value, n: usize) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let mut new_data = HashMap::new();
            for col in columns {
                if let Some(col_vals) = data.get(col) {
                    let subset: Vec<Value> = col_vals.iter().take(n).cloned().collect();
                    new_data.insert(col.clone(), subset);
                }
            }
            Ok(Value::DataFrame {
                columns: columns.clone(),
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`head()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

pub fn df_tail(df: &Value, n: usize) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let mut new_data = HashMap::new();
            for col in columns {
                if let Some(col_vals) = data.get(col) {
                    let total = col_vals.len();
                    let start = total.saturating_sub(n);
                    let subset: Vec<Value> = col_vals[start..].to_vec();
                    new_data.insert(col.clone(), subset);
                }
            }
            Ok(Value::DataFrame {
                columns: columns.clone(),
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`tail()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `mutate(df, "new_col", values_vector)` — add or replace a column with pre-computed values.
///
/// Pipe-friendly: `df |> mutate("log_dose", log_vals)`
pub fn df_mutate(df: &Value, col_name: &str, new_values: Vec<Value>) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let num_rows = columns.first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);

            if !new_values.is_empty() && new_values.len() != num_rows && num_rows > 0 {
                return Err(Diagnostic::compute_error(
                    "C0205",
                    format!(
                        "`mutate()`: column `{}` has {} values, but DataFrame has {} rows",
                        col_name, new_values.len(), num_rows
                    ),
                ));
            }

            let mut new_columns = columns.clone();
            let mut new_data = data.clone();

            if !new_data.contains_key(col_name) {
                new_columns.push(col_name.to_string());
            }
            new_data.insert(col_name.to_string(), new_values);

            Ok(Value::DataFrame {
                columns: new_columns,
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`mutate()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// Total order over two optional cell values, used by `arrange()`, `rank()`, `sort_asc()`/`sort_desc()`.
/// Mixed/incomparable types (including missing cells) compare as equal rather than panicking.
pub(crate) fn compare_values(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(Value::I64(x)), Some(Value::I64(y))) => x.cmp(y),
        (Some(Value::F64(x)), Some(Value::F64(y))) => {
            x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Some(Value::I64(x)), Some(Value::F64(y))) => {
            (*x as f64).partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Some(Value::F64(x)), Some(Value::I64(y))) => {
            x.partial_cmp(&(*y as f64)).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Some(Value::String(x)), Some(Value::String(y))) => x.cmp(y),
        (Some(Value::Bool(x)), Some(Value::Bool(y))) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    }
}

/// `arrange(df, col1, col2, ...)` — stable, multi-column sort. Each `(col, desc)` pair
/// is tried in order, falling through to the next column on ties.
pub fn df_arrange(df: &Value, specs: &[(String, bool)]) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let mut sort_cols: Vec<(&Vec<Value>, bool)> = Vec::with_capacity(specs.len());
            for (col_name, desc) in specs {
                let col_vals = data.get(col_name).ok_or_else(|| {
                    Diagnostic::statistical_error(
                        "S0201",
                        format!("Column `{}` not found in DataFrame for `arrange()`", col_name),
                    )
                })?;
                sort_cols.push((col_vals, *desc));
            }

            let num_rows = columns.first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);
            let mut indices: Vec<usize> = (0..num_rows).collect();

            indices.sort_by(|&a, &b| {
                for (col_vals, desc) in &sort_cols {
                    let ord = compare_values(col_vals.get(a), col_vals.get(b));
                    if ord != std::cmp::Ordering::Equal {
                        return if *desc { ord.reverse() } else { ord };
                    }
                }
                std::cmp::Ordering::Equal
            });

            let mut new_data = HashMap::new();
            for col in columns {
                if let Some(col_vals) = data.get(col) {
                    let sorted: Vec<Value> = indices.iter()
                        .filter_map(|&i| col_vals.get(i))
                        .cloned()
                        .collect();
                    new_data.insert(col.clone(), sorted);
                }
            }

            Ok(Value::DataFrame {
                columns: columns.clone(),
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`arrange()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `slice_min(df, col, n)` — the `n` rows with the smallest `col` value.
pub fn df_slice_min(df: &Value, col: &str, n: usize) -> Result<Value, Diagnostic> {
    let sorted = df_arrange(df, &[(col.to_string(), false)])?;
    df_head(&sorted, n)
}

/// `slice_max(df, col, n)` — the `n` rows with the largest `col` value.
pub fn df_slice_max(df: &Value, col: &str, n: usize) -> Result<Value, Diagnostic> {
    let sorted = df_arrange(df, &[(col.to_string(), true)])?;
    df_head(&sorted, n)
}

fn xorshift_next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn sample_indices(num_rows: usize, k: usize) -> Vec<usize> {
    let k = k.min(num_rows);
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        | 1; // xorshift requires a non-zero state

    let mut indices: Vec<usize> = (0..num_rows).collect();
    for i in 0..k {
        let r = i + (xorshift_next(&mut state) as usize) % (num_rows - i);
        indices.swap(i, r);
    }
    indices.truncate(k);
    indices
}

/// `sample_n(df, n)` — `n` random rows, without replacement.
pub fn df_sample_n(df: &Value, n: usize) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let num_rows = columns.first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);
            let indices = sample_indices(num_rows, n);

            let mut new_data = HashMap::new();
            for col in columns {
                if let Some(col_vals) = data.get(col) {
                    let subset: Vec<Value> = indices.iter()
                        .filter_map(|&i| col_vals.get(i))
                        .cloned()
                        .collect();
                    new_data.insert(col.clone(), subset);
                }
            }
            Ok(Value::DataFrame { columns: columns.clone(), data: new_data })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`sample_n()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `sample_frac(df, frac)` — a random `frac` fraction of rows, without replacement.
pub fn df_sample_frac(df: &Value, frac: f64) -> Result<Value, Diagnostic> {
    let num_rows = match df {
        Value::DataFrame { columns, data } => columns.first()
            .and_then(|c| data.get(c))
            .map(|v| v.len())
            .unwrap_or(0),
        other => {
            return Err(Diagnostic::compute_error(
                "C0201",
                format!("`sample_frac()` requires a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };
    let n = ((num_rows as f64) * frac).round().max(0.0) as usize;
    df_sample_n(df, n)
}

/// `rename(df, "old_name", "new_name")` — rename a column non-destructively.
pub fn df_rename(df: &Value, old_name: &str, new_name: &str) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            if !data.contains_key(old_name) {
                return Err(Diagnostic::statistical_error(
                    "S0201",
                    format!("Column `{}` not found in DataFrame for `rename()`", old_name),
                ));
            }
            if data.contains_key(new_name) {
                return Err(Diagnostic::compute_error(
                    "C0206",
                    format!("`rename()`: target column `{}` already exists", new_name),
                ));
            }

            let new_columns: Vec<String> = columns.iter()
                .map(|c| if c == old_name { new_name.to_string() } else { c.clone() })
                .collect();

            let mut new_data = HashMap::new();
            for (k, v) in data {
                let key = if k == old_name { new_name.to_string() } else { k.clone() };
                new_data.insert(key, v.clone());
            }

            Ok(Value::DataFrame {
                columns: new_columns,
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`rename()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `drop(df, ["col_a", "col_b"])` — remove columns from the DataFrame.
pub fn df_drop(df: &Value, cols_to_drop: &[String]) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let drop_set: std::collections::HashSet<&str> =
                cols_to_drop.iter().map(|s| s.as_str()).collect();

            let new_columns: Vec<String> = columns.iter()
                .filter(|c| !drop_set.contains(c.as_str()))
                .cloned()
                .collect();

            let new_data: HashMap<String, Vec<Value>> = data.iter()
                .filter(|(k, _)| !drop_set.contains(k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            Ok(Value::DataFrame {
                columns: new_columns,
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`drop()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `distinct(df)` — remove duplicate rows (all columns checked).
/// `distinct(df, ["col"])` — deduplicate by specific key columns.
pub fn df_distinct(df: &Value, key_cols: Option<&[String]>) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let num_rows = columns.first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);

            let check_cols: Vec<&String> = match key_cols {
                Some(ks) => ks.iter().collect(),
                None => columns.iter().collect(),
            };

            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut keep_indices: Vec<usize> = Vec::new();

            for row_idx in 0..num_rows {
                let row_key: String = check_cols.iter()
                    .map(|col| {
                        data.get(*col)
                            .and_then(|cv| cv.get(row_idx))
                            .map(|v| format!("{:?}", v))
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join("|");

                if seen.insert(row_key) {
                    keep_indices.push(row_idx);
                }
            }

            let mut new_data = HashMap::new();
            for col in columns {
                if let Some(col_vals) = data.get(col) {
                    let filtered: Vec<Value> = keep_indices.iter()
                        .filter_map(|&i| col_vals.get(i))
                        .cloned()
                        .collect();
                    new_data.insert(col.clone(), filtered);
                }
            }

            Ok(Value::DataFrame {
                columns: columns.clone(),
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`distinct()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `nrow(df)` — number of rows.
pub fn df_nrow(df: &Value) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let n = columns.first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);
            Ok(Value::I64(n as i64))
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`nrow()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `ncol(df)` — number of columns.
pub fn df_ncol(df: &Value) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, .. } => Ok(Value::I64(columns.len() as i64)),
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`ncol()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `colnames(df)` — returns a `Vector[String]` of column names.
pub fn df_colnames(df: &Value) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, .. } => {
            let names = columns.iter().map(|c| Value::String(c.clone())).collect();
            Ok(Value::Vector(names))
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`colnames()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `slice(df, from, to)` — slice rows by 0-based exclusive range [from, to).
pub fn df_slice(df: &Value, from: usize, to: usize) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let num_rows = columns.first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);
            let end = to.min(num_rows);

            let mut new_data = HashMap::new();
            for col in columns {
                if let Some(col_vals) = data.get(col) {
                    let subset = col_vals.get(from..end)
                        .map(|s| s.to_vec())
                        .unwrap_or_default();
                    new_data.insert(col.clone(), subset);
                }
            }
            Ok(Value::DataFrame {
                columns: columns.clone(),
                data: new_data,
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`slice()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

// =========================================================================
// 4. Grouping / summarizing + remaining column helpers
// =========================================================================

/// `group_by(df, key1, key2, ...)` — partitions rows by one or more key columns.
/// Returns a `GroupedDataFrame`; only `summarize()`/`ungroup()` consume it, so grouped
/// state never leaks into unrelated verbs.
pub fn df_group_by(df: &Value, keys: &[String]) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            for k in keys {
                if !data.contains_key(k) {
                    return Err(Diagnostic::statistical_error(
                        "S0201",
                        format!("Column `{}` not found in DataFrame for `group_by()`", k),
                    ));
                }
            }

            let num_rows = columns.first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);

            let mut group_index: HashMap<String, usize> = HashMap::new();
            let mut groups: Vec<(Vec<Value>, Vec<usize>)> = Vec::new();

            for row in 0..num_rows {
                let key_vals: Vec<Value> = keys.iter()
                    .map(|k| data[k].get(row).cloned().unwrap_or(Value::NA(None)))
                    .collect();
                let hash_key: String = key_vals.iter()
                    .map(|v| format!("{:?}", v))
                    .collect::<Vec<_>>()
                    .join("\u{1}");

                match group_index.get(&hash_key) {
                    Some(&idx) => groups[idx].1.push(row),
                    None => {
                        group_index.insert(hash_key, groups.len());
                        groups.push((key_vals, vec![row]));
                    }
                }
            }

            Ok(Value::GroupedDataFrame {
                keys: keys.to_vec(),
                groups,
                columns: columns.clone(),
                data: data.clone(),
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`group_by()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

fn compute_agg(
    kind: &str,
    col: Option<&str>,
    data: &HashMap<String, Vec<Value>>,
    indices: &[usize],
) -> Result<Value, Diagnostic> {
    if kind == "count" {
        return Ok(Value::I64(indices.len() as i64));
    }

    let col_name = col.ok_or_else(|| {
        Diagnostic::compute_error("C0201", format!("`{}()` requires a column argument in `summarize()`", kind))
    })?;
    let col_vals = data.get(col_name).ok_or_else(|| {
        Diagnostic::statistical_error(
            "S0201",
            format!("Column `{}` not found in DataFrame for `summarize()`", col_name),
        )
    })?;
    let subset: Vec<Value> = indices.iter().filter_map(|&i| col_vals.get(i).cloned()).collect();

    // Reuse the same aggregation math the standalone functions use, so `mean(x)`
    // inside `summarize()` and `mean(pull(df, "x"))` outside it never disagree.
    let native_fn: crate::value::NativeFunction = match kind {
        "mean" => crate::env::native_mean,
        "sum" => crate::env::native_sum,
        "std_dev" => crate::env::native_std_dev,
        "var" => crate::env::native_var,
        "min" => crate::env::native_min,
        "max" => crate::env::native_max,
        "first" => crate::env::native_first,
        "last" => crate::env::native_last,
        "median" => crate::env::native_median,
        "n_distinct" => crate::env::native_n_distinct,
        other => {
            return Err(Diagnostic::compute_error(
                "C0201",
                format!("Unknown aggregation `{}` in `summarize()`", other),
            ));
        }
    };
    native_fn(vec![Value::Vector(subset)])
}

/// `summarize(gdf, name = agg, ...)` — consumes the `GroupedDataFrame`, returns a plain
/// `DataFrame`. `specs` is `(output_name, agg_kind, source_col)`.
pub fn df_summarize(gdf: &Value, specs: &[(String, String, Option<String>)]) -> Result<Value, Diagnostic> {
    match gdf {
        Value::GroupedDataFrame { keys, groups, data, .. } => {
            let mut out_columns: Vec<String> = keys.clone();
            let mut out_data: HashMap<String, Vec<Value>> = HashMap::new();
            for k in keys {
                out_data.insert(k.clone(), Vec::with_capacity(groups.len()));
            }
            for (name, _, _) in specs {
                out_columns.push(name.clone());
                out_data.insert(name.clone(), Vec::with_capacity(groups.len()));
            }

            for (key_vals, indices) in groups {
                for (k, v) in keys.iter().zip(key_vals.iter()) {
                    out_data.get_mut(k).unwrap().push(v.clone());
                }
                for (name, kind, col) in specs {
                    let result = compute_agg(kind, col.as_deref(), data, indices)?;
                    out_data.get_mut(name).unwrap().push(result);
                }
            }

            Ok(Value::DataFrame { columns: out_columns, data: out_data })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!(
                "`summarize()` requires a GroupedDataFrame (did you forget `group_by()`?), found `{}`",
                other.type_name()
            ),
        )),
    }
}

/// `pull(df, col)` — extract a column as a plain `Vector`.
pub fn df_pull(df: &Value, col: &str) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { data, .. } => data.get(col).cloned().map(Value::Vector).ok_or_else(|| {
            Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame for `pull()`", col))
        }),
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`pull()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `fill_na(df, col, default)` — replace `NA` in one column with a constant.
pub fn df_fill_na(df: &Value, col: &str, default: &Value) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let col_vals = data.get(col).ok_or_else(|| {
                Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame for `fill_na()`", col))
            })?;
            let filled: Vec<Value> = col_vals.iter()
                .map(|v| if v.is_na() { default.clone() } else { v.clone() })
                .collect();
            let mut new_data = data.clone();
            new_data.insert(col.to_string(), filled);
            Ok(Value::DataFrame { columns: columns.clone(), data: new_data })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`fill_na()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `fill_na_all(df, default)` — replace `NA` in every column with a constant.
pub fn df_fill_na_all(df: &Value, default: &Value) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let mut new_data = HashMap::new();
            for col in columns {
                if let Some(col_vals) = data.get(col) {
                    let filled: Vec<Value> = col_vals.iter()
                        .map(|v| if v.is_na() { default.clone() } else { v.clone() })
                        .collect();
                    new_data.insert(col.clone(), filled);
                }
            }
            Ok(Value::DataFrame { columns: columns.clone(), data: new_data })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`fill_na_all()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `count(df, col)` — frequency table: `DataFrame[col, n]`, one row per distinct value.
pub fn df_count(df: &Value, col: &str) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { data, .. } => {
            let col_vals = data.get(col).ok_or_else(|| {
                Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame for `count()`", col))
            })?;

            let mut order: Vec<Value> = Vec::new();
            let mut counts: HashMap<String, i64> = HashMap::new();
            for v in col_vals {
                let key = format!("{:?}", v);
                if !counts.contains_key(&key) {
                    order.push(v.clone());
                }
                *counts.entry(key).or_insert(0) += 1;
            }

            let n_out: Vec<Value> = order.iter()
                .map(|v| Value::I64(*counts.get(&format!("{:?}", v)).unwrap_or(&0)))
                .collect();

            let mut new_data = HashMap::new();
            new_data.insert(col.to_string(), order);
            new_data.insert("n".to_string(), n_out);

            Ok(Value::DataFrame { columns: vec![col.to_string(), "n".to_string()], data: new_data })
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`count()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// `glimpse(df)` — compact per-column overview (name, type, first few values); prints, returns `Unit`.
pub fn df_glimpse(df: &Value) -> Result<Value, Diagnostic> {
    match df {
        Value::DataFrame { columns, data } => {
            let num_rows = columns.first()
                .and_then(|c| data.get(c))
                .map(|v| v.len())
                .unwrap_or(0);
            println!("Rows: {}", num_rows);
            println!("Columns: {}", columns.len());
            for col in columns {
                if let Some(col_vals) = data.get(col) {
                    let ty = col_vals.iter().find(|v| !v.is_na()).map(|v| v.type_name()).unwrap_or("any");
                    let preview: Vec<String> = col_vals.iter().take(10).map(|v| v.to_string()).collect();
                    println!("$ {:<15} <{}> {}", col, ty, preview.join(", "));
                }
            }
            Ok(Value::Unit)
        }
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`glimpse()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

// =========================================================================
// Helper parsing functions
// =========================================================================

fn detect_delimiter(header: &str) -> char {
    let commas = header.chars().filter(|&c| c == ',').count();
    let semicolons = header.chars().filter(|&c| c == ';').count();
    let tabs = header.chars().filter(|&c| c == '\t').count();

    if tabs > commas && tabs > semicolons {
        '\t'
    } else if semicolons > commas {
        ';'
    } else {
        ','
    }
}

fn parse_csv_row(line: &str, sep: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    // Escaped quote: ""
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ch if ch == sep && !in_quotes => {
                fields.push(cur.trim().to_string());
                cur.clear();
            }
            ch => {
                cur.push(ch);
            }
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

fn escape_csv_field(field: &str, sep: char) -> String {
    if field.contains(sep) || field.contains('"') || field.contains('\n') {
        let escaped = field.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        field.to_string()
    }
}

fn infer_and_convert_column(raw: &[String]) -> Vec<Value> {
    // 1. Identify non-NA values
    let non_na_entries: Vec<&str> = raw
        .iter()
        .map(|s| s.as_str())
        .filter(|&s| !is_na_token(s))
        .collect();

    if non_na_entries.is_empty() {
        return raw.iter().map(|s| parse_single_val(s)).collect();
    }

    // 2. Check if all non-NA parse as i64
    let all_i64 = non_na_entries.iter().all(|s| s.parse::<i64>().is_ok());
    if all_i64 {
        return raw
            .iter()
            .map(|s| {
                if is_na_token(s) {
                    parse_na_token(s)
                } else {
                    Value::I64(s.parse::<i64>().unwrap())
                }
            })
            .collect();
    }

    // 3. Check if all non-NA parse as f64
    let all_f64 = non_na_entries.iter().all(|s| s.parse::<f64>().is_ok());
    if all_f64 {
        return raw
            .iter()
            .map(|s| {
                if is_na_token(s) {
                    parse_na_token(s)
                } else {
                    Value::F64(s.parse::<f64>().unwrap())
                }
            })
            .collect();
    }

    // 4. Check if all non-NA parse as bool
    let all_bool = non_na_entries.iter().all(|s| {
        let lower = s.to_lowercase();
        lower == "true" || lower == "false" || lower == "t" || lower == "f"
    });
    if all_bool {
        return raw
            .iter()
            .map(|s| {
                if is_na_token(s) {
                    parse_na_token(s)
                } else {
                    let lower = s.to_lowercase();
                    Value::Bool(lower == "true" || lower == "t")
                }
            })
            .collect();
    }

    // 5. Fallback: string
    raw.iter()
        .map(|s| {
            if is_na_token(s) {
                parse_na_token(s)
            } else {
                Value::String(s.clone())
            }
        })
        .collect()
}

fn is_na_token(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t == "NA" || t == "NaN" || t == "null" || t == "None" || t.starts_with("NA:")
}

fn parse_na_token(s: &str) -> Value {
    let t = s.trim();
    if let Some(reason) = t.strip_prefix("NA:") {
        Value::NA(Some(reason.to_string()))
    } else {
        Value::NA(None)
    }
}

fn parse_single_val(s: &str) -> Value {
    if is_na_token(s) {
        parse_na_token(s)
    } else if let Ok(n) = s.parse::<i64>() {
        Value::I64(n)
    } else if let Ok(x) = s.parse::<f64>() {
        Value::F64(x)
    } else if s.eq_ignore_ascii_case("true") {
        Value::Bool(true)
    } else if s.eq_ignore_ascii_case("false") {
        Value::Bool(false)
    } else {
        Value::String(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_file_io() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("ghl_test_file.txt");
        let p_str = path.to_str().unwrap();

        assert!(write_file(p_str, "Hello GHL\nLine 2\n").is_ok());
        assert!(file_exists(p_str));

        let content = read_file(p_str).unwrap();
        assert_eq!(content, "Hello GHL\nLine 2\n");

        let lines = read_lines(p_str).unwrap();
        assert_eq!(lines, vec!["Hello GHL", "Line 2"]);

        assert!(append_file(p_str, "Line 3\n").is_ok());
        let updated_lines = read_lines(p_str).unwrap();
        assert_eq!(updated_lines.len(), 3);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_csv_parser_with_inferred_types() {
        let csv_data = r#"id,name,score,active,sensor_status
1,Alpha,98.5,true,OK
2,Beta,NA,false,NA:SensorDropout
3,Gamma,87.2,true,OK
4,Delta,76.0,false,NA:LowBattery
"#;
        let df_val = parse_csv_string(csv_data, None).expect("CSV parsed");
        if let Value::DataFrame { columns, data } = df_val {
            assert_eq!(columns, vec!["id", "name", "score", "active", "sensor_status"]);

            // id is i64
            let ids = data.get("id").unwrap();
            assert_eq!(ids[0], Value::I64(1));
            assert_eq!(ids[1], Value::I64(2));

            // score is f64 with reasoned NA
            let scores = data.get("score").unwrap();
            assert_eq!(scores[0], Value::F64(98.5));
            assert_eq!(scores[1], Value::NA(None));

            // sensor_status with reasoned NA
            let sensors = data.get("sensor_status").unwrap();
            assert_eq!(sensors[0], Value::String("OK".into()));
            assert_eq!(sensors[1], Value::NA(Some("SensorDropout".into())));
            assert_eq!(sensors[3], Value::NA(Some("LowBattery".into())));
        } else {
            panic!("Expected DataFrame");
        }
    }

    #[test]
    fn test_df_select_head_tail() {
        let csv_data = "x,y,z\n1,10,100\n2,20,200\n3,30,300\n4,40,400\n5,50,500\n";
        let df = parse_csv_string(csv_data, None).unwrap();

        let selected = df_select(&df, &["z".into(), "x".into()]).unwrap();
        if let Value::DataFrame { columns, data } = selected {
            assert_eq!(columns, vec!["z", "x"]);
            assert_eq!(data.get("z").unwrap().len(), 5);
        }

        let h = df_head(&df, 2).unwrap();
        if let Value::DataFrame { columns: _, data } = h {
            assert_eq!(data.get("x").unwrap().len(), 2);
            assert_eq!(data.get("x").unwrap()[1], Value::I64(2));
        }

        let t = df_tail(&df, 2).unwrap();
        if let Value::DataFrame { columns: _, data } = t {
            assert_eq!(data.get("x").unwrap().len(), 2);
            assert_eq!(data.get("x").unwrap()[0], Value::I64(4));
            assert_eq!(data.get("x").unwrap()[1], Value::I64(5));
        }
    }
}
