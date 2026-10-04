//! Generic Numerical Optimization Engine (RFC 15).
//!
//! Provides unconstrained and box-constrained multivariate optimization algorithms:
//! - **Nelder-Mead**: Derivative-free simplex method, robust for non-smooth functions.
//! - **BFGS**: Quasi-Newton method with backtracking Armijo line search.
//! - **L-BFGS-B**: Projected Quasi-Newton method with lower and upper box constraints.
//! - **Levenberg-Marquardt**: Damped Gauss-Newton for nonlinear least squares curve fitting.
//!
//! Exposes both internal Rust APIs (`minimize_bfgs`, `minimize_nelder_mead`, `minimize_lbfgs_b`,
//! `minimize_levenberg_marquardt`) and GHL runtime bindings:
//! - `optim(f, init, [method], [max_iter], [tol], [lower], [upper])`
//! - `nls(residuals_fn, init, [max_iter], [tol])`

use std::sync::Arc;
use std::collections::BTreeMap;
use ghl_diagnostics::Diagnostic;
use crate::eval::Interpreter;
use crate::value::Value;
use crate::vector_data::VectorData;

/// Result of a numerical optimization routine.
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub par: Vec<f64>,
    pub value: f64,
    pub converged: bool,
    pub iterations: usize,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Rust-Native Solvers
// ---------------------------------------------------------------------------

/// Minimizes `f: &[f64] -> f64` using the BFGS Quasi-Newton algorithm
/// with backtracking Armijo line search.
pub fn minimize_bfgs<F>(
    mut f: F,
    init: &[f64],
    max_iter: usize,
    tol: f64,
) -> OptimizationResult
where
    F: FnMut(&[f64]) -> f64,
{
    let n = init.len();
    if n == 0 {
        return OptimizationResult {
            par: vec![],
            value: 0.0,
            converged: true,
            iterations: 0,
            message: "Empty parameter vector".to_string(),
        };
    }

    let mut x = init.to_vec();
    let mut f_val = f(&x);

    // Initial inverse Hessian approximation H_0 = I_n
    let mut h_inv = vec![0.0; n * n];
    for i in 0..n {
        h_inv[i * n + i] = 1.0;
    }

    let mut grad = compute_gradient(&mut f, &x);
    let mut iter = 0;

    while iter < max_iter {
        iter += 1;

        // Gradient norm
        let grad_norm = grad.iter().map(|g| g * g).sum::<f64>().sqrt();
        if grad_norm < tol || grad_norm.is_nan() {
            return OptimizationResult {
                par: x,
                value: f_val,
                converged: true,
                iterations: iter,
                message: "Gradient norm below tolerance".to_string(),
            };
        }

        // Search direction d = -H * grad
        let mut d = vec![0.0; n];
        for i in 0..n {
            let mut sum = 0.0;
            for j in 0..n {
                sum += h_inv[i * n + j] * grad[j];
            }
            d[i] = -sum;
        }

        // Directional derivative: g^T d
        let dir_deriv: f64 = grad.iter().zip(&d).map(|(g, p)| g * p).sum();
        if dir_deriv >= 0.0 || dir_deriv.is_nan() {
            // Not a descent direction -- reset inverse Hessian to identity
            for i in 0..n {
                for j in 0..n {
                    h_inv[i * n + j] = if i == j { 1.0 } else { 0.0 };
                }
                d[i] = -grad[i];
            }
        }

        let dir_deriv_reset: f64 = grad.iter().zip(&d).map(|(g, p)| g * p).sum();

        // Backtracking line search (Armijo condition)
        let c1 = 1e-4;
        let mut step = 1.0;
        let min_step = 1e-12;
        let mut x_new = vec![0.0; n];
        let mut f_new = f_val;
        let mut step_accepted = false;

        while step >= min_step {
            for i in 0..n {
                x_new[i] = x[i] + step * d[i];
            }
            f_new = f(&x_new);

            if !f_new.is_nan() && f_new <= f_val + c1 * step * dir_deriv_reset {
                step_accepted = true;
                break;
            }
            step *= 0.5;
        }

        if !step_accepted {
            // Line search could not find a decrease
            return OptimizationResult {
                par: x,
                value: f_val,
                converged: iter > 5,
                iterations: iter,
                message: "Line search terminated without decrease".to_string(),
            };
        }

        // Relative function value change
        let f_diff = (f_val - f_new).abs();
        if f_diff < tol * (1.0 + f_val.abs()) {
            return OptimizationResult {
                par: x_new,
                value: f_new,
                converged: true,
                iterations: iter,
                message: "Relative function improvement below tolerance".to_string(),
            };
        }

        // Compute step s = x_new - x and gradient change y = grad_new - grad
        let s: Vec<f64> = x_new.iter().zip(&x).map(|(xn, xo)| xn - xo).collect();
        let grad_new = compute_gradient(&mut f, &x_new);
        let y: Vec<f64> = grad_new.iter().zip(&grad).map(|(gn, go)| gn - go).collect();

        let sy: f64 = s.iter().zip(&y).map(|(si, yi)| si * yi).sum();

        if sy > 1e-10 {
            // BFGS update formula for inverse Hessian
            let rho = 1.0 / sy;
            let mut h_new = h_inv.clone();

            // Compute H y
            let mut hy = vec![0.0; n];
            for i in 0..n {
                for j in 0..n {
                    hy[i] += h_inv[i * n + j] * y[j];
                }
            }

            let y_hy: f64 = y.iter().zip(&hy).map(|(yi, hyi)| yi * hyi).sum();

            for i in 0..n {
                for j in 0..n {
                    let term1 = (sy + y_hy) * (s[i] * s[j]) * (rho * rho);
                    let term2 = rho * (hy[i] * s[j] + s[i] * hy[j]);
                    h_new[i * n + j] = h_inv[i * n + j] + term1 - term2;
                }
            }

            h_inv = h_new;
        }

        x = x_new;
        f_val = f_new;
        grad = grad_new;
    }

    OptimizationResult {
        par: x,
        value: f_val,
        converged: false,
        iterations: iter,
        message: "Maximum iterations reached".to_string(),
    }
}

/// Minimizes `f: &[f64] -> f64` subject to lower and upper box constraints
/// using Projected Quasi-Newton (L-BFGS-B style) with Armijo backtracking.
pub fn minimize_lbfgs_b<F>(
    mut f: F,
    init: &[f64],
    lower: &[f64],
    upper: &[f64],
    max_iter: usize,
    tol: f64,
) -> OptimizationResult
where
    F: FnMut(&[f64]) -> f64,
{
    let n = init.len();
    if n == 0 {
        return OptimizationResult {
            par: vec![],
            value: 0.0,
            converged: true,
            iterations: 0,
            message: "Empty parameter vector".to_string(),
        };
    }

    // Clamp initial point within bounds
    let mut x: Vec<f64> = init
        .iter()
        .enumerate()
        .map(|(i, &val)| {
            let l = lower.get(i).copied().unwrap_or(f64::NEG_INFINITY);
            let u = upper.get(i).copied().unwrap_or(f64::INFINITY);
            val.clamp(l, u)
        })
        .collect();

    let mut f_val = f(&x);
    let mut h_inv = vec![0.0; n * n];
    for i in 0..n {
        h_inv[i * n + i] = 1.0;
    }

    let mut grad = compute_gradient(&mut f, &x);
    let mut iter = 0;

    while iter < max_iter {
        iter += 1;

        // Compute projected gradient
        let mut proj_grad = vec![0.0; n];
        for i in 0..n {
            let l = lower.get(i).copied().unwrap_or(f64::NEG_INFINITY);
            let u = upper.get(i).copied().unwrap_or(f64::INFINITY);
            let g = grad[i];
            if x[i] <= l + 1e-12 && g > 0.0 {
                proj_grad[i] = 0.0;
            } else if x[i] >= u - 1e-12 && g < 0.0 {
                proj_grad[i] = 0.0;
            } else {
                proj_grad[i] = g;
            }
        }

        let proj_norm = proj_grad.iter().map(|g| g * g).sum::<f64>().sqrt();
        if proj_norm < tol || proj_norm.is_nan() {
            return OptimizationResult {
                par: x,
                value: f_val,
                converged: true,
                iterations: iter,
                message: "Projected gradient norm below tolerance".to_string(),
            };
        }

        // Search direction d = -H * proj_grad
        let mut d = vec![0.0; n];
        for i in 0..n {
            let mut sum = 0.0;
            for j in 0..n {
                sum += h_inv[i * n + j] * proj_grad[j];
            }
            d[i] = -sum;
        }

        // Directional derivative
        let mut dir_deriv: f64 = proj_grad.iter().zip(&d).map(|(g, p)| g * p).sum();
        if dir_deriv >= 0.0 || dir_deriv.is_nan() {
            for i in 0..n {
                for j in 0..n {
                    h_inv[i * n + j] = if i == j { 1.0 } else { 0.0 };
                }
                d[i] = -proj_grad[i];
            }
            dir_deriv = proj_grad.iter().zip(&d).map(|(g, p)| g * p).sum();
        }

        // Backtracking line search with projection onto box constraints
        let c1 = 1e-4;
        let mut step = 1.0;
        let min_step = 1e-12;
        let mut x_new = vec![0.0; n];
        let mut f_new = f_val;
        let mut step_accepted = false;

        while step >= min_step {
            for i in 0..n {
                let l = lower.get(i).copied().unwrap_or(f64::NEG_INFINITY);
                let u = upper.get(i).copied().unwrap_or(f64::INFINITY);
                x_new[i] = (x[i] + step * d[i]).clamp(l, u);
            }
            f_new = f(&x_new);

            if !f_new.is_nan() && f_new <= f_val + c1 * step * dir_deriv {
                step_accepted = true;
                break;
            }
            step *= 0.5;
        }

        if !step_accepted {
            return OptimizationResult {
                par: x,
                value: f_val,
                converged: iter > 5,
                iterations: iter,
                message: "Line search terminated without decrease".to_string(),
            };
        }

        let f_diff = (f_val - f_new).abs();
        if f_diff < tol * (1.0 + f_val.abs()) {
            return OptimizationResult {
                par: x_new,
                value: f_new,
                converged: true,
                iterations: iter,
                message: "Relative function improvement below tolerance".to_string(),
            };
        }

        let s: Vec<f64> = x_new.iter().zip(&x).map(|(xn, xo)| xn - xo).collect();
        let grad_new = compute_gradient(&mut f, &x_new);
        let y: Vec<f64> = grad_new.iter().zip(&grad).map(|(gn, go)| gn - go).collect();
        let sy: f64 = s.iter().zip(&y).map(|(si, yi)| si * yi).sum();

        if sy > 1e-10 {
            let rho = 1.0 / sy;
            let mut h_new = h_inv.clone();
            let mut hy = vec![0.0; n];
            for i in 0..n {
                for j in 0..n {
                    hy[i] += h_inv[i * n + j] * y[j];
                }
            }
            let y_hy: f64 = y.iter().zip(&hy).map(|(yi, hyi)| yi * hyi).sum();
            for i in 0..n {
                for j in 0..n {
                    let term1 = (sy + y_hy) * (s[i] * s[j]) * (rho * rho);
                    let term2 = rho * (hy[i] * s[j] + s[i] * hy[j]);
                    h_new[i * n + j] = h_inv[i * n + j] + term1 - term2;
                }
            }
            h_inv = h_new;
        }

        x = x_new;
        f_val = f_new;
        grad = grad_new;
    }

    OptimizationResult {
        par: x,
        value: f_val,
        converged: false,
        iterations: iter,
        message: "Maximum iterations reached".to_string(),
    }
}

/// Minimizes nonlinear sum of squared residuals S(p) = 0.5 * \sum r_i(p)^2
/// using the damped Gauss-Newton Levenberg-Marquardt algorithm.
pub fn minimize_levenberg_marquardt<F>(
    mut residuals: F,
    init: &[f64],
    max_iter: usize,
    tol: f64,
) -> OptimizationResult
where
    F: FnMut(&[f64]) -> Vec<f64>,
{
    let n = init.len();
    if n == 0 {
        return OptimizationResult {
            par: vec![],
            value: 0.0,
            converged: true,
            iterations: 0,
            message: "Empty parameter vector".to_string(),
        };
    }

    let mut p = init.to_vec();
    let mut r = residuals(&p);
    let m = r.len();
    if m < n {
        return OptimizationResult {
            par: p,
            value: 0.0,
            converged: false,
            iterations: 0,
            message: "Underdetermined system: fewer residuals than parameters".to_string(),
        };
    }

    let mut s_val: f64 = 0.5 * r.iter().map(|ri| ri * ri).sum::<f64>();
    let mut lambda = 1e-3;
    let mut iter = 0;

    while iter < max_iter {
        iter += 1;

        // Compute numerical Jacobian J (m x n)
        let mut jac = vec![0.0; m * n];
        for j in 0..n {
            let orig = p[j];
            let h = 1e-6 * (1.0 + orig.abs());
            p[j] = orig + h;
            let r_plus = residuals(&p);
            p[j] = orig - h;
            let r_minus = residuals(&p);
            p[j] = orig;

            for i in 0..m {
                jac[i * n + j] = (r_plus[i] - r_minus[i]) / (2.0 * h);
            }
        }

        // Form J^T J (n x n) and g = J^T r (n)
        let mut jtj = vec![0.0; n * n];
        let mut g = vec![0.0; n];
        for j in 0..n {
            for k in 0..n {
                let mut sum = 0.0;
                for i in 0..m {
                    sum += jac[i * n + j] * jac[i * n + k];
                }
                jtj[j * n + k] = sum;
            }
            let mut sum_g = 0.0;
            for i in 0..m {
                sum_g += jac[i * n + j] * r[i];
            }
            g[j] = sum_g;
        }

        // Check gradient norm
        let g_norm = g.iter().map(|gi| gi.abs()).fold(0.0, f64::max);
        if g_norm < tol {
            return OptimizationResult {
                par: p,
                value: s_val,
                converged: true,
                iterations: iter,
                message: "Gradient norm below tolerance".to_string(),
            };
        }

        // Form augmented system (J^T J + \lambda * (diag(J^T J) + 1e-6 I)) dp = -g
        let mut a = jtj.clone();
        for j in 0..n {
            let diag_term = jtj[j * n + j].abs();
            a[j * n + j] += lambda * (if diag_term > 1e-12 { diag_term } else { 1.0 });
        }

        let neg_g: Vec<f64> = g.iter().map(|gi| -gi).collect();
        let dp_res = crate::matrix::MatrixOps::solve(n, &a, &neg_g);

        let dp = match dp_res {
            Ok(step_vec) => step_vec,
            Err(_) => {
                // If singular, increase damping lambda and retry
                lambda *= 10.0;
                continue;
            }
        };

        let p_candidate: Vec<f64> = p.iter().zip(&dp).map(|(pi, dpi)| pi + dpi).collect();
        let r_candidate = residuals(&p_candidate);
        let s_candidate: f64 = 0.5 * r_candidate.iter().map(|ri| ri * ri).sum::<f64>();

        if s_candidate < s_val {
            // Successful step: accept and reduce damping
            p = p_candidate;
            r = r_candidate;
            let s_diff = s_val - s_candidate;
            s_val = s_candidate;
            lambda = (lambda * 0.1).max(1e-10);

            // Step size convergence
            let dp_norm = dp.iter().map(|v| v * v).sum::<f64>().sqrt();
            if dp_norm < tol * (1.0 + p.iter().map(|v| v * v).sum::<f64>().sqrt()) || s_diff < tol {
                return OptimizationResult {
                    par: p,
                    value: s_val,
                    converged: true,
                    iterations: iter,
                    message: "Levenberg-Marquardt step converged".to_string(),
                };
            }
        } else {
            // Unsuccessful step: reject candidate and increase damping
            lambda = (lambda * 10.0).min(1e10);
        }
    }

    OptimizationResult {
        par: p,
        value: s_val,
        converged: false,
        iterations: iter,
        message: "Maximum iterations reached".to_string(),
    }
}

/// Minimizes `f: &[f64] -> f64` using the Nelder-Mead Simplex method.
pub fn minimize_nelder_mead<F>(
    mut f: F,
    init: &[f64],
    max_iter: usize,
    tol: f64,
) -> OptimizationResult
where
    F: FnMut(&[f64]) -> f64,
{
    let n = init.len();
    if n == 0 {
        return OptimizationResult {
            par: vec![],
            value: 0.0,
            converged: true,
            iterations: 0,
            message: "Empty parameter vector".to_string(),
        };
    }

    let alpha = 1.0;  // Reflection
    let gamma = 2.0;  // Expansion
    let rho = 0.5;    // Contraction
    let sigma = 0.5;  // Shrink

    // Build simplex with n + 1 vertices
    let mut simplex: Vec<(Vec<f64>, f64)> = Vec::with_capacity(n + 1);
    let f0 = f(init);
    simplex.push((init.to_vec(), f0));

    for i in 0..n {
        let mut p = init.to_vec();
        let h = if p[i].abs() > 1e-4 { 0.05 * p[i] } else { 0.00025 };
        p[i] += h;
        let fp = f(&p);
        simplex.push((p, fp));
    }

    let mut iter = 0;

    while iter < max_iter {
        iter += 1;

        // Sort vertices by function value
        simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Check convergence based on simplex variation
        let f_best = simplex[0].1;
        let f_worst = simplex[n].1;
        if (f_worst - f_best).abs() < tol * (1.0 + f_best.abs()) {
            return OptimizationResult {
                par: simplex[0].0.clone(),
                value: f_best,
                converged: true,
                iterations: iter,
                message: "Simplex variation below tolerance".to_string(),
            };
        }

        // Centroid of the best n vertices (excluding the worst vertex simplex[n])
        let mut centroid = vec![0.0; n];
        for i in 0..n {
            for j in 0..n {
                centroid[j] += simplex[i].0[j];
            }
        }
        for j in 0..n {
            centroid[j] /= n as f64;
        }

        // Reflection
        let worst = &simplex[n].0;
        let mut reflected = vec![0.0; n];
        for j in 0..n {
            reflected[j] = centroid[j] + alpha * (centroid[j] - worst[j]);
        }
        let f_refl = f(&reflected);

        if f_best <= f_refl && f_refl < simplex[n - 1].1 {
            simplex[n] = (reflected, f_refl);
            continue;
        }

        // Expansion
        if f_refl < f_best {
            let mut expanded = vec![0.0; n];
            for j in 0..n {
                expanded[j] = centroid[j] + gamma * (reflected[j] - centroid[j]);
            }
            let f_exp = f(&expanded);
            if f_exp < f_refl {
                simplex[n] = (expanded, f_exp);
            } else {
                simplex[n] = (reflected, f_refl);
            }
            continue;
        }

        // Contraction
        if f_refl < simplex[n].1 {
            // Outside contraction
            let mut contracted = vec![0.0; n];
            for j in 0..n {
                contracted[j] = centroid[j] + rho * (reflected[j] - centroid[j]);
            }
            let f_cont = f(&contracted);
            if f_cont <= f_refl {
                simplex[n] = (contracted, f_cont);
                continue;
            }
        } else {
            // Inside contraction
            let mut contracted = vec![0.0; n];
            for j in 0..n {
                contracted[j] = centroid[j] + rho * (worst[j] - centroid[j]);
            }
            let f_cont = f(&contracted);
            if f_cont < simplex[n].1 {
                simplex[n] = (contracted, f_cont);
                continue;
            }
        }

        // Shrink towards best vertex
        let best_pt = simplex[0].0.clone();
        for i in 1..=n {
            for j in 0..n {
                simplex[i].0[j] = best_pt[j] + sigma * (simplex[i].0[j] - best_pt[j]);
            }
            simplex[i].1 = f(&simplex[i].0);
        }
    }

    simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    OptimizationResult {
        par: simplex[0].0.clone(),
        value: simplex[0].1,
        converged: false,
        iterations: iter,
        message: "Maximum iterations reached".to_string(),
    }
}

/// Central finite difference gradient estimation: ∇f(x).
fn compute_gradient<F>(f: &mut F, x: &[f64]) -> Vec<f64>
where
    F: FnMut(&[f64]) -> f64,
{
    let n = x.len();
    let mut grad = Vec::with_capacity(n);
    let mut x_mut = x.to_vec();

    for i in 0..n {
        let orig = x_mut[i];
        let h = 1e-6 * (1.0 + orig.abs());

        x_mut[i] = orig + h;
        let fp = f(&x_mut);

        x_mut[i] = orig - h;
        let fm = f(&x_mut);

        x_mut[i] = orig;
        grad.push((fp - fm) / (2.0 * h));
    }

    grad
}

// ---------------------------------------------------------------------------
// GHL Runtime Native Function Bindings
// ---------------------------------------------------------------------------

/// `optim(f, init, [method], [max_iter], [tol], [lower], [upper])` — General-purpose numerical optimizer.
pub fn native_optim(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`optim()` requires at least 2 arguments: function and initial vector `optim(fn, init)`",
        ));
    }

    let callable = args[0].clone();
    let init_vec = match &args[1] {
        Value::Vector(vd) => {
            let mut v = Vec::with_capacity(vd.len());
            for item in vd.iter() {
                v.push(extract_numeric(item)?);
            }
            v
        }
        Value::F64(x) => vec![*x],
        Value::I64(n) => vec![*n as f64],
        other => {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!("`optim()` second argument must be Vector or numeric, found `{}`", other.type_name()),
            ));
        }
    };

    let method = args
        .get(2)
        .and_then(|v| match v {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("BFGS");

    let max_iter = args
        .get(3)
        .and_then(|v| match v {
            Value::I64(n) => Some(*n as usize),
            Value::F64(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(1000);

    let tol = args
        .get(4)
        .and_then(|v| match v {
            Value::F64(f) => Some(*f),
            Value::I64(n) => Some(*n as f64),
            _ => None,
        })
        .unwrap_or(1e-6);

    let lower_vec = args.get(5).and_then(|v| match v {
        Value::Vector(vd) => {
            let mut v = Vec::with_capacity(vd.len());
            for item in vd.iter() {
                v.push(extract_numeric(item).unwrap_or(f64::NEG_INFINITY));
            }
            Some(v)
        }
        _ => None,
    });

    let upper_vec = args.get(6).and_then(|v| match v {
        Value::Vector(vd) => {
            let mut v = Vec::with_capacity(vd.len());
            for item in vd.iter() {
                v.push(extract_numeric(item).unwrap_or(f64::INFINITY));
            }
            Some(v)
        }
        _ => None,
    });

    let objective = |point: &[f64]| -> f64 {
        let arg = Value::Vector(VectorData::from_f64(point.to_vec()));
        match interp.call_value(callable.clone(), vec![arg]) {
            Ok(v) => match extract_numeric(&v) {
                Ok(val) => val,
                Err(_) => f64::NAN,
            },
            Err(_) => f64::NAN,
        }
    };

    let has_bounds = lower_vec.is_some() || upper_vec.is_some();
    let is_lbfgs = method.eq_ignore_ascii_case("L-BFGS-B")
        || method.eq_ignore_ascii_case("LBFGSB")
        || method.eq_ignore_ascii_case("LBFGS-B")
        || method.eq_ignore_ascii_case("L-BFGS");

    let res = if method.eq_ignore_ascii_case("Nelder-Mead") || method.eq_ignore_ascii_case("NelderMead") {
        minimize_nelder_mead(objective, &init_vec, max_iter, tol)
    } else if is_lbfgs || has_bounds {
        let lower = lower_vec.unwrap_or_else(|| vec![f64::NEG_INFINITY; init_vec.len()]);
        let upper = upper_vec.unwrap_or_else(|| vec![f64::INFINITY; init_vec.len()]);
        minimize_lbfgs_b(objective, &init_vec, &lower, &upper, max_iter, tol)
    } else {
        minimize_bfgs(objective, &init_vec, max_iter, tol)
    };

    let mut record = BTreeMap::new();
    record.insert("par".to_string(), Value::Vector(VectorData::from_f64(res.par)));
    record.insert("value".to_string(), Value::F64(res.value));
    record.insert("converged".to_string(), Value::Bool(res.converged));
    record.insert("iterations".to_string(), Value::I64(res.iterations as i64));
    record.insert("message".to_string(), Value::String(res.message));

    Ok(Value::Record(Arc::new(record)))
}

/// `nls(residuals_fn, init, [max_iter], [tol])` — Nonlinear least squares via Levenberg-Marquardt.
pub fn native_nls(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`nls()` requires at least 2 arguments: residuals function and initial vector `nls(fn, init)`",
        ));
    }

    let callable = args[0].clone();
    let init_vec = match &args[1] {
        Value::Vector(vd) => {
            let mut v = Vec::with_capacity(vd.len());
            for item in vd.iter() {
                v.push(extract_numeric(item)?);
            }
            v
        }
        Value::F64(x) => vec![*x],
        Value::I64(n) => vec![*n as f64],
        other => {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!("`nls()` second argument must be Vector or numeric, found `{}`", other.type_name()),
            ));
        }
    };

    let max_iter = args
        .get(2)
        .and_then(|v| match v {
            Value::I64(n) => Some(*n as usize),
            Value::F64(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(1000);

    let tol = args
        .get(3)
        .and_then(|v| match v {
            Value::F64(f) => Some(*f),
            Value::I64(n) => Some(*n as f64),
            _ => None,
        })
        .unwrap_or(1e-6);

    let residuals_fn = |point: &[f64]| -> Vec<f64> {
        let arg = Value::Vector(VectorData::from_f64(point.to_vec()));
        match interp.call_value(callable.clone(), vec![arg]) {
            Ok(Value::Vector(vd)) => {
                let mut out = Vec::with_capacity(vd.len());
                for item in vd.iter() {
                    if let Ok(num) = extract_numeric(item) {
                        out.push(num);
                    } else {
                        out.push(f64::NAN);
                    }
                }
                out
            }
            Ok(val) => match extract_numeric(&val) {
                Ok(num) => vec![num],
                Err(_) => vec![f64::NAN],
            },
            Err(_) => vec![f64::NAN],
        }
    };

    let res = minimize_levenberg_marquardt(residuals_fn, &init_vec, max_iter, tol);

    let mut record = BTreeMap::new();
    record.insert("par".to_string(), Value::Vector(VectorData::from_f64(res.par)));
    record.insert("value".to_string(), Value::F64(res.value));
    record.insert("converged".to_string(), Value::Bool(res.converged));
    record.insert("iterations".to_string(), Value::I64(res.iterations as i64));
    record.insert("message".to_string(), Value::String(res.message));

    Ok(Value::Record(Arc::new(record)))
}

fn extract_numeric(val: &Value) -> Result<f64, Diagnostic> {
    match val {
        Value::F64(f) => Ok(*f),
        Value::I64(n) => Ok(*n as f64),
        other => Err(Diagnostic::compute_error(
            "C0202",
            format!("Expected numeric parameter, found `{}`", other.type_name()),
        )),
    }
}
