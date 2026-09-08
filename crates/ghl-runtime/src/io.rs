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
use ghl_syntax::ast::BinaryOp;
use polars_core::prelude::*;
use polars_io::prelude::*;
use polars_ops::prelude::*;
use rayon::prelude::*;
use crate::na_reasons::NaReasonTable;
use crate::polars_bridge;
use crate::value::Value;

/// Extrae `&DataFrame`/`&NaReasonTable` de un `Value`, o el diagnóstico de error estándar
/// que usan los ~30 verbos de este módulo cuando el primer argumento no es un DataFrame.
fn as_dataframe<'a>(df: &'a Value, verb: &str) -> Result<(&'a DataFrame, &'a NaReasonTable), Diagnostic> {
    match df {
        Value::DataFrame { frame, na_reasons } => Ok((frame, na_reasons)),
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("`{verb}()` requires a DataFrame, found `{}`", other.type_name()),
        )),
    }
}

/// Aplica una permutación/subconjunto de filas (por posición absoluta en `frame`) a la
/// vez sobre el `DataFrame` y su `NaReasonTable`, dejando ambos alineados — el mismo
/// mecanismo validado en el Spike #2, ahora compartido por `arrange`/`slice`/`head`/
/// `tail`/`sample_n`/`distinct`.
pub(crate) fn take_rows(frame: &DataFrame, na_reasons: &NaReasonTable, indices: &[usize]) -> Result<(DataFrame, NaReasonTable), Diagnostic> {
    let idx: Vec<IdxSize> = indices.iter().map(|&i| i as IdxSize).collect();
    let idx_ca = IdxCa::from_vec(PlSmallStr::EMPTY, idx);
    let new_frame = frame.take(&idx_ca).map_err(|e| {
        Diagnostic::compute_error("C0210", format!("Error seleccionando filas: {e}"))
    })?;
    Ok((new_frame, na_reasons.reindex(indices)))
}

fn value_to_any_value(v: &Value) -> AnyValue<'static> {
    match v {
        Value::I64(n) => AnyValue::Int64(*n),
        Value::F64(x) => AnyValue::Float64(*x),
        Value::Bool(b) => AnyValue::Boolean(*b),
        Value::String(s) => AnyValue::StringOwned(s.as_str().into()),
        _ => AnyValue::Null,
    }
}

/// `col(...) OP scalar` (`filter(df, col("score") > 75.0)`) — Caso 2.3 de Suite 02:
/// "filtrado vectorial mediante bitmasks de validez Arrow", en vez de boxear la columna
/// entera a `Vec<Value>` y comparar celda por celda en Rust (lo que hacía la versión
/// anterior de `filter()`). El escalar se envuelve en un `Column::Scalar` de largo
/// lógico igual al frame (sin materializarlo — broadcast real, no una copia de N
/// elementos) y la comparación corre nativa en polars, dando directo un `BooleanChunked`.
pub(crate) fn colref_predicate_mask(
    frame: &DataFrame,
    col: &str,
    op: BinaryOp,
    rhs: &Value,
) -> Result<BooleanChunked, Diagnostic> {
    let left = frame.column(col).map_err(|_| {
        Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame for `filter()`", col))
    })?;

    let av = value_to_any_value(rhs);
    let scalar = Scalar::new(av.dtype(), av);
    let right = Column::new_scalar(PlSmallStr::EMPTY, scalar, frame.height());

    let result = match op {
        BinaryOp::Gt => left.gt(&right),
        BinaryOp::GtEq => left.gt_eq(&right),
        BinaryOp::Lt => left.lt(&right),
        BinaryOp::LtEq => left.lt_eq(&right),
        BinaryOp::Eq => left.equal(&right),
        BinaryOp::NotEq => left.not_equal(&right),
        other => {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!("`filter()` does not support operator `{:?}` on a column comparison", other),
            ));
        }
    };
    result.map_err(|e| Diagnostic::compute_error("C0210", format!("`filter()` comparison failed: {e}")))
}

/// Row positions (in order) where `mask` is `true` — nulls in the mask count as `false`,
/// matching Kleene semantics (an unknown condition doesn't pass a filter). Shared by
/// `filter()`'s vectorized path and anything else that needs indices back from a
/// `BooleanChunked` for `take_rows`/`NaReasonTable::reindex`.
pub(crate) fn mask_to_indices(mask: &BooleanChunked) -> Vec<usize> {
    mask.iter().enumerate().filter_map(|(i, v)| v.unwrap_or(false).then_some(i)).collect()
}

/// `filter(df, col OP scalar)` end to end: computes the vectorized mask, then applies it
/// to both the frame and the NA-reason table via the same `take_rows` every other
/// row-preserving verb uses.
pub fn df_filter_by_col_predicate(
    df: &Value,
    col: &str,
    op: BinaryOp,
    rhs: &Value,
) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "filter")?;
    let mask = colref_predicate_mask(frame, col, op, rhs)?;
    let indices = mask_to_indices(&mask);
    let (new_frame, new_reasons) = take_rows(frame, na_reasons, &indices)?;
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

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

/// Reads a CSV file at GB scale (`benchmarks/suites/02`, Caso 2.1) using polars-io's
/// multi-threaded reader for the expensive part (splitting the file into fields across
/// however many rows it has), then runs GHL's own type inference and `NA:reason` parsing
/// (`infer_and_convert_column`, unchanged) on top of the resulting string columns.
///
/// This is a deliberate hybrid rather than handing the whole read to polars: polars'
/// native numeric parsing has no notion of `NA:SensorDropout` — a single such cell in a
/// numeric column would either force that whole column to `String` or get silently
/// dropped, depending on how strict the reader is configured. Forcing every column to
/// `String` via `dtype_overwrite` sidesteps that entirely: polars only does the fast
/// I/O + tokenizing, and the exact same inference/NA-reason logic already covered by
/// `test_csv_parser_with_inferred_types` runs unchanged on the result.
pub fn read_csv_file(path: &str, delim: Option<char>) -> Result<Value, Diagnostic> {
    let header_line = {
        let file = fs::File::open(path).map_err(|e| {
            Diagnostic::compute_error("C0401", format!("Failed to open file `{}`: {}", path, e))
        })?;
        let mut lines = BufReader::new(file).lines();
        loop {
            match lines.next() {
                Some(Ok(l)) if l.trim().is_empty() => continue,
                Some(Ok(l)) => break l,
                Some(Err(e)) => {
                    return Err(Diagnostic::compute_error("C0401", format!("Failed to read `{}`: {}", path, e)));
                }
                None => {
                    return Err(Diagnostic::compute_error("C0403", "Cannot parse empty CSV content (no header found)"));
                }
            }
        }
    };
    let header_line = header_line.trim_start_matches('\u{feff}');
    let sep = delim.unwrap_or_else(|| detect_delimiter(header_line));
    let num_cols = parse_csv_row(header_line, sep).len();
    if num_cols == 0 {
        return Err(Diagnostic::compute_error("C0403", "CSV header row contains zero columns"));
    }

    let parse_options = CsvParseOptions::default().with_separator(sep as u8);
    let dtype_overwrite = std::sync::Arc::new(vec![DataType::String; num_cols]);
    let raw_frame = CsvReadOptions::default()
        .with_has_header(true)
        .with_parse_options(parse_options)
        .with_dtype_overwrite(Some(dtype_overwrite))
        .try_into_reader_with_file_path(Some(std::path::PathBuf::from(path)))
        .and_then(|reader| reader.finish())
        .map_err(|e| Diagnostic::compute_error("C0403", format!("Failed to read CSV `{}`: {}", path, e)))?;

    // Columns are independent of each other, so the per-column type-inference/NA-reason
    // pass (several full scans over each column, see infer_and_convert_column) runs
    // across `rayon`'s thread pool rather than one column at a time -- this is where
    // most of the wall-clock time actually goes once polars-io has done the fast part
    // (tokenizing the file), so it's worth parallelizing even with as few as ~12 columns.
    let cols: Vec<(String, Vec<Value>)> = raw_frame
        .get_column_names()
        .into_par_iter()
        .map(|name| -> Result<(String, Vec<Value>), Diagnostic> {
            let raw_col = raw_frame.column(name).map_err(|e| {
                Diagnostic::compute_error("C0403", format!("Internal error reading column `{name}`: {e}"))
            })?;
            let raw_strs = raw_col.str().map_err(|e| {
                Diagnostic::compute_error("C0403", format!("Internal error reading column `{name}`: {e}"))
            })?;
            let raw_vals: Vec<String> = raw_strs.iter().map(|opt| opt.unwrap_or("").to_string()).collect();
            Ok((name.to_string(), infer_and_convert_column(&raw_vals)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let (frame, na_reasons) = polars_bridge::build_dataframe(&cols)?;
    Ok(Value::DataFrame { frame, na_reasons })
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

    // Infer typed column vectors, then build the real polars-backed DataFrame through
    // the same construction boundary the `dataframe { ... }` literal uses.
    let mut cols = Vec::with_capacity(num_cols);
    for (col_idx, col_name) in header_fields.into_iter().enumerate() {
        let typed_vals = infer_and_convert_column(&raw_columns[col_idx]);
        cols.push((col_name, typed_vals));
    }

    let (frame, na_reasons) = polars_bridge::build_dataframe(&cols)?;
    Ok(Value::DataFrame { frame, na_reasons })
}

pub fn write_csv_file(df: &Value, path: &str, delim: Option<char>) -> Result<(), Diagnostic> {
    let sep = delim.unwrap_or(',');
    let (frame, na_reasons) = as_dataframe(df, "write_csv")?;

    let columns: Vec<String> = frame.get_column_names().iter().map(|s| s.to_string()).collect();
    let num_rows = frame.height();

    let mut out = String::new();
    let quoted_headers: Vec<String> = columns.iter().map(|c| escape_csv_field(c, sep)).collect();
    out.push_str(&quoted_headers.join(&sep.to_string()));
    out.push('\n');

    let col_values: Vec<Vec<Value>> = columns
        .iter()
        .map(|c| polars_bridge::pull_column_as_values(frame, na_reasons, c))
        .collect::<Result<_, _>>()?;

    for r in 0..num_rows {
        let mut row_fields = Vec::with_capacity(columns.len());
        for values in &col_values {
            let s = match &values[r] {
                Value::String(s) => s.clone(),
                Value::I64(n) => n.to_string(),
                Value::F64(x) => x.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::NA(None) => "NA".to_string(),
                Value::NA(Some(reason)) => format!("NA:{}", reason),
                other => format!("{other}"),
            };
            row_fields.push(escape_csv_field(&s, sep));
        }
        out.push_str(&row_fields.join(&sep.to_string()));
        out.push('\n');
    }

    write_file(path, &out)
}

/// Reads a Parquet file (`benchmarks/README.md` mentions it alongside CSV; Fase 1).
/// Unlike CSV, Parquet has its own native validity bitmap — there's no `NA:Reason` text
/// to reinterpret, so nulls always come back as plain `Value::NA(None)`. No hybrid
/// string-then-infer step is needed either: Parquet is already typed and columnar, so
/// this is a direct, fully multi-threaded read straight into the target representation.
pub fn read_parquet_file(path: &str) -> Result<Value, Diagnostic> {
    let file = fs::File::open(path).map_err(|e| {
        Diagnostic::compute_error("C0401", format!("Failed to open file `{}`: {}", path, e))
    })?;
    let frame = ParquetReader::new(file).finish().map_err(|e| {
        Diagnostic::compute_error("C0405", format!("Failed to read Parquet `{}`: {}", path, e))
    })?;
    Ok(Value::DataFrame { frame, na_reasons: NaReasonTable::new() })
}

/// Writes a `DataFrame` to Parquet. NA-with-reason is intentionally not persisted:
/// Parquet's binary format has no text cell to encode `NA:Reason` into the way CSV does
/// (where `write_csv`/`read_csv` round-trip it for free), and inventing a side-channel
/// column/metadata convention for it would be exactly the kind of extra complexity the
/// NA-reason design decided against (TODO.md, Fase 0). A visible notice beats silently
/// dropping data the caller might not expect to lose.
pub fn write_parquet_file(df: &Value, path: &str) -> Result<(), Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "write_parquet")?;
    if !na_reasons.is_empty() {
        eprintln!(
            "(U・ᴥ・U) `write_parquet()`: this DataFrame has NA reasons recorded (na_reasons()); \
             Parquet has no slot for them, so they will not round-trip through this file."
        );
    }

    let file = fs::File::create(path).map_err(|e| {
        Diagnostic::compute_error("C0402", format!("Failed to create file `{}`: {}", path, e))
    })?;
    let mut frame = frame.clone();
    ParquetWriter::new(file).finish(&mut frame).map_err(|e| {
        Diagnostic::compute_error("C0405", format!("Failed to write Parquet `{}`: {}", path, e))
    })?;
    Ok(())
}

// =========================================================================
// 3. DataFrame Wrangling Verbs (select, head, tail, mutate, arrange,
//    rename, drop, distinct, nrow, ncol, colnames, slice)
// =========================================================================

pub fn df_select(df: &Value, cols_to_keep: &[String]) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "select")?;

    for col in cols_to_keep {
        if frame.column(col).is_err() {
            return Err(Diagnostic::statistical_error(
                "S0201",
                format!("Column `{}` not found in DataFrame for `select()`", col),
            ));
        }
    }

    let new_frame = frame.select(cols_to_keep.iter().map(|s| s.as_str())).map_err(|e| {
        Diagnostic::compute_error("C0210", format!("`select()` failed: {e}"))
    })?;
    let new_reasons = na_reasons.retain_columns(cols_to_keep);
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

pub fn df_head(df: &Value, n: usize) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "head")?;
    let indices: Vec<usize> = (0..n.min(frame.height())).collect();
    let (new_frame, new_reasons) = take_rows(frame, na_reasons, &indices)?;
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

pub fn df_tail(df: &Value, n: usize) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "tail")?;
    let start = frame.height().saturating_sub(n);
    let indices: Vec<usize> = (start..frame.height()).collect();
    let (new_frame, new_reasons) = take_rows(frame, na_reasons, &indices)?;
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

/// `mutate(df, "new_col", values_vector)` — add or replace a column with pre-computed values.
///
/// Pipe-friendly: `df |> mutate("log_dose", log_vals)`
pub fn df_mutate(df: &Value, col_name: &str, new_values: Vec<Value>) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "mutate")?;
    let num_rows = frame.height();

    if !new_values.is_empty() && new_values.len() != num_rows && num_rows > 0 {
        return Err(Diagnostic::compute_error(
            "C0205",
            format!(
                "`mutate()`: column `{}` has {} values, but DataFrame has {} rows",
                col_name, new_values.len(), num_rows
            ),
        ));
    }

    // Broadcast a single scalar value to every row (`mutate(df, "flag", true)`).
    let new_values = if new_values.len() == 1 && num_rows > 1 {
        vec![new_values[0].clone(); num_rows]
    } else {
        new_values
    };

    let mut new_reasons = na_reasons.without_column(col_name);
    for (row, v) in new_values.iter().enumerate() {
        if let Value::NA(Some(reason)) = v {
            new_reasons.set(col_name, row, reason.clone());
        }
    }

    let column = polars_bridge::value_column_to_polars(col_name, &new_values);
    let mut new_frame = frame.clone();
    new_frame.with_column(column).map_err(|e| {
        Diagnostic::compute_error("C0210", format!("`mutate()` failed: {e}"))
    })?;

    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
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
    let (frame, na_reasons) = as_dataframe(df, "arrange")?;

    let mut sort_cols: Vec<(Vec<Value>, bool)> = Vec::with_capacity(specs.len());
    for (col_name, desc) in specs {
        let col_vals = polars_bridge::pull_column_as_values(frame, na_reasons, col_name).map_err(|_| {
            Diagnostic::statistical_error(
                "S0201",
                format!("Column `{}` not found in DataFrame for `arrange()`", col_name),
            )
        })?;
        sort_cols.push((col_vals, *desc));
    }

    let num_rows = frame.height();
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

    let (new_frame, new_reasons) = take_rows(frame, na_reasons, &indices)?;
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
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
    let (frame, na_reasons) = as_dataframe(df, "sample_n")?;
    let indices = sample_indices(frame.height(), n);
    let (new_frame, new_reasons) = take_rows(frame, na_reasons, &indices)?;
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

/// `sample_frac(df, frac)` — a random `frac` fraction of rows, without replacement.
pub fn df_sample_frac(df: &Value, frac: f64) -> Result<Value, Diagnostic> {
    let (frame, _) = as_dataframe(df, "sample_frac")?;
    let n = ((frame.height() as f64) * frac).round().max(0.0) as usize;
    df_sample_n(df, n)
}

/// `rename(df, "old_name", "new_name")` — rename a column non-destructively.
pub fn df_rename(df: &Value, old_name: &str, new_name: &str) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "rename")?;

    if frame.column(old_name).is_err() {
        return Err(Diagnostic::statistical_error(
            "S0201",
            format!("Column `{}` not found in DataFrame for `rename()`", old_name),
        ));
    }
    if frame.column(new_name).is_ok() {
        return Err(Diagnostic::compute_error(
            "C0206",
            format!("`rename()`: target column `{}` already exists", new_name),
        ));
    }

    let mut new_frame = frame.clone();
    new_frame.rename(old_name, new_name.into()).map_err(|e| {
        Diagnostic::compute_error("C0210", format!("`rename()` failed: {e}"))
    })?;
    let new_reasons = na_reasons.rename_column(old_name, new_name);
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

/// `drop(df, ["col_a", "col_b"])` — remove columns from the DataFrame.
pub fn df_drop(df: &Value, cols_to_drop: &[String]) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "drop")?;
    let drop_set: std::collections::HashSet<&str> = cols_to_drop.iter().map(|s| s.as_str()).collect();

    let new_frame = frame.drop_many(cols_to_drop.iter().map(|s| s.as_str()));
    let keep_cols: Vec<String> = frame.get_column_names().iter()
        .map(|s| s.to_string())
        .filter(|c| !drop_set.contains(c.as_str()))
        .collect();
    let new_reasons = na_reasons.retain_columns(&keep_cols);
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

/// `distinct(df)` — remove duplicate rows (all columns checked).
/// `distinct(df, ["col"])` — deduplicate by specific key columns.
pub fn df_distinct(df: &Value, key_cols: Option<&[String]>) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "distinct")?;
    let num_rows = frame.height();

    let all_cols: Vec<String> = frame.get_column_names().iter().map(|s| s.to_string()).collect();
    let check_cols: &[String] = key_cols.unwrap_or(&all_cols);

    let check_values: Vec<Vec<Value>> = check_cols.iter()
        .map(|c| polars_bridge::pull_column_as_values(frame, na_reasons, c))
        .collect::<Result<_, _>>()?;

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut keep_indices: Vec<usize> = Vec::new();
    for row_idx in 0..num_rows {
        let row_key: String = check_values.iter()
            .map(|col_vals| format!("{:?}", col_vals[row_idx]))
            .collect::<Vec<_>>()
            .join("|");
        if seen.insert(row_key) {
            keep_indices.push(row_idx);
        }
    }

    let (new_frame, new_reasons) = take_rows(frame, na_reasons, &keep_indices)?;
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

/// `nrow(df)` — number of rows.
pub fn df_nrow(df: &Value) -> Result<Value, Diagnostic> {
    let (frame, _) = as_dataframe(df, "nrow")?;
    Ok(Value::I64(frame.height() as i64))
}

/// `ncol(df)` — number of columns.
pub fn df_ncol(df: &Value) -> Result<Value, Diagnostic> {
    let (frame, _) = as_dataframe(df, "ncol")?;
    Ok(Value::I64(frame.width() as i64))
}

/// `colnames(df)` — returns a `Vector[String]` of column names.
pub fn df_colnames(df: &Value) -> Result<Value, Diagnostic> {
    let (frame, _) = as_dataframe(df, "colnames")?;
    let names = frame.get_column_names().iter().map(|s| Value::String(s.to_string())).collect();
    Ok(Value::Vector(names))
}

/// `slice(df, from, to)` — slice rows by 0-based exclusive range [from, to).
pub fn df_slice(df: &Value, from: usize, to: usize) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "slice")?;
    let end = to.min(frame.height());
    let indices: Vec<usize> = if from < end { (from..end).collect() } else { Vec::new() };
    let (new_frame, new_reasons) = take_rows(frame, na_reasons, &indices)?;
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

// =========================================================================
// 4. Grouping / summarizing + remaining column helpers
// =========================================================================

/// `group_by(df, key1, key2, ...)` — partitions rows by one or more key columns.
/// Returns a `GroupedDataFrame`; only `summarize()`/`ungroup()` consume it, so grouped
/// state never leaks into unrelated verbs. The grouping itself is recomputed fresh in
/// `df_summarize` rather than stored here, since polars' `GroupBy<'a>` borrows its
/// source frame and can't live inside an owned `Value`.
pub fn df_group_by(df: &Value, keys: &[String]) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "group_by")?;

    for k in keys {
        if frame.column(k).is_err() {
            return Err(Diagnostic::statistical_error(
                "S0201",
                format!("Column `{}` not found in DataFrame for `group_by()`", k),
            ));
        }
    }

    Ok(Value::GroupedDataFrame { frame: frame.clone(), na_reasons: na_reasons.clone(), keys: keys.to_vec() })
}

fn group_indicator_to_indices(g: &GroupsIndicator) -> Vec<usize> {
    match g {
        GroupsIndicator::Idx((_, idxs)) => idxs.iter().map(|&i| i as usize).collect(),
        GroupsIndicator::Slice([first, len]) => (*first as usize..(*first + *len) as usize).collect(),
    }
}

/// Computes one `AggSpec` for one group directly against polars — no boxing the
/// group's cells into `Vec<Value>` and looping in Rust first (that was the "optimización
/// pendiente" TODO.md flagged when `summarize()` first shipped). Aggregated results
/// never carry NA-with-reason regardless of path (which reason "wins" across a
/// collapsed group isn't defined), so there's no `NaReasonTable` to thread through here.
fn compute_agg(kind: &str, col: Option<&str>, frame: &DataFrame, indices: &[usize]) -> Result<Value, Diagnostic> {
    if kind == "count" {
        return Ok(Value::I64(indices.len() as i64));
    }

    let col_name = col.ok_or_else(|| {
        Diagnostic::compute_error("C0201", format!("`{}()` requires a column argument in `summarize()`", kind))
    })?;
    let column = frame.column(col_name).map_err(|_| {
        Diagnostic::statistical_error(
            "S0201",
            format!("Column `{}` not found in DataFrame for `summarize()`", col_name),
        )
    })?;

    let idx: Vec<IdxSize> = indices.iter().map(|&i| i as IdxSize).collect();
    let idx_ca = IdxCa::from_vec(PlSmallStr::EMPTY, idx);
    let subset = column.take(&idx_ca).map_err(|e| {
        Diagnostic::compute_error("C0210", format!("`summarize()`: error extracting group subset: {e}"))
    })?;

    // Kleene propagation (RFC 02 sect2.4: GHL never silently skips missing data the way
    // pandas/numpy do) -- polars' native mean_reduce/sum_reduce/etc. skip nulls by
    // default, which is exactly the "silent" behavior the language's own NA design
    // rejects. `null_count()` is O(1) (Arrow tracks it in the array metadata), so this
    // check costs nothing extra before the reduce would otherwise run.
    let propagates_na = matches!(kind, "mean" | "sum" | "std_dev" | "var" | "min" | "max" | "median");
    if propagates_na && subset.null_count() > 0 {
        // No specific reason is attempted here even if the group's NA had one recorded:
        // aggregated columns never carry NA-with-reason regardless (see doc comment on
        // df_summarize) -- which reason would "win" isn't defined, so it isn't guessed at.
        return Ok(Value::NA(None));
    }

    let reduce_err = |e: PolarsError| Diagnostic::compute_error("C0210", format!("`{kind}()` failed in `summarize()`: {e}"));

    match kind {
        "mean" => subset.mean_reduce().map_err(reduce_err).map(|s| polars_bridge::any_value_to_plain_value(s.value())),
        "sum" => subset.sum_reduce().map_err(reduce_err).map(|s| polars_bridge::any_value_to_plain_value(s.value())),
        "std_dev" => subset.std_reduce(1).map_err(reduce_err).map(|s| polars_bridge::any_value_to_plain_value(s.value())),
        "var" => subset.var_reduce(1).map_err(reduce_err).map(|s| polars_bridge::any_value_to_plain_value(s.value())),
        "min" => subset.min_reduce().map_err(reduce_err).map(|s| polars_bridge::any_value_to_plain_value(s.value())),
        "max" => subset.max_reduce().map_err(reduce_err).map(|s| polars_bridge::any_value_to_plain_value(s.value())),
        "median" => subset.median_reduce().map_err(reduce_err).map(|s| polars_bridge::any_value_to_plain_value(s.value())),
        "first" => subset.get(0).map_err(reduce_err).map(|av| polars_bridge::any_value_to_plain_value(&av)),
        "last" => subset.get(subset.len().saturating_sub(1)).map_err(reduce_err).map(|av| polars_bridge::any_value_to_plain_value(&av)),
        "n_distinct" => subset.n_unique().map_err(reduce_err).map(|n| Value::I64(n as i64)),
        other => Err(Diagnostic::compute_error(
            "C0201",
            format!("Unknown aggregation `{}` in `summarize()`", other),
        )),
    }
}

/// `summarize(gdf, name = agg, ...)` — consumes the `GroupedDataFrame`, returns a plain
/// `DataFrame`. `specs` is `(output_name, agg_kind, source_col)`.
pub fn df_summarize(gdf: &Value, specs: &[(String, String, Option<String>)]) -> Result<Value, Diagnostic> {
    let (frame, na_reasons, keys) = match gdf {
        Value::GroupedDataFrame { frame, na_reasons, keys } => (frame, na_reasons, keys),
        other => {
            return Err(Diagnostic::compute_error(
                "C0201",
                format!(
                    "`summarize()` requires a GroupedDataFrame (did you forget `group_by()`?), found `{}`",
                    other.type_name()
                ),
            ));
        }
    };

    // `GroupBy<'a>` borrows `frame`; copy out owned (key values, row indices) per group
    // immediately so nothing here outlives this function's local `grouped`/`positions`.
    let grouped = frame.group_by(keys.iter().map(|s| s.as_str())).map_err(|e| {
        Diagnostic::compute_error("C0210", format!("`summarize()`: group_by failed: {e}"))
    })?;
    let positions = grouped.get_groups();

    let mut groups: Vec<(Vec<Value>, Vec<usize>)> = Vec::with_capacity(positions.len());
    for indicator in positions.iter() {
        let indices = group_indicator_to_indices(&indicator);
        let first_idx = indices[0];
        let key_vals: Vec<Value> = keys.iter()
            .map(|k| polars_bridge::get_cell_as_value(frame, na_reasons, k, first_idx))
            .collect::<Result<_, _>>()?;
        groups.push((key_vals, indices));
    }

    let mut out_columns: Vec<String> = keys.clone();
    let mut out_data: HashMap<String, Vec<Value>> = HashMap::new();
    for k in keys {
        out_data.insert(k.clone(), Vec::with_capacity(groups.len()));
    }
    for (name, _, _) in specs {
        out_columns.push(name.clone());
        out_data.insert(name.clone(), Vec::with_capacity(groups.len()));
    }

    for (key_vals, indices) in &groups {
        for (k, v) in keys.iter().zip(key_vals.iter()) {
            out_data.get_mut(k).unwrap().push(v.clone());
        }
        for (name, kind, col) in specs {
            let result = compute_agg(kind, col.as_deref(), frame, indices)?;
            out_data.get_mut(name).unwrap().push(result);
        }
    }

    // Fresh DataFrame via the same construction boundary as everything else. Aggregated
    // columns carry no NA-with-reason: which reason "wins" across a collapsed group
    // isn't defined (and doesn't need to be, per TODO.md Fase 0).
    let cols: Vec<(String, Vec<Value>)> = out_columns.iter()
        .map(|c| (c.clone(), out_data.remove(c).unwrap_or_default()))
        .collect();
    let (result_frame, result_reasons) = polars_bridge::build_dataframe(&cols)?;
    Ok(Value::DataFrame { frame: result_frame, na_reasons: result_reasons })
}

/// `pull(df, col)` — extract a column as a plain `Vector`.
pub fn df_pull(df: &Value, col: &str) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "pull")?;
    let values = polars_bridge::pull_column_as_values(frame, na_reasons, col)?;
    Ok(Value::Vector(values))
}

/// `na_reasons(df, col)` — a `Vector` the same length as `col`, with the recorded
/// reason (`Value::String`) at each row that has one and `Value::NA(None)` elsewhere.
/// The point of this verb (RFC 02 §2.5): the reason side-channel isn't just internal
/// bookkeeping — it exists so missing-data-analysis code (GHL's own or third-party) can
/// consume it like any other column, with the verbs that already exist.
pub fn df_na_reasons(df: &Value, col: &str) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "na_reasons")?;
    if frame.column(col).is_err() {
        return Err(Diagnostic::statistical_error(
            "S0201",
            format!("Column `{}` not found in DataFrame for `na_reasons()`", col),
        ));
    }
    let values = (0..frame.height())
        .map(|row| match na_reasons.get(col, row) {
            Some(reason) => Value::String(reason.to_string()),
            None => Value::NA(None),
        })
        .collect();
    Ok(Value::Vector(values))
}

/// `fill_na(df, col, default)` — replace `NA` in one column with a constant.
pub fn df_fill_na(df: &Value, col: &str, default: &Value) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "fill_na")?;
    let col_vals = polars_bridge::pull_column_as_values(frame, na_reasons, col).map_err(|_| {
        Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame for `fill_na()`", col))
    })?;
    let filled: Vec<Value> = col_vals.iter().map(|v| if v.is_na() { default.clone() } else { v.clone() }).collect();

    let column = polars_bridge::value_column_to_polars(col, &filled);
    let mut new_frame = frame.clone();
    new_frame.with_column(column).map_err(|e| {
        Diagnostic::compute_error("C0210", format!("`fill_na()` failed: {e}"))
    })?;
    let new_reasons = na_reasons.without_column(col);
    Ok(Value::DataFrame { frame: new_frame, na_reasons: new_reasons })
}

/// `fill_na_all(df, default)` — replace `NA` in every column with a constant.
pub fn df_fill_na_all(df: &Value, default: &Value) -> Result<Value, Diagnostic> {
    let (frame, _) = as_dataframe(df, "fill_na_all")?;
    let mut result = df.clone();
    for col in frame.get_column_names() {
        result = df_fill_na(&result, col, default)?;
    }
    Ok(result)
}

/// `count(df, col)` — frequency table: `DataFrame[col, n]`, one row per distinct value.
pub fn df_count(df: &Value, col: &str) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "count")?;
    let col_vals = polars_bridge::pull_column_as_values(frame, na_reasons, col).map_err(|_| {
        Diagnostic::statistical_error("S0201", format!("Column `{}` not found in DataFrame for `count()`", col))
    })?;

    let mut order: Vec<Value> = Vec::new();
    let mut counts: HashMap<String, i64> = HashMap::new();
    for v in &col_vals {
        let key = format!("{:?}", v);
        if !counts.contains_key(&key) {
            order.push(v.clone());
        }
        *counts.entry(key).or_insert(0) += 1;
    }

    let n_out: Vec<Value> = order.iter()
        .map(|v| Value::I64(*counts.get(&format!("{:?}", v)).unwrap_or(&0)))
        .collect();

    let (result_frame, result_reasons) = polars_bridge::build_dataframe(&[
        (col.to_string(), order),
        ("n".to_string(), n_out),
    ])?;
    Ok(Value::DataFrame { frame: result_frame, na_reasons: result_reasons })
}

/// `glimpse(df)` — compact per-column overview (name, type, first few values); prints, returns `Unit`.
pub fn df_glimpse(df: &Value) -> Result<Value, Diagnostic> {
    let (frame, na_reasons) = as_dataframe(df, "glimpse")?;
    println!("Rows: {}", frame.height());
    println!("Columns: {}", frame.width());
    for col in frame.get_column_names() {
        let col_vals = polars_bridge::pull_column_as_values(frame, na_reasons, col)?;
        let ty = col_vals.iter().find(|v| !v.is_na()).map(|v| v.type_name()).unwrap_or("any");
        let preview: Vec<String> = col_vals.iter().take(10).map(|v| v.to_string()).collect();
        println!("$ {:<15} <{}> {}", col, ty, preview.join(", "));
    }
    Ok(Value::Unit)
}

/// `inner_join(left, right, on)` — hash join, keeping only matching rows on both sides.
pub fn df_inner_join(left: &Value, right: &Value, on: &[String]) -> Result<Value, Diagnostic> {
    df_join(left, right, on, JoinType::Inner)
}

/// `left_join(left, right, on)` — hash join, keeping every row of `left` (unmatched
/// `right` columns become NA).
pub fn df_left_join(left: &Value, right: &Value, on: &[String]) -> Result<Value, Diagnostic> {
    df_join(left, right, on, JoinType::Left)
}

fn df_join(left: &Value, right: &Value, on: &[String], how: JoinType) -> Result<Value, Diagnostic> {
    let verb = match how {
        JoinType::Inner => "inner_join",
        JoinType::Left => "left_join",
        _ => "join",
    };
    let (left_frame, _) = as_dataframe(left, verb)?;
    let (right_frame, _) = as_dataframe(right, verb)?;

    for k in on {
        if left_frame.column(k).is_err() {
            return Err(Diagnostic::statistical_error(
                "S0201",
                format!("Column `{}` not found in the left DataFrame for `{verb}()`", k),
            ));
        }
        if right_frame.column(k).is_err() {
            return Err(Diagnostic::statistical_error(
                "S0201",
                format!("Column `{}` not found in the right DataFrame for `{verb}()`", k),
            ));
        }
    }

    let joined = left_frame
        .join(right_frame, on, on, JoinArgs::new(how), None)
        .map_err(|e| Diagnostic::compute_error("C0210", format!("`{verb}()` failed: {e}")))?;

    // NA-with-reason is intentionally dropped here rather than guessed at: a join can
    // duplicate a left row (one-to-many match) or drop it (inner join, no match), so
    // the simple positional reindex `filter`/`arrange`/`slice` use doesn't apply — the
    // safe eager join API doesn't expose which output row(s) a given input row landed
    // on, so there's no correct mapping to reconstruct without one. Revisit if/when
    // Fase 1's lazy engine exposes that mapping.
    Ok(Value::DataFrame { frame: joined, na_reasons: NaReasonTable::new() })
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
        if let Value::DataFrame { frame, na_reasons } = &df_val {
            let names: Vec<String> = frame.get_column_names().iter().map(|s| s.to_string()).collect();
            assert_eq!(names, vec!["id", "name", "score", "active", "sensor_status"]);

            // id is i64
            let ids = polars_bridge::pull_column_as_values(frame, na_reasons, "id").unwrap();
            assert_eq!(ids[0], Value::I64(1));
            assert_eq!(ids[1], Value::I64(2));

            // score is f64 with reasoned NA
            let scores = polars_bridge::pull_column_as_values(frame, na_reasons, "score").unwrap();
            assert_eq!(scores[0], Value::F64(98.5));
            assert_eq!(scores[1], Value::NA(None));

            // sensor_status with reasoned NA
            let sensors = polars_bridge::pull_column_as_values(frame, na_reasons, "sensor_status").unwrap();
            assert_eq!(sensors[0], Value::String("OK".into()));
            assert_eq!(sensors[1], Value::NA(Some("SensorDropout".into())));
            assert_eq!(sensors[3], Value::NA(Some("LowBattery".into())));
        } else {
            panic!("Expected DataFrame");
        }
    }

    #[test]
    fn test_read_csv_file_matches_parse_csv_string() {
        // read_csv_file uses polars-io for the fast/multi-threaded tokenizing step
        // (forcing every column to String via dtype_overwrite) and then runs the exact
        // same infer_and_convert_column/NA:reason logic as parse_csv_string on top --
        // this locks in that the two paths produce identical results.
        let csv_data = "id,name,score,active,sensor_status\n\
                         1,Alpha,98.5,true,OK\n\
                         2,Beta,NA,false,NA:SensorDropout\n\
                         3,Gamma,87.2,true,OK\n\
                         4,Delta,76.0,false,NA:LowBattery\n";

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("ghl_test_read_csv_file.csv");
        let p_str = path.to_str().unwrap();
        write_file(p_str, csv_data).unwrap();

        let from_string = parse_csv_string(csv_data, None).expect("parse_csv_string should succeed");
        let from_file = read_csv_file(p_str, None).expect("read_csv_file should succeed");
        let _ = fs::remove_file(&path);

        let (frame_a, reasons_a) = match &from_string {
            Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
            _ => panic!("Expected DataFrame from parse_csv_string"),
        };
        let (frame_b, reasons_b) = match &from_file {
            Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
            _ => panic!("Expected DataFrame from read_csv_file"),
        };

        assert_eq!(
            frame_a.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            frame_b.get_column_names().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        for col in frame_a.get_column_names() {
            let vals_a = polars_bridge::pull_column_as_values(frame_a, reasons_a, col).unwrap();
            let vals_b = polars_bridge::pull_column_as_values(frame_b, reasons_b, col).unwrap();
            assert_eq!(vals_a, vals_b, "column `{col}` differs between the two CSV paths");
        }
    }

    #[test]
    fn test_df_select_head_tail() {
        let csv_data = "x,y,z\n1,10,100\n2,20,200\n3,30,300\n4,40,400\n5,50,500\n";
        let df = parse_csv_string(csv_data, None).unwrap();

        let selected = df_select(&df, &["z".into(), "x".into()]).unwrap();
        if let Value::DataFrame { frame, .. } = &selected {
            let names: Vec<String> = frame.get_column_names().iter().map(|s| s.to_string()).collect();
            assert_eq!(names, vec!["z", "x"]);
            assert_eq!(frame.height(), 5);
        }

        let h = df_head(&df, 2).unwrap();
        if let Value::DataFrame { frame, na_reasons } = &h {
            let xs = polars_bridge::pull_column_as_values(frame, na_reasons, "x").unwrap();
            assert_eq!(xs.len(), 2);
            assert_eq!(xs[1], Value::I64(2));
        }

        let t = df_tail(&df, 2).unwrap();
        if let Value::DataFrame { frame, na_reasons } = &t {
            let xs = polars_bridge::pull_column_as_values(frame, na_reasons, "x").unwrap();
            assert_eq!(xs.len(), 2);
            assert_eq!(xs[0], Value::I64(4));
            assert_eq!(xs[1], Value::I64(5));
        }
    }
}
