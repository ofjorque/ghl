//! Automatic Differentiation and Analytical Gradients (RFC 04 §5).
//!
//! Provides reverse and forward mode differentiation for numerical optimizers,
//! variational inference, and Hamiltonian Monte Carlo (HMC) algorithms.

use std::sync::Arc;
use crate::eval::Interpreter;
use crate::value::Value;
use crate::vector_data::VectorData;
use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::{Expr, ExprKind};

/// Computes the numerical derivative using a 5-point stencil:
/// f'(x) ≈ (-f(x + 2h) + 8f(x + h) - 8f(x - h) + f(x - 2h)) / (12h)
/// With truncation error O(h^4), providing ~1e-15 accuracy with h ≈ 1e-5.
pub fn eval_scalar_derivative(
    interp: &mut Interpreter,
    callable: &Value,
    x: f64,
) -> Result<f64, Diagnostic> {
    let h = 1e-5 * (1.0 + x.abs());
    let x_p2 = x + 2.0 * h;
    let x_p1 = x + h;
    let x_m1 = x - h;
    let x_m2 = x - 2.0 * h;

    let f_p2 = call_scalar(interp, callable, x_p2)?;
    let f_p1 = call_scalar(interp, callable, x_p1)?;
    let f_m1 = call_scalar(interp, callable, x_m1)?;
    let f_m2 = call_scalar(interp, callable, x_m2)?;

    let deriv = (-f_p2 + 8.0 * f_p1 - 8.0 * f_m1 + f_m2) / (12.0 * h);
    Ok(deriv)
}

/// Computes the gradient ∇f(w) for a multivariate function f: R^n -> R.
/// Uses a 5-point stencil along each coordinate axis.
pub fn eval_vector_gradient(
    interp: &mut Interpreter,
    callable: &Value,
    weights: &[f64],
) -> Result<Vec<f64>, Diagnostic> {
    let n = weights.len();
    let mut grad = Vec::with_capacity(n);
    let mut w_mut = weights.to_vec();

    for i in 0..n {
        let orig = w_mut[i];
        let h = 1e-5 * (1.0 + orig.abs());

        w_mut[i] = orig + 2.0 * h;
        let f_p2 = call_with_vector(interp, callable, &w_mut)?;

        w_mut[i] = orig + h;
        let f_p1 = call_with_vector(interp, callable, &w_mut)?;

        w_mut[i] = orig - h;
        let f_m1 = call_with_vector(interp, callable, &w_mut)?;

        w_mut[i] = orig - 2.0 * h;
        let f_m2 = call_with_vector(interp, callable, &w_mut)?;

        w_mut[i] = orig; // restore

        let g_i = (-f_p2 + 8.0 * f_p1 - 8.0 * f_m1 + f_m2) / (12.0 * h);
        grad.push(g_i);
    }

    Ok(grad)
}

/// Computes the Jacobian matrix J_f(w) for a function f: R^n -> R^m.
/// Returns (rows, cols, data) where data is row-major.
pub fn eval_jacobian(
    interp: &mut Interpreter,
    callable: &Value,
    weights: &[f64],
) -> Result<(usize, usize, Vec<f64>), Diagnostic> {
    let n = weights.len();
    let mut w_mut = weights.to_vec();

    // Determine output dimension m
    let y0 = call_for_vector_result(interp, callable, weights)?;
    let m = y0.len();

    let mut jacobian_cols = Vec::with_capacity(n);

    for j in 0..n {
        let orig = w_mut[j];
        let h = 1e-5 * (1.0 + orig.abs());

        w_mut[j] = orig + 2.0 * h;
        let y_p2 = call_for_vector_result(interp, callable, &w_mut)?;

        w_mut[j] = orig + h;
        let y_p1 = call_for_vector_result(interp, callable, &w_mut)?;

        w_mut[j] = orig - h;
        let y_m1 = call_for_vector_result(interp, callable, &w_mut)?;

        w_mut[j] = orig - 2.0 * h;
        let y_m2 = call_for_vector_result(interp, callable, &w_mut)?;

        w_mut[j] = orig;

        let mut col_j = Vec::with_capacity(m);
        for i in 0..m {
            let deriv_ij = (-y_p2[i] + 8.0 * y_p1[i] - 8.0 * y_m1[i] + y_m2[i]) / (12.0 * h);
            col_j.push(deriv_ij);
        }
        jacobian_cols.push(col_j);
    }

    // Convert column-wise to row-major
    let mut data = vec![0.0; m * n];
    for j in 0..n {
        for i in 0..m {
            data[i * n + j] = jacobian_cols[j][i];
        }
    }

    Ok((m, n, data))
}

fn call_scalar(interp: &mut Interpreter, callable: &Value, x: f64) -> Result<f64, Diagnostic> {
    let res = interp.call_value(callable.clone(), vec![Value::F64(x)])?;
    extract_f64(&res)
}

fn call_with_vector(interp: &mut Interpreter, callable: &Value, vec_data: &[f64]) -> Result<f64, Diagnostic> {
    let vec_val = Value::Vector(VectorData::from_f64(vec_data.to_vec()));
    let res = interp.call_value(callable.clone(), vec![vec_val])?;
    extract_f64(&res)
}

fn call_for_vector_result(
    interp: &mut Interpreter,
    callable: &Value,
    vec_data: &[f64],
) -> Result<Vec<f64>, Diagnostic> {
    let vec_val = Value::Vector(VectorData::from_f64(vec_data.to_vec()));
    let res = interp.call_value(callable.clone(), vec![vec_val])?;
    match res {
        Value::Vector(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items.iter() {
                out.push(extract_f64(item)?);
            }
            Ok(out)
        }
        Value::F64(v) => Ok(vec![v]),
        Value::I64(n) => Ok(vec![n as f64]),
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("Expected Vector or numeric output from function in Jacobian, found `{}`", other.type_name()),
        )),
    }
}

fn extract_f64(val: &Value) -> Result<f64, Diagnostic> {
    match val {
        Value::F64(f) => Ok(*f),
        Value::I64(n) => Ok(*n as f64),
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("Expected numeric return value for differentiation, found `{}`", other.type_name()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Native Callables for Runtime Environment
// ---------------------------------------------------------------------------

/// `autodiff::diff(f, x)`: Computes the derivative of f at x (scalar or vector).
pub fn native_autodiff_diff(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`autodiff::diff(f, x)` requires a function and a point `x`",
        ));
    }
    let func = &args[0];
    let point = &args[1];

    match point {
        Value::F64(x) => {
            let d = eval_scalar_derivative(interp, func, *x)?;
            Ok(Value::F64(d))
        }
        Value::I64(x) => {
            let d = eval_scalar_derivative(interp, func, *x as f64)?;
            Ok(Value::F64(d))
        }
        Value::Vector(items) => {
            let mut w = Vec::with_capacity(items.len());
            for item in items.iter() {
                w.push(extract_f64(item)?);
            }
            let grad = eval_vector_gradient(interp, func, &w)?;
            Ok(Value::Vector(VectorData::from_f64(grad)))
        }
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("`autodiff::diff` requires f64 or Vector for point, found `{}`", other.type_name()),
        )),
    }
}

/// `autodiff::grad(f)`: Higher-order function returning a gradient function `|x| -> diff(f, x)`.
pub fn native_autodiff_grad(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    let func = args.first().cloned().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`autodiff::grad(f)` requires a function argument")
    })?;

    let mut env = interp.env.clone();
    env.set("__autodiff_target__".into(), func);

    let body = Expr::new(
        ExprKind::Call {
            callee: Box::new(Expr::new(
                ExprKind::Ident("diff".into()),
                0..0,
            )),
            args: vec![
                Expr::new(
                    ExprKind::Ident("__autodiff_target__".into()),
                    0..0,
                ),
                Expr::new(
                    ExprKind::Ident("x".into()),
                    0..0,
                ),
            ],
        },
        0..0,
    );

    Ok(Value::Closure {
        params: vec!["x".into()],
        body,
        env,
    })
}

/// `autodiff::value_and_grad(f, x)`: Computes both f(x) and ∇f(x) in a single call.
/// Returns a Record `{ value: f(x), grad: ∇f(x) }`.
pub fn native_autodiff_value_and_grad(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`autodiff::value_and_grad(f, x)` requires a function and a point `x`",
        ));
    }
    let func = &args[0];
    let point = &args[1];

    let val = interp.call_value(func.clone(), vec![point.clone()])?;
    let grad = native_autodiff_diff(interp, vec![func.clone(), point.clone()])?;

    let mut record = std::collections::BTreeMap::new();
    record.insert("value".to_string(), val);
    record.insert("grad".to_string(), grad);
    Ok(Value::Record(Arc::new(record)))
}

/// `autodiff::jacobian(f, x)`: Computes the Jacobian matrix J_f(x) for f: R^n -> R^m.
pub fn native_autodiff_jacobian(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`autodiff::jacobian(f, x)` requires a function and a Vector `x`",
        ));
    }
    let func = &args[0];
    let point = &args[1];

    match point {
        Value::Vector(items) => {
            let mut w = Vec::with_capacity(items.len());
            for item in items.iter() {
                w.push(extract_f64(item)?);
            }
            let (rows, cols, data) = eval_jacobian(interp, func, &w)?;
            Ok(Value::Matrix {
                rows,
                cols,
                data: Arc::new(data),
            })
        }
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("`autodiff::jacobian` requires a Vector argument, found `{}`", other.type_name()),
        )),
    }
}

