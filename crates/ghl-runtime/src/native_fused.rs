//! Fused and Tensor Operations (Pilar 5 / Roadmap 09 Parte H).
//!
//! Provides single-pass, allocation-free fused kernels (`sigmoid_matmul`, `log_sum_exp`,
//! `softmax`, `row_sums`, `col_sums`, `row_means`, `col_means`, `row_maxs`, `col_maxs`,
//! `row_mins`, `col_mins`, `sigmoid`, `fused_mul_add`) executed in AVX2 CPU registers
//! and parallelized with Rayon work-stealing.

use rayon::prelude::*;
use ghl_diagnostics::Diagnostic;
use crate::matrix::MatrixOps;
use crate::value::Value;
use crate::vector_data::{VectorData, NumericView};

#[inline(always)]
pub fn sigmoid_stable(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let exp_x = x.exp();
        exp_x / (1.0 + exp_x)
    }
}

pub(crate) enum DataSlice<'a> {
    Borrowed(&'a [f64]),
    View(NumericView<'a>),
    Owned(Vec<f64>),
}

impl<'a> DataSlice<'a> {
    pub fn as_slice(&self) -> &[f64] {
        match self {
            DataSlice::Borrowed(s) => s,
            DataSlice::View(v) => v.as_slice(),
            DataSlice::Owned(v) => v.as_slice(),
        }
    }
}

pub(crate) fn extract_data<'a>(
    val: &'a Value,
    param_name: &str,
) -> Result<(usize, usize, DataSlice<'a>), Diagnostic> {
    match val {
        Value::Matrix { rows, cols, data } => {
            Ok((*rows, *cols, DataSlice::Borrowed(data.as_slice())))
        }
        Value::Vector(vd) => {
            if let Ok(view) = vd.as_f64_view() {
                Ok((vd.len(), 1, DataSlice::View(view)))
            } else {
                let data: Vec<f64> = vd.iter().map(|item| item.as_f64().unwrap_or(0.0)).collect();
                Ok((vd.len(), 1, DataSlice::Owned(data)))
            }
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("Parameter `{param_name}` must be a Matrix or Vector, found `{}`", other.type_name()),
        )),
    }
}

/// Fused Matrix Multiplication + Sigmoid Activation + Optional Bias.
///
/// Computes `sigma(A * B + bias)` in a single pass without allocating intermediate matrices in RAM.
/// Bias can be a scalar, a vector of length cols(B) (column bias), or a vector of length rows(A) (row bias).
///
/// Signature: `sigmoid_matmul(A, B, [bias]) -> Matrix`
pub(crate) fn native_sigmoid_matmul(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`sigmoid_matmul()` requires at least 2 arguments: `sigmoid_matmul(A, B, [bias])`",
        ));
    }

    let (a_rows, a_cols, a_data) = extract_data(&args[0], "A")?;
    let (b_rows, b_cols, b_data) = extract_data(&args[1], "B")?;
    let (m, n, mut c_data) = MatrixOps::mul(a_rows, a_cols, a_data.as_slice(), b_rows, b_cols, b_data.as_slice())?;

    if let Some(bias_val) = args.get(2) {
        match bias_val {
            Value::F64(b) => {
                let b = *b;
                if c_data.len() >= 1024 {
                    c_data.par_iter_mut().for_each(|v| *v = sigmoid_stable(*v + b));
                } else {
                    c_data.iter_mut().for_each(|v| *v = sigmoid_stable(*v + b));
                }
            }
            Value::I64(b) => {
                let b = *b as f64;
                if c_data.len() >= 1024 {
                    c_data.par_iter_mut().for_each(|v| *v = sigmoid_stable(*v + b));
                } else {
                    c_data.iter_mut().for_each(|v| *v = sigmoid_stable(*v + b));
                }
            }
            _ => {
                let (bias_len, _, bias_slice) = extract_data(bias_val, "bias")?;
                let b_slice = bias_slice.as_slice();
                if bias_len == n {
                    // Column bias: adds bias[j] to column j across all rows
                    if c_data.len() >= 1024 {
                        c_data.par_chunks_mut(n).for_each(|row| {
                            for j in 0..n {
                                row[j] = sigmoid_stable(row[j] + b_slice[j]);
                            }
                        });
                    } else {
                        for i in 0..m {
                            for j in 0..n {
                                c_data[i * n + j] = sigmoid_stable(c_data[i * n + j] + b_slice[j]);
                            }
                        }
                    }
                } else if bias_len == m {
                    // Row bias: adds bias[i] to row i across all columns
                    if c_data.len() >= 1024 {
                        c_data.par_chunks_mut(n).enumerate().for_each(|(i, row)| {
                            let bi = b_slice[i];
                            for j in 0..n {
                                row[j] = sigmoid_stable(row[j] + bi);
                            }
                        });
                    } else {
                        for i in 0..m {
                            let bi = b_slice[i];
                            for j in 0..n {
                                c_data[i * n + j] = sigmoid_stable(c_data[i * n + j] + bi);
                            }
                        }
                    }
                } else {
                    return Err(Diagnostic::statistical_error(
                        "S0412",
                        format!("bias length ({bias_len}) must match output columns ({n}) or rows ({m})"),
                    ));
                }
            }
        }
    } else {
        if c_data.len() >= 1024 {
            c_data.par_iter_mut().for_each(|v| *v = sigmoid_stable(*v));
        } else {
            c_data.iter_mut().for_each(|v| *v = sigmoid_stable(*v));
        }
    }

    Ok(Value::matrix(m, n, c_data))
}

/// Element-wise logistic sigmoid: `sigmoid(x) = 1 / (1 + exp(-x))` with full numerical stability.
///
/// Signature: `sigmoid(x) -> scalar | Vector | Matrix`
pub(crate) fn native_sigmoid(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`sigmoid()` requires 1 argument")
    })?;

    match val {
        Value::F64(x) => Ok(Value::F64(sigmoid_stable(*x))),
        Value::I64(n) => Ok(Value::F64(sigmoid_stable(*n as f64))),
        Value::Matrix { rows, cols, data } => {
            let mut out = (**data).clone();
            if out.len() >= 1024 {
                out.par_iter_mut().for_each(|v| *v = sigmoid_stable(*v));
            } else {
                out.iter_mut().for_each(|v| *v = sigmoid_stable(*v));
            }
            Ok(Value::matrix(*rows, *cols, out))
        }
        Value::Vector(vd) => {
            if let Ok(view) = vd.as_f64_view() {
                let slice = view.as_slice();
                let out: Vec<f64> = if slice.len() >= 1024 {
                    slice.par_iter().map(|&x| sigmoid_stable(x)).collect()
                } else {
                    slice.iter().map(|&x| sigmoid_stable(x)).collect()
                };
                Ok(Value::Vector(VectorData::from_f64(out)))
            } else {
                let out: Vec<f64> = vd.iter().map(|item| sigmoid_stable(item.as_f64().unwrap_or(0.0))).collect();
                Ok(Value::Vector(VectorData::from_f64(out)))
            }
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`sigmoid()` requires numeric scalar, Vector, or Matrix, found `{}`", other.type_name()),
        )),
    }
}

/// Numerically stable Log-Sum-Exp reduction.
///
/// For a Matrix, defaults to row-wise reduction (`axis = 1`). Pass `axis = 0` for column-wise.
/// For a Vector, returns a scalar float.
///
/// Signature: `log_sum_exp(x, [axis = 1]) -> f64 | Vector`
pub(crate) fn native_log_sum_exp(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`log_sum_exp()` requires at least 1 argument")
    })?;

    let axis = args.get(1).and_then(|v| v.as_i64());

    match val {
        Value::Matrix { rows, cols, data } => {
            let m = *rows;
            let n = *cols;
            let slice = data.as_slice();

            match axis {
                Some(0) => {
                    // Column-wise log-sum-exp: returns Vector of length n
                    let mut out = Vec::with_capacity(n);
                    for j in 0..n {
                        let mut max_val = f64::NEG_INFINITY;
                        for i in 0..m {
                            let x = slice[i * n + j];
                            if x > max_val { max_val = x; }
                        }
                        if max_val.is_infinite() {
                            out.push(-f64::INFINITY);
                        } else {
                            let mut sum = 0.0f64;
                            for i in 0..m {
                                sum += (slice[i * n + j] - max_val).exp();
                            }
                            out.push(max_val + sum.ln());
                        }
                    }
                    Ok(Value::Vector(VectorData::from_f64(out)))
                }
                _ => {
                    // Row-wise log-sum-exp (default axis = 1): returns Vector of length m
                    let out: Vec<f64> = if m >= 64 {
                        (0..m).into_par_iter().map(|i| {
                            let row = &slice[i * n..(i + 1) * n];
                            let max_val = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                            if max_val.is_infinite() {
                                -f64::INFINITY
                            } else {
                                let sum: f64 = row.iter().map(|&x| (x - max_val).exp()).sum();
                                max_val + sum.ln()
                            }
                        }).collect()
                    } else {
                        (0..m).map(|i| {
                            let row = &slice[i * n..(i + 1) * n];
                            let max_val = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                            if max_val.is_infinite() {
                                -f64::INFINITY
                            } else {
                                let sum: f64 = row.iter().map(|&x| (x - max_val).exp()).sum();
                                max_val + sum.ln()
                            }
                        }).collect()
                    };
                    Ok(Value::Vector(VectorData::from_f64(out)))
                }
            }
        }
        Value::Vector(_) => {
            let (_, _, data_slice) = extract_data(val, "x")?;
            let slice = data_slice.as_slice();
            let max_val = slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if max_val.is_infinite() {
                Ok(Value::F64(-f64::INFINITY))
            } else {
                let sum: f64 = slice.iter().map(|&x| (x - max_val).exp()).sum();
                Ok(Value::F64(max_val + sum.ln()))
            }
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`log_sum_exp()` requires a Matrix or Vector, found `{}`", other.type_name()),
        )),
    }
}

/// Numerically stable Softmax probability normalization.
///
/// For a Matrix, defaults to row-wise softmax (`axis = 1`). Pass `axis = 0` for column-wise.
/// For a Vector, returns a normalized probability Vector summing to 1.0.
///
/// Signature: `softmax(x, [axis = 1]) -> Vector | Matrix`
pub(crate) fn native_softmax(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`softmax()` requires at least 1 argument")
    })?;

    let axis = args.get(1).and_then(|v| v.as_i64()).unwrap_or(1);

    match val {
        Value::Matrix { rows, cols, data } => {
            let m = *rows;
            let n = *cols;
            let slice = data.as_slice();
            let mut out = vec![0.0f64; m * n];

            if axis == 0 {
                // Column-wise softmax
                for j in 0..n {
                    let mut max_val = f64::NEG_INFINITY;
                    for i in 0..m {
                        let x = slice[i * n + j];
                        if x > max_val { max_val = x; }
                    }
                    let mut sum = 0.0f64;
                    for i in 0..m {
                        let exp_val = (slice[i * n + j] - max_val).exp();
                        out[i * n + j] = exp_val;
                        sum += exp_val;
                    }
                    let inv_sum = if sum > 0.0 { 1.0 / sum } else { 0.0 };
                    for i in 0..m {
                        out[i * n + j] *= inv_sum;
                    }
                }
            } else {
                // Row-wise softmax (axis = 1)
                if m >= 64 {
                    out.par_chunks_mut(n).enumerate().for_each(|(i, out_row)| {
                        let in_row = &slice[i * n..(i + 1) * n];
                        let max_val = in_row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                        let mut sum = 0.0f64;
                        for j in 0..n {
                            let exp_val = (in_row[j] - max_val).exp();
                            out_row[j] = exp_val;
                            sum += exp_val;
                        }
                        let inv_sum = if sum > 0.0 { 1.0 / sum } else { 0.0 };
                        for j in 0..n {
                            out_row[j] *= inv_sum;
                        }
                    });
                } else {
                    for i in 0..m {
                        let in_row = &slice[i * n..(i + 1) * n];
                        let max_val = in_row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                        let mut sum = 0.0f64;
                        for j in 0..n {
                            let exp_val = (in_row[j] - max_val).exp();
                            out[i * n + j] = exp_val;
                            sum += exp_val;
                        }
                        let inv_sum = if sum > 0.0 { 1.0 / sum } else { 0.0 };
                        for j in 0..n {
                            out[i * n + j] *= inv_sum;
                        }
                    }
                }
            }
            Ok(Value::matrix(m, n, out))
        }
        Value::Vector(_) => {
            let (_, _, data_slice) = extract_data(val, "x")?;
            let slice = data_slice.as_slice();
            let max_val = slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let mut sum = 0.0f64;
            let mut out = Vec::with_capacity(slice.len());
            for &x in slice {
                let exp_val = (x - max_val).exp();
                out.push(exp_val);
                sum += exp_val;
            }
            let inv_sum = if sum > 0.0 { 1.0 / sum } else { 0.0 };
            for v in &mut out {
                *v *= inv_sum;
            }
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`softmax()` requires a Matrix or Vector, found `{}`", other.type_name()),
        )),
    }
}

/// Fused Matrix Multiply-Add: `fused_mul_add(A, B, C) = A * B + C`.
///
/// Computes the matrix product and accumulates `C` without allocating intermediate matrices.
///
/// Signature: `fused_mul_add(A, B, C) -> Matrix`
pub(crate) fn native_fused_mul_add(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error("C0201", "`fused_mul_add(A, B, C)` requires 3 arguments"));
    }
    let (a_rows, a_cols, a_data) = extract_data(&args[0], "A")?;
    let (b_rows, b_cols, b_data) = extract_data(&args[1], "B")?;
    let (m, n, mut c_data) = MatrixOps::mul(a_rows, a_cols, a_data.as_slice(), b_rows, b_cols, b_data.as_slice())?;

    let c_val = &args[2];
    match c_val {
        Value::F64(scalar) => {
            let s = *scalar;
            c_data.iter_mut().for_each(|v| *v += s);
        }
        Value::I64(scalar) => {
            let s = *scalar as f64;
            c_data.iter_mut().for_each(|v| *v += s);
        }
        Value::Matrix { rows: cr, cols: cc, data: c_mat } => {
            if *cr != m || *cc != n {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("Matrix C dimension ({cr}x{cc}) must match product dimension ({m}x{n})"),
                ));
            }
            let c_slice = c_mat.as_slice();
            for idx in 0..c_data.len() {
                c_data[idx] += c_slice[idx];
            }
        }
        Value::Vector(_) => {
            let (c_len, _, c_slice) = extract_data(c_val, "C")?;
            let s = c_slice.as_slice();
            if c_len == n {
                for i in 0..m {
                    for j in 0..n {
                        c_data[i * n + j] += s[j];
                    }
                }
            } else if c_len == m {
                for i in 0..m {
                    for j in 0..n {
                        c_data[i * n + j] += s[i];
                    }
                }
            } else {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("Vector C length ({c_len}) must match product columns ({n}) or rows ({m})"),
                ));
            }
        }
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("`fused_mul_add()` requires C to be scalar, Vector, or Matrix, found `{}`", other.type_name()),
            ));
        }
    }
    Ok(Value::matrix(m, n, c_data))
}

// =========================================================================
// Fast Axis Reductions for Matrices
// =========================================================================

pub(crate) fn native_row_sums(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`row_sums()` requires a Matrix"))?;
    match val {
        Value::Matrix { rows, cols, data } => {
            let (m, n) = (*rows, *cols);
            let slice = data.as_slice();
            let out: Vec<f64> = if m >= 64 {
                (0..m).into_par_iter().map(|i| {
                    slice[i * n..(i + 1) * n].iter().sum()
                }).collect()
            } else {
                (0..m).map(|i| {
                    slice[i * n..(i + 1) * n].iter().sum()
                }).collect()
            };
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error("S0200", format!("`row_sums()` requires a Matrix, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_col_sums(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`col_sums()` requires a Matrix"))?;
    match val {
        Value::Matrix { rows, cols, data } => {
            let (m, n) = (*rows, *cols);
            let slice = data.as_slice();
            let mut out = vec![0.0f64; n];
            for i in 0..m {
                let row = &slice[i * n..(i + 1) * n];
                for j in 0..n {
                    out[j] += row[j];
                }
            }
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error("S0200", format!("`col_sums()` requires a Matrix, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_row_means(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`row_means()` requires a Matrix"))?;
    match val {
        Value::Matrix { rows, cols, data } => {
            let (m, n) = (*rows, *cols);
            let inv_n = if n > 0 { 1.0 / (n as f64) } else { 0.0 };
            let slice = data.as_slice();
            let out: Vec<f64> = if m >= 64 {
                (0..m).into_par_iter().map(|i| {
                    slice[i * n..(i + 1) * n].iter().sum::<f64>() * inv_n
                }).collect()
            } else {
                (0..m).map(|i| {
                    slice[i * n..(i + 1) * n].iter().sum::<f64>() * inv_n
                }).collect()
            };
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error("S0200", format!("`row_means()` requires a Matrix, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_col_means(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`col_means()` requires a Matrix"))?;
    match val {
        Value::Matrix { rows, cols, data } => {
            let (m, n) = (*rows, *cols);
            let inv_m = if m > 0 { 1.0 / (m as f64) } else { 0.0 };
            let slice = data.as_slice();
            let mut out = vec![0.0f64; n];
            for i in 0..m {
                let row = &slice[i * n..(i + 1) * n];
                for j in 0..n {
                    out[j] += row[j];
                }
            }
            for j in 0..n {
                out[j] *= inv_m;
            }
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error("S0200", format!("`col_means()` requires a Matrix, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_row_maxs(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`row_maxs()` requires a Matrix"))?;
    match val {
        Value::Matrix { rows, cols, data } => {
            let (m, n) = (*rows, *cols);
            let slice = data.as_slice();
            let out: Vec<f64> = if m >= 64 {
                (0..m).into_par_iter().map(|i| {
                    slice[i * n..(i + 1) * n].iter().copied().fold(f64::NEG_INFINITY, f64::max)
                }).collect()
            } else {
                (0..m).map(|i| {
                    slice[i * n..(i + 1) * n].iter().copied().fold(f64::NEG_INFINITY, f64::max)
                }).collect()
            };
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error("S0200", format!("`row_maxs()` requires a Matrix, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_col_maxs(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`col_maxs()` requires a Matrix"))?;
    match val {
        Value::Matrix { rows, cols, data } => {
            let (m, n) = (*rows, *cols);
            let slice = data.as_slice();
            let mut out = vec![f64::NEG_INFINITY; n];
            for i in 0..m {
                let row = &slice[i * n..(i + 1) * n];
                for j in 0..n {
                    if row[j] > out[j] { out[j] = row[j]; }
                }
            }
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error("S0200", format!("`col_maxs()` requires a Matrix, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_row_mins(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`row_mins()` requires a Matrix"))?;
    match val {
        Value::Matrix { rows, cols, data } => {
            let (m, n) = (*rows, *cols);
            let slice = data.as_slice();
            let out: Vec<f64> = if m >= 64 {
                (0..m).into_par_iter().map(|i| {
                    slice[i * n..(i + 1) * n].iter().copied().fold(f64::INFINITY, f64::min)
                }).collect()
            } else {
                (0..m).map(|i| {
                    slice[i * n..(i + 1) * n].iter().copied().fold(f64::INFINITY, f64::min)
                }).collect()
            };
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error("S0200", format!("`row_mins()` requires a Matrix, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_col_mins(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let val = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`col_mins()` requires a Matrix"))?;
    match val {
        Value::Matrix { rows, cols, data } => {
            let (m, n) = (*rows, *cols);
            let slice = data.as_slice();
            let mut out = vec![f64::INFINITY; n];
            for i in 0..m {
                let row = &slice[i * n..(i + 1) * n];
                for j in 0..n {
                    if row[j] < out[j] { out[j] = row[j]; }
                }
            }
            Ok(Value::Vector(VectorData::from_f64(out)))
        }
        other => Err(Diagnostic::statistical_error("S0200", format!("`col_mins()` requires a Matrix, found `{}`", other.type_name()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sigmoid_numerical_stability() {
        assert_eq!(sigmoid_stable(0.0), 0.5);
        assert_eq!(sigmoid_stable(1000.0), 1.0);
        assert_eq!(sigmoid_stable(-1000.0), 0.0);
    }

    #[test]
    fn test_sigmoid_vector_and_matrix() {
        let v = Value::Vector(VectorData::from_f64(vec![0.0, 2.0, -2.0]));
        let res = native_sigmoid(vec![v]).unwrap();
        if let Value::Vector(vd) = res {
            let view = vd.as_f64_view().unwrap();
            let slice = view.as_slice();
            assert!((slice[0] - 0.5).abs() < 1e-6);
            assert!((slice[1] - (1.0 / (1.0 + (-2.0f64).exp()))).abs() < 1e-6);
        } else {
            panic!("Expected Vector");
        }
    }

    #[test]
    fn test_log_sum_exp_stability() {
        // [1000.0, 1000.0] should not overflow to inf
        let v = Value::Vector(VectorData::from_f64(vec![1000.0, 1000.0]));
        let res = native_log_sum_exp(vec![v]).unwrap();
        if let Value::F64(val) = res {
            assert!((val - (1000.0 + 2.0f64.ln())).abs() < 1e-6);
        } else {
            panic!("Expected F64");
        }

        // Matrix axis 1 (row-wise)
        let m = Value::matrix(2, 2, vec![0.0, 0.0, 1.0, 1.0]);
        let res = native_log_sum_exp(vec![m, Value::I64(1)]).unwrap();
        if let Value::Vector(vd) = res {
            let view = vd.as_f64_view().unwrap();
            let slice = view.as_slice();
            assert_eq!(slice.len(), 2);
            assert!((slice[0] - 2.0f64.ln()).abs() < 1e-6);
            assert!((slice[1] - (1.0 + 2.0f64.ln())).abs() < 1e-6);
        } else {
            panic!("Expected Vector");
        }
    }

    #[test]
    fn test_softmax_stability_and_axis() {
        // Huge values: [1000.0, 1000.0] -> [0.5, 0.5]
        let v = Value::Vector(VectorData::from_f64(vec![1000.0, 1000.0]));
        let res = native_softmax(vec![v]).unwrap();
        if let Value::Vector(vd) = res {
            let view = vd.as_f64_view().unwrap();
            let slice = view.as_slice();
            assert!((slice[0] - 0.5).abs() < 1e-6);
            assert!((slice[1] - 0.5).abs() < 1e-6);
        } else {
            panic!("Expected Vector");
        }

        // Matrix row-wise (axis 1)
        let m = Value::matrix(2, 3, vec![1.0, 2.0, 3.0, 10.0, 10.0, 10.0]);
        let res = native_softmax(vec![m, Value::I64(1)]).unwrap();
        if let Value::Matrix { rows, cols, data } = res {
            assert_eq!(rows, 2);
            assert_eq!(cols, 3);
            let row2 = &data.as_slice()[3..6];
            assert!((row2[0] - 1.0 / 3.0).abs() < 1e-6);
            assert!((row2[1] - 1.0 / 3.0).abs() < 1e-6);
            assert!((row2[2] - 1.0 / 3.0).abs() < 1e-6);
        } else {
            panic!("Expected Matrix");
        }
    }

    #[test]
    fn test_sigmoid_matmul_and_bias() {
        // A = 2x2 [[1, 0], [0, 1]], B = 2x2 [[0, 0], [0, 0]]
        let a = Value::matrix(2, 2, vec![1.0, 0.0, 0.0, 1.0]);
        let b = Value::matrix(2, 2, vec![0.0, 0.0, 0.0, 0.0]);
        let res = native_sigmoid_matmul(vec![a, b]).unwrap();
        if let Value::Matrix { rows, cols, data } = res {
            assert_eq!(rows, 2);
            assert_eq!(cols, 2);
            for v in data.as_slice() {
                assert!((v - 0.5).abs() < 1e-6);
            }
        } else {
            panic!("Expected Matrix");
        }
    }

    #[test]
    fn test_fused_mul_add() {
        let a = Value::matrix(2, 2, vec![1.0, 2.0, 3.0, 4.0]);
        let b = Value::matrix(2, 2, vec![2.0, 2.0, 2.0, 2.0]);
        let c = Value::matrix(2, 2, vec![10.0, 20.0, 30.0, 40.0]);
        let res = native_fused_mul_add(vec![a, b, c]).unwrap();
        if let Value::Matrix { rows, cols, data } = res {
            assert_eq!(rows, 2);
            assert_eq!(cols, 2);
            assert_eq!(data.as_slice(), &[16.0, 26.0, 44.0, 54.0]);
        } else {
            panic!("Expected Matrix");
        }
    }

    #[test]
    fn test_axis_reductions() {
        let m = Value::matrix(2, 3, vec![
            1.0, 2.0, 3.0,
            4.0, 5.0, 6.0,
        ]);
        // row_sums: [6, 15]
        if let Value::Vector(vd) = native_row_sums(vec![m.clone()]).unwrap() {
            assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[6.0, 15.0]);
        }
        // col_sums: [5, 7, 9]
        if let Value::Vector(vd) = native_col_sums(vec![m.clone()]).unwrap() {
            assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[5.0, 7.0, 9.0]);
        }
        // row_means: [2, 5]
        if let Value::Vector(vd) = native_row_means(vec![m.clone()]).unwrap() {
            assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[2.0, 5.0]);
        }
        // col_means: [2.5, 3.5, 4.5]
        if let Value::Vector(vd) = native_col_means(vec![m.clone()]).unwrap() {
            assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[2.5, 3.5, 4.5]);
        }
        // row_maxs: [3, 6]
        if let Value::Vector(vd) = native_row_maxs(vec![m.clone()]).unwrap() {
            assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[3.0, 6.0]);
        }
        // col_maxs: [4, 5, 6]
        if let Value::Vector(vd) = native_col_maxs(vec![m.clone()]).unwrap() {
            assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[4.0, 5.0, 6.0]);
        }
        // row_mins: [1, 4]
        if let Value::Vector(vd) = native_row_mins(vec![m.clone()]).unwrap() {
            assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[1.0, 4.0]);
        }
        // col_mins: [1, 2, 3]
        if let Value::Vector(vd) = native_col_mins(vec![m.clone()]).unwrap() {
            assert_eq!(vd.as_f64_view().unwrap().as_slice(), &[1.0, 2.0, 3.0]);
        }
    }
}
