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
// 3. DataFrame Wrangling Verbs (`select`, `head`, `tail`)
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
