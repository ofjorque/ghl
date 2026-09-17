//! Scalar `String` + `Vector[String]` native functions, both vectorized.
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use ghl_diagnostics::Diagnostic;
use crate::value::Value;
use crate::vector_data::VectorData;


// =========================================================================
// String helpers — scalar `String` + `Vector[String]`, both vectorized
// =========================================================================

pub(crate) fn map_string_fn(v: &Value, f: impl Fn(&str) -> Value + Clone) -> Value {
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

pub(crate) fn native_str_upper(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_upper()` requires 1 argument"))?;
    Ok(map_string_fn(v, |s| Value::String(s.to_uppercase())))
}

pub(crate) fn native_str_lower(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_lower()` requires 1 argument"))?;
    Ok(map_string_fn(v, |s| Value::String(s.to_lowercase())))
}

pub(crate) fn native_str_trim(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_trim()` requires 1 argument"))?;
    Ok(map_string_fn(v, |s| Value::String(s.trim().to_string())))
}

pub(crate) fn native_str_len(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_len()` requires 1 argument"))?;
    Ok(map_string_fn(v, |s| Value::I64(s.chars().count() as i64)))
}

pub(crate) fn native_str_contains(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_contains()` requires 2 arguments"))?;
    let pat = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_contains()` second argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| Value::Bool(s.contains(&pat))))
}

pub(crate) fn native_str_starts(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_starts()` requires 2 arguments"))?;
    let pat = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_starts()` second argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| Value::Bool(s.starts_with(&pat))))
}

pub(crate) fn native_str_ends(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_ends()` requires 2 arguments"))?;
    let pat = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_ends()` second argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| Value::Bool(s.ends_with(&pat))))
}

pub(crate) fn native_str_replace(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_replace()` requires 3 arguments"))?;
    let from = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_replace()` second argument must be a string")
    })?.to_string();
    let to = args.get(2).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_replace()` third argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| Value::String(s.replace(&from, &to))))
}

pub(crate) fn native_str_split(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`str_split()` requires 2 arguments"))?;
    let sep = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`str_split()` second argument must be a string")
    })?.to_string();
    Ok(map_string_fn(v, move |s| {
        Value::Vector(VectorData::from_values(s.split(sep.as_str()).map(|p| Value::String(p.to_string())).collect()))
    }))
}

pub(crate) fn native_str_pad(args: Vec<Value>) -> Result<Value, Diagnostic> {
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


pub(crate) fn native_filter(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_zeros(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_len(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_get(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_set(mut args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.len() {
        3 => {
            // set(vector, index, val)
            let val = args.pop().unwrap();
            let idx_val = args.pop().unwrap();
            let idx = match &idx_val {
                Value::I64(i) => *i,
                Value::F64(f) if f.fract() == 0.0 => *f as i64,
                _ => {
                    return Err(Diagnostic::compute_error("C0201", "`set(vector, index, val)` requires an integer index"));
                }
            };
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

pub(crate) fn native_get_row(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_set_row(mut args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_get_col(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_transpose(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_identity(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_diag(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

pub(crate) fn native_log_sum_exp(args: Vec<Value>) -> Result<Value, Diagnostic> {
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
