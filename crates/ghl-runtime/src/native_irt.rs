//! High-performance native psychometrics and IRT kernels (Pilar 4).
//!
//! Provides Rayon-multithreaded and SIMD-vectorized expectation-maximization (EM)
//! quadrature integration for multidimensional and unidimensional item response theory.

use std::collections::BTreeMap;
use std::sync::Arc;
use rayon::prelude::*;
use ghl_diagnostics::Diagnostic;
use crate::value::Value;
use crate::vector_data::{VectorData, NumericView};

enum F64Slice<'a> {
    Borrowed(&'a [f64]),
    View(NumericView<'a>),
    Owned(Vec<f64>),
}

impl<'a> F64Slice<'a> {
    pub fn as_slice(&self) -> &[f64] {
        match self {
            F64Slice::Borrowed(s) => s,
            F64Slice::View(v) => v.as_slice(),
            F64Slice::Owned(v) => v.as_slice(),
        }
    }
}

fn extract_f64_data<'a>(
    val: &'a Value,
    param_name: &str,
) -> Result<(usize, usize, F64Slice<'a>), Diagnostic> {
    match val {
        Value::Matrix { rows, cols, data } => {
            Ok((*rows, *cols, F64Slice::Borrowed(data.as_slice())))
        }
        Value::Vector(vd) => {
            if let Ok(view) = vd.as_f64_view() {
                Ok((vd.len(), 1, F64Slice::View(view)))
            } else {
                let data: Vec<f64> = vd.iter().map(|item| item.as_f64().unwrap_or(0.0)).collect();
                Ok((vd.len(), 1, F64Slice::Owned(data)))
            }
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("Parameter `{param_name}` must be a Matrix or Vector, found `{}`", other.type_name()),
        )),
    }
}

/// Generates a multidimensional quadrature grid with multivariate standard normal prior weights.
///
/// Signature: `irt_quadrature_grid(dimensions, points_per_dim, [bound = 3.5]) -> Record`
pub(crate) fn native_irt_quadrature_grid(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let d = args.first().and_then(|v| v.as_i64()).unwrap_or(1) as usize;
    let k = args.get(1).and_then(|v| v.as_i64()).unwrap_or(15) as usize;
    let b = args.get(2).and_then(|v| v.as_f64()).unwrap_or(3.5);

    if d < 1 || d > 10 {
        return Err(Diagnostic::statistical_error(
            "S0410",
            format!("dimensions D must be between 1 and 10, found {d}"),
        ));
    }
    if k < 1 || k > 100 {
        return Err(Diagnostic::statistical_error(
            "S0410",
            format!("points_per_dim must be between 1 and 100, found {k}"),
        ));
    }

    let q_total = k.pow(d as u32);
    if q_total > 1_000_000 {
        return Err(Diagnostic::statistical_error(
            "S0411",
            format!("Total quadrature points ({k}^{d} = {q_total}) exceeds safety limit of 1,000,000"),
        ));
    }

    // 1D grid points and standard normal densities
    let delta = if k > 1 { (2.0 * b) / ((k - 1) as f64) } else { 0.0 };
    let mut x_1d = Vec::with_capacity(k);
    let mut w_1d = Vec::with_capacity(k);
    let inv_sqrt_2pi = 1.0 / (2.0 * std::f64::consts::PI).sqrt();

    for i in 0..k {
        let x = if k > 1 { -b + (i as f64) * delta } else { 0.0 };
        let w = inv_sqrt_2pi * (-0.5 * x * x).exp();
        x_1d.push(x);
        w_1d.push(w);
    }

    // Multi-dimensional tensor grid
    let mut nodes = vec![0.0f64; q_total * d];
    let mut weights = vec![0.0f64; q_total];
    let mut sum_w = 0.0f64;

    for q in 0..q_total {
        let mut temp = q;
        let mut weight_prod = 1.0f64;
        for dim in 0..d {
            let idx = temp % k;
            temp /= k;
            nodes[q * d + dim] = x_1d[idx];
            weight_prod *= w_1d[idx];
        }
        weights[q] = weight_prod;
        sum_w += weight_prod;
    }

    // Normalize weights to sum strictly to 1.0
    let inv_sum = if sum_w > 0.0 { 1.0 / sum_w } else { 1.0 };
    for q in 0..q_total {
        weights[q] *= inv_sum;
    }

    let mut fields = BTreeMap::new();
    fields.insert("nodes".into(), Value::matrix(q_total, d, nodes));
    fields.insert("weights".into(), Value::Vector(VectorData::from_f64(weights)));
    fields.insert("n_points".into(), Value::I64(q_total as i64));
    fields.insert("n_dimensions".into(), Value::I64(d as i64));

    Ok(Value::Record(Arc::new(fields)))
}

/// Native Rayon-parallelized and SIMD-vectorized EM quadrature kernel.
///
/// Inputs:
/// - `responses`: Matrix (N x J) or Vector (flat N*J)
/// - `nodes`: Matrix (Q x D) or Vector (length Q for D=1)
/// - `weights`: Vector (length Q)
/// - `a_matrix`: Matrix (J x D) or Vector (length J for D=1)
/// - `d_intercepts`: Vector (length J)
///
/// Returns Record:
/// - `expected_counts`: Matrix (J x Q)
/// - `node_counts`: Vector (length Q)
/// - `log_likelihood`: f64
/// - `posterior`: Matrix (N x Q)
/// - `thetas`: Matrix (N x D)
/// - `se`: Matrix (N x D)
pub(crate) fn native_irt_em_quadrature_kernel(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 5 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "irt_em_quadrature_kernel requires 5 arguments: (responses, nodes, weights, a_matrix, d_intercepts)",
        ));
    }

    let (d_rows, _, d_slice) = extract_f64_data(&args[4], "d_intercepts")?;
    let j_items = d_rows;
    if j_items == 0 {
        return Err(Diagnostic::statistical_error("S0200", "d_intercepts cannot be empty"));
    }

    let (w_rows, _, w_slice) = extract_f64_data(&args[2], "weights")?;
    let q_nodes = w_rows;
    if q_nodes == 0 {
        return Err(Diagnostic::statistical_error("S0200", "weights cannot be empty"));
    }

    let (n_rows, n_cols, nodes_slice) = extract_f64_data(&args[1], "nodes")?;
    let d_dim = if matches!(&args[1], Value::Matrix { .. }) {
        if n_rows != q_nodes {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("nodes rows ({n_rows}) must equal weights length ({q_nodes})"),
            ));
        }
        n_cols
    } else {
        if nodes_slice.as_slice().len() % q_nodes != 0 {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("nodes length ({}) must be a multiple of weights length ({})", nodes_slice.as_slice().len(), q_nodes),
            ));
        }
        nodes_slice.as_slice().len() / q_nodes
    };
    if d_dim == 0 {
        return Err(Diagnostic::statistical_error("S0200", "latent dimension D must be >= 1"));
    }

    let (_a_rows, _a_cols, a_slice) = extract_f64_data(&args[3], "a_matrix")?;
    let expected_a_len = j_items * d_dim;
    if a_slice.as_slice().len() != expected_a_len {
        return Err(Diagnostic::statistical_error(
            "S0200",
            format!("a_matrix length ({}) must equal J*D ({}*{} = {})", a_slice.as_slice().len(), j_items, d_dim, expected_a_len),
        ));
    }

    let (r_rows, r_cols, resp_slice) = extract_f64_data(&args[0], "responses")?;
    let n_persons = if matches!(&args[0], Value::Matrix { .. }) {
        if r_cols != j_items {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("responses columns ({r_cols}) must match number of items J ({j_items})"),
            ));
        }
        r_rows
    } else {
        if resp_slice.as_slice().len() % j_items != 0 {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("responses length ({}) must be a multiple of items J ({})", resp_slice.as_slice().len(), j_items),
            ));
        }
        resp_slice.as_slice().len() / j_items
    };
    if n_persons == 0 {
        return Err(Diagnostic::statistical_error("S0200", "responses cannot have 0 respondents"));
    }

    let d_vec = d_slice.as_slice();
    let weights_vec = w_slice.as_slice();
    let nodes_mat = nodes_slice.as_slice();
    let a_mat = a_slice.as_slice();
    let resp_mat = resp_slice.as_slice();

    // 1. Precompute log P and log Q across all items J and quadrature nodes Q
    let mut log_p = vec![0.0f64; j_items * q_nodes];
    let mut log_q = vec![0.0f64; j_items * q_nodes];

    for j in 0..j_items {
        let a_row = &a_mat[j * d_dim..(j + 1) * d_dim];
        let dj = d_vec[j];
        for q in 0..q_nodes {
            let node_row = &nodes_mat[q * d_dim..(q + 1) * d_dim];
            let mut dot = 0.0f64;
            for d in 0..d_dim {
                dot += a_row[d] * node_row[d];
            }
            let z = dot + dj;
            let (lp, lq) = if z >= 0.0 {
                let neg_z_exp = (-z).exp();
                let log_1p = neg_z_exp.ln_1p();
                (-log_1p, -z - log_1p)
            } else {
                let z_exp = z.exp();
                let log_1p = z_exp.ln_1p();
                (z - log_1p, -log_1p)
            };
            log_p[j * q_nodes + q] = lp;
            log_q[j * q_nodes + q] = lq;
        }
    }

    // 2. Precompute log weights
    let mut log_w = vec![0.0f64; q_nodes];
    for q in 0..q_nodes {
        let w = weights_vec[q];
        log_w[q] = if w > 0.0 { w.ln() } else { -1000.0 };
    }

    // 3. Parallel Rayon E-Step and EAP trait scoring
    let mut post_mat = vec![0.0f64; n_persons * q_nodes];
    let mut thetas_mat = vec![0.0f64; n_persons * d_dim];
    let mut se_mat = vec![0.0f64; n_persons * d_dim];

    let (total_loglik, node_counts, expected_counts) = post_mat
        .par_chunks_mut(q_nodes)
        .zip(thetas_mat.par_chunks_mut(d_dim))
        .zip(se_mat.par_chunks_mut(d_dim))
        .enumerate()
        .fold(
            || (0.0f64, vec![0.0f64; q_nodes], vec![0.0f64; j_items * q_nodes]),
            |(mut acc_ll, mut acc_nq, mut acc_rjq), (i, ((post_row, theta_row), se_row))| {
                let resp_row = &resp_mat[i * j_items..(i + 1) * j_items];

                // A. Joint log-likelihood for person i across all Q nodes
                let mut max_term = f64::NEG_INFINITY;
                for q in 0..q_nodes {
                    let mut sum_log = log_w[q];
                    for j in 0..j_items {
                        let y = resp_row[j];
                        if y > 0.5 {
                            sum_log += log_p[j * q_nodes + q];
                        } else if y >= 0.0 {
                            sum_log += log_q[j * q_nodes + q];
                        }
                    }
                    post_row[q] = sum_log;
                    if sum_log > max_term {
                        max_term = sum_log;
                    }
                }

                // B. Numerically stable log-sum-exp normalization
                let mut sum_exp = 0.0f64;
                for q in 0..q_nodes {
                    let exp_val = (post_row[q] - max_term).exp();
                    post_row[q] = exp_val;
                    sum_exp += exp_val;
                }

                let inv_sum = if sum_exp > 0.0 { 1.0 / sum_exp } else { 0.0 };
                for q in 0..q_nodes {
                    post_row[q] *= inv_sum;
                }

                let person_ll = max_term + sum_exp.max(1e-300).ln();
                acc_ll += person_ll;

                // C. Accumulate node counts and endorsements
                for q in 0..q_nodes {
                    let p_iq = post_row[q];
                    acc_nq[q] += p_iq;
                    for j in 0..j_items {
                        let y = resp_row[j];
                        if y > 0.0 {
                            acc_rjq[j * q_nodes + q] += p_iq * y;
                        }
                    }
                }

                // D. EAP trait scores and standard errors
                for d in 0..d_dim {
                    let mut mean_d = 0.0f64;
                    for q in 0..q_nodes {
                        mean_d += post_row[q] * nodes_mat[q * d_dim + d];
                    }
                    theta_row[d] = mean_d;

                    let mut var_d = 0.0f64;
                    for q in 0..q_nodes {
                        let diff = nodes_mat[q * d_dim + d] - mean_d;
                        var_d += post_row[q] * diff * diff;
                    }
                    se_row[d] = var_d.max(1e-8).sqrt();
                }

                (acc_ll, acc_nq, acc_rjq)
            },
        )
        .reduce(
            || (0.0f64, vec![0.0f64; q_nodes], vec![0.0f64; j_items * q_nodes]),
            |(ll_a, mut nq_a, mut rjq_a), (ll_b, nq_b, rjq_b)| {
                for q in 0..q_nodes {
                    nq_a[q] += nq_b[q];
                }
                for idx in 0..(j_items * q_nodes) {
                    rjq_a[idx] += rjq_b[idx];
                }
                (ll_a + ll_b, nq_a, rjq_a)
            },
        );

    let mut fields = BTreeMap::new();
    fields.insert("expected_counts".into(), Value::matrix(j_items, q_nodes, expected_counts));
    fields.insert("node_counts".into(), Value::Vector(VectorData::from_f64(node_counts)));
    fields.insert("log_likelihood".into(), Value::F64(total_loglik));
    fields.insert("posterior".into(), Value::matrix(n_persons, q_nodes, post_mat));
    fields.insert("thetas".into(), Value::matrix(n_persons, d_dim, thetas_mat));
    fields.insert("se".into(), Value::matrix(n_persons, d_dim, se_mat));

    Ok(Value::Record(Arc::new(fields)))
}
