//! High-Dimensional Fixed Effects OLS (`feols`) Core Engine (Roadmap 08, Paso B.2).
//!
//! Provides the mathematical Layer 0 and native estimator for high-dimensional panel data:
//! - Multi-factor demeaning via the Method of Alternating Projections (MAP) in O(N) memory.
//! - Single-pass exact group-mean centering for D = 1 fixed effect factor.
//! - Cyclic alternating projections for D >= 2 fixed effect factors (e.g. entity + time).
//! - Numerical inversion and projected OLS estimation via `faer`.
//! - Invariance and collinearity detection with Gojo statistical diagnostics (S0101).
//! - Classical (IID), Robust (HC1 / White), and Cluster-Robust (Liang & Zeger / Cameron & Miller)
//!   covariance matrices with small-sample and absorbed degrees-of-freedom adjustments.
//! - Asymptotic Student-t inference (p-values) with statrs.
//! - `FeolsResult` struct packaging with full NEKO contract (`summary`, `tidy`, `glance`,
//!   `vcov`, `coef`, `residuals`).

use std::sync::Arc;
use std::collections::{BTreeMap, HashMap};
use faer::prelude::*;
use faer::Mat;
use polars_core::prelude::*;
use statrs::distribution::{ContinuousCDF, StudentsT, Normal};
use ghl_diagnostics::Diagnostic;

use crate::value::{Value, FormulaParts};
use crate::vector_data::VectorData;
use crate::polars_bridge::pull_column_as_values;

// ---------------------------------------------------------------------------
// 1. Group-mean Demeaning Algorithms (Method of Alternating Projections)
// ---------------------------------------------------------------------------

/// Demeans a single numeric vector `vec` across `D` fixed effect factor dimensions.
///
/// - For D = 1: Single-pass exact group-mean subtraction in O(N) time and O(G) memory.
/// - For D >= 2: Method of Alternating Projections (von Neumann-Halperin cyclic projection)
///   iteratively centering across factor dimensions until convergence (max mean < 1e-9).
pub fn demean_vector(
    vec: &[f64],
    groups_per_dim: &[Vec<usize>],
    n_groups_per_dim: &[usize],
) -> Vec<f64> {
    let n = vec.len();
    let d = groups_per_dim.len();
    if d == 0 || n == 0 {
        return vec.to_vec();
    }

    if d == 1 {
        // Fast path: Exact 1-pass demeaning
        let groups = &groups_per_dim[0];
        let num_groups = n_groups_per_dim[0];
        let mut sums = vec![0.0; num_groups];
        let mut counts = vec![0usize; num_groups];

        for i in 0..n {
            let g = groups[i];
            sums[g] += vec[i];
            counts[g] += 1;
        }

        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let g = groups[i];
            let mean = if counts[g] > 0 { sums[g] / (counts[g] as f64) } else { 0.0 };
            out.push(vec[i] - mean);
        }
        return out;
    }

    // Multi-way fixed effects (D >= 2): Method of Alternating Projections (MAP)
    let mut cur = vec.to_vec();
    let max_iter = 1000;
    let tol = 1e-9;

    for _iter in 0..max_iter {
        let mut max_abs_mean: f64 = 0.0;

        for dim in 0..d {
            let groups = &groups_per_dim[dim];
            let num_groups = n_groups_per_dim[dim];
            let mut sums = vec![0.0; num_groups];
            let mut counts = vec![0usize; num_groups];

            for i in 0..n {
                let g = groups[i];
                sums[g] += cur[i];
                counts[g] += 1;
            }

            for i in 0..n {
                let g = groups[i];
                if counts[g] > 0 {
                    let mean = sums[g] / (counts[g] as f64);
                    cur[i] -= mean;
                    let abs_m = mean.abs();
                    if abs_m > max_abs_mean {
                        max_abs_mean = abs_m;
                    }
                }
            }
        }

        if max_abs_mean < tol {
            break;
        }
    }

    cur
}

// ---------------------------------------------------------------------------
// 2. Helper Functions: Key Mapping & Value Conversion
// ---------------------------------------------------------------------------

fn value_to_group_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::I64(i) => i.to_string(),
        Value::F64(f) => format!("{:.8}", f),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn value_to_f64_opt(v: &Value) -> Option<f64> {
    match v {
        Value::F64(f) if !f.is_nan() => Some(*f),
        Value::I64(i) => Some(*i as f64),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 3. High-Dimensional Fixed Effects Estimator Core
// ---------------------------------------------------------------------------

/// Specification of variance-covariance estimator for `feols`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeolsVcovSpec {
    Classical,
    Robust,
    Clustered(String),
}

/// Estimates High-Dimensional Fixed Effects OLS on a DataFrame.
pub fn fit_feols(
    formula_parts: &FormulaParts,
    frame: &DataFrame,
    na_reasons: &crate::na_reasons::NaReasonTable,
    vcov_spec: FeolsVcovSpec,
) -> Result<Value, Diagnostic> {
    // 1. Validate formula structure
    if !formula_parts.has_fixed_effects() || formula_parts.absorbed.is_empty() {
        return Err(Diagnostic::statistical_error(
            "S0100",
            "Formula for `feols()` must specify at least one fixed effect factor after `|` (e.g. `y ~ x1 + x2 | entity + time`)",
        ).with_help("Use `|` to separate regressors from fixed effect factors: `response ~ predictors | fixed_effects`"));
    }

    let y_col = &formula_parts.response;
    let x_cols = &formula_parts.terms;
    let fe_cols = &formula_parts.absorbed;

    if x_cols.is_empty() {
        return Err(Diagnostic::statistical_error(
            "S0100",
            "`feols()` requires at least one predictor variable (e.g., `y ~ x1 | entity`)",
        ));
    }

    let k = x_cols.len();
    let d = fe_cols.len();

    // Determine cluster column name (if clustered)
    let cluster_col_opt = match &vcov_spec {
        FeolsVcovSpec::Clustered(c) => Some(c.clone()),
        _ => None,
    };

    // 2. Extract columns from DataFrame
    let y_vals = pull_column_as_values(frame, na_reasons, y_col)?;
    let mut x_vals = Vec::with_capacity(k);
    for term in x_cols {
        x_vals.push(pull_column_as_values(frame, na_reasons, term)?);
    }
    let mut fe_vals = Vec::with_capacity(d);
    for fe in fe_cols {
        fe_vals.push(pull_column_as_values(frame, na_reasons, fe)?);
    }
    let cluster_vals = if let Some(ref c_col) = cluster_col_opt {
        Some(pull_column_as_values(frame, na_reasons, c_col)?)
    } else {
        None
    };

    let total_rows = y_vals.len();

    // 3. Listwise deletion of missing values (NAs and NaNs)
    let mut valid_rows = Vec::new();
    for i in 0..total_rows {
        if value_to_f64_opt(&y_vals[i]).is_none() {
            continue;
        }

        let mut x_valid = true;
        for j in 0..k {
            if value_to_f64_opt(&x_vals[j][i]).is_none() {
                x_valid = false;
                break;
            }
        }
        if !x_valid {
            continue;
        }

        let mut fe_valid = true;
        for dim in 0..d {
            if fe_vals[dim][i].is_na() {
                fe_valid = false;
                break;
            }
        }
        if !fe_valid {
            continue;
        }

        if let Some(ref c_vals) = cluster_vals {
            if c_vals[i].is_na() {
                continue;
            }
        }

        valid_rows.push(i);
    }

    let n = valid_rows.len();
    if n == 0 {
        return Err(Diagnostic::statistical_error(
            "S0101",
            "No valid observations remaining after listwise deletion of missing values",
        ));
    }
    if n <= k {
        return Err(Diagnostic::statistical_error(
            "S0101",
            format!("Sample size N={n} must be strictly greater than number of predictors k={k}"),
        ));
    }

    // 4. Assemble clean numeric vectors and factor group maps
    let mut y = Vec::with_capacity(n);
    let mut x_vectors = vec![Vec::with_capacity(n); k];

    for &r in &valid_rows {
        y.push(value_to_f64_opt(&y_vals[r]).unwrap());
        for j in 0..k {
            x_vectors[j].push(value_to_f64_opt(&x_vals[j][r]).unwrap());
        }
    }

    // Map each fixed effect factor to integer group IDs [0..G_d)
    let mut groups_per_dim = Vec::with_capacity(d);
    let mut n_groups_per_dim = Vec::with_capacity(d);

    for dim in 0..d {
        let mut map: HashMap<String, usize> = HashMap::new();
        let mut g_ids = Vec::with_capacity(n);
        for &r in &valid_rows {
            let key = value_to_group_key(&fe_vals[dim][r]);
            let next_id = map.len();
            let gid = *map.entry(key).or_insert(next_id);
            g_ids.push(gid);
        }
        n_groups_per_dim.push(map.len());
        groups_per_dim.push(g_ids);
    }

    // Map cluster column (if applicable)
    let (cluster_ids, n_clusters) = if let Some(ref c_vals) = cluster_vals {
        let mut map: HashMap<String, usize> = HashMap::new();
        let mut c_ids = Vec::with_capacity(n);
        for &r in &valid_rows {
            let key = value_to_group_key(&c_vals[r]);
            let next_id = map.len();
            let cid = *map.entry(key).or_insert(next_id);
            c_ids.push(cid);
        }
        let total_clusters = map.len();
        (Some(c_ids), total_clusters)
    } else {
        (None, 0)
    };

    // 5. Degrees of Freedom Calculation
    // For D = 1: df_FE = G_0
    // For D >= 2: df_FE = G_0 + sum_{d=1}^{D-1} (G_d - 1)
    let df_fe: usize = if d == 1 {
        n_groups_per_dim[0]
    } else {
        let mut sum = n_groups_per_dim[0];
        for dim in 1..d {
            sum += if n_groups_per_dim[dim] > 1 { n_groups_per_dim[dim] - 1 } else { 0 };
        }
        sum
    };

    let df_resid_signed = (n as i64) - (k as i64) - (df_fe as i64);
    if df_resid_signed <= 0 {
        return Err(Diagnostic::statistical_error(
            "S0101",
            format!(
                "Residual degrees of freedom ({df_resid_signed}) must be strictly positive: sample size N={n} is too small for k={k} predictors and df_FE={df_fe} absorbed fixed effects"
            ),
        ).with_help("Increase sample size or reduce the number of fixed effects categories."));
    }
    let df_resid = df_resid_signed as usize;
    let df_resid_f64 = df_resid as f64;

    // 6. Demeaning via Method of Alternating Projections
    let demeaned_y = demean_vector(&y, &groups_per_dim, &n_groups_per_dim);

    let mut demeaned_x = Vec::with_capacity(k);
    for j in 0..k {
        let dx = demean_vector(&x_vectors[j], &groups_per_dim, &n_groups_per_dim);

        // Check for within-group invariance (zero variance after absorption)
        let ss_x: f64 = dx.iter().map(|&val| val * val).sum();
        if ss_x < 1e-12 {
            return Err(Diagnostic::statistical_error(
                "S0101",
                format!(
                    "Predictor variable `{}` has zero within-variation after absorbing fixed effects: it is completely invariant within groups",
                    x_cols[j]
                ),
            ).with_help("Remove invariant predictors or remove the absorbing fixed effects that collinearize with them."));
        }

        demeaned_x.push(dx);
    }

    // 7. Normal Equations Assembly: A = X'X (k x k), b = X'y (k x 1)
    let mut a_flat = vec![0.0; k * k];
    let mut b_vec = vec![0.0; k];

    for i in 0..n {
        let yi = demeaned_y[i];
        for r in 0..k {
            let xr = demeaned_x[r][i];
            b_vec[r] += xr * yi;
            for c in 0..k {
                a_flat[r * k + c] += xr * demeaned_x[c][i];
            }
        }
    }

    // 8. Solve Projected OLS System via faer LU decomposition
    let faer_a = Mat::from_fn(k, k, |r, c| a_flat[r * k + c]);
    let lu = faer_a.partial_piv_lu();

    // Check singularity on LU diagonal
    let u = lu.U();
    let min_pivot = (0..k).map(|i| u[(i, i)].abs()).fold(f64::INFINITY, f64::min);
    if min_pivot < 1e-12 {
        return Err(Diagnostic::statistical_error(
            "S0101",
            "Predictor variables are collinear after absorbing fixed effects: matrix (X'X) is singular and cannot be inverted",
        ).with_help("Check for collinear predictors or redundant interaction terms."));
    }

    let faer_b = Mat::from_fn(k, 1, |r, _| b_vec[r]);
    let faer_beta = lu.solve(&faer_b);
    let beta: Vec<f64> = (0..k).map(|r| faer_beta[(r, 0)]).collect();

    // Bread matrix Q = (X'X)^-1
    let ident_k = Mat::from_fn(k, k, |r, c| if r == c { 1.0 } else { 0.0 });
    let q_mat = lu.solve(&ident_k);

    // 9. Compute Residuals, SSR, TSS, R-Squared
    let mut residuals = Vec::with_capacity(n);
    let mut ssr = 0.0;
    let mut tss_within = 0.0;

    for i in 0..n {
        let mut pred = 0.0;
        for j in 0..k {
            pred += demeaned_x[j][i] * beta[j];
        }
        let res = demeaned_y[i] - pred;
        residuals.push(res);
        ssr += res * res;
        tss_within += demeaned_y[i] * demeaned_y[i];
    }

    let y_mean = y.iter().sum::<f64>() / (n as f64);
    let tss_overall = y.iter().map(|&yi| (yi - y_mean).powi(2)).sum::<f64>();

    let r2_within = if tss_within > 1e-15 {
        (1.0 - ssr / tss_within).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let r2_overall = if tss_overall > 1e-15 {
        (1.0 - ssr / tss_overall).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let mut fitted = Vec::with_capacity(n);
    for i in 0..n {
        fitted.push(y[i] - residuals[i]);
    }

    // 10. Compute Covariance Matrix V(beta)
    let (vcov_faer, vcov_desc) = match &vcov_spec {
        FeolsVcovSpec::Classical => {
            let sigma2 = ssr / df_resid_f64;
            let v = &q_mat * sigma2;
            (v, "Classical (IID)".to_string())
        }
        FeolsVcovSpec::Robust => {
            let mut m_mat = Mat::<f64>::zeros(k, k);
            for i in 0..n {
                let res2 = residuals[i] * residuals[i];
                for r in 0..k {
                    let xr = demeaned_x[r][i];
                    for c in 0..k {
                        m_mat[(r, c)] += res2 * xr * demeaned_x[c][i];
                    }
                }
            }
            let factor = (n as f64) / df_resid_f64;
            let v = &q_mat * (&m_mat * &q_mat) * factor;
            (v, "Robust (HC1)".to_string())
        }
        FeolsVcovSpec::Clustered(c_name) => {
            let c_ids = cluster_ids.as_ref().unwrap();
            let mut u_clusters = vec![vec![0.0; k]; n_clusters];

            for i in 0..n {
                let c = c_ids[i];
                let res = residuals[i];
                for j in 0..k {
                    u_clusters[c][j] += res * demeaned_x[j][i];
                }
            }

            let mut m_mat = Mat::<f64>::zeros(k, k);
            for c in 0..n_clusters {
                let u_c = &u_clusters[c];
                for r in 0..k {
                    for col in 0..k {
                        m_mat[(r, col)] += u_c[r] * u_c[col];
                    }
                }
            }

            let g_f64 = n_clusters as f64;
            let factor = if n_clusters > 1 {
                (g_f64 / (g_f64 - 1.0)) * (((n - 1) as f64) / df_resid_f64)
            } else {
                (n as f64) / df_resid_f64
            };

            let v = &q_mat * (&m_mat * &q_mat) * factor;
            (v, format!("Clustered by `{}` ({} clusters)", c_name, n_clusters))
        }
    };

    // 11. Compute Standard Errors, t-statistics, and Student-t p-values
    let mut std_errors = Vec::with_capacity(k);
    let mut t_stats = Vec::with_capacity(k);
    let mut p_values = Vec::with_capacity(k);

    let t_dist = StudentsT::new(0.0, 1.0, df_resid_f64).ok();

    for j in 0..k {
        let var_j = vcov_faer[(j, j)];
        let se = if var_j > 0.0 { var_j.sqrt() } else { 0.0 };
        let t = if se > 1e-15 { beta[j] / se } else { 0.0 };
        let p = if let Some(ref dist) = t_dist {
            let cdf = dist.cdf(t.abs());
            2.0 * (1.0 - cdf).clamp(0.0, 1.0)
        } else {
            let norm = Normal::new(0.0, 1.0).unwrap();
            2.0 * (1.0 - norm.cdf(t.abs())).clamp(0.0, 1.0)
        };
        std_errors.push(se);
        t_stats.push(t);
        p_values.push(p);
    }

    // Flatten covariance matrix
    let mut flat_vcov = Vec::with_capacity(k * k);
    for r in 0..k {
        for c in 0..k {
            flat_vcov.push(vcov_faer[(r, c)]);
        }
    }

    // 12. Build Parameters DataFrame
    let param_columns: Vec<(String, Vec<Value>)> = vec![
        ("term".to_string(), x_cols.iter().map(|s| Value::String(s.clone())).collect()),
        ("estimate".to_string(), beta.iter().map(|&x| Value::F64(x)).collect()),
        ("std_error".to_string(), std_errors.iter().map(|&x| Value::F64(x)).collect()),
        ("statistic".to_string(), t_stats.iter().map(|&x| Value::F64(x)).collect()),
        ("p_value".to_string(), p_values.iter().map(|&x| Value::F64(x)).collect()),
    ];
    let (param_frame, param_na) = crate::polars_bridge::build_dataframe(&param_columns)?;
    let params_df = Value::DataFrame {
        frame: param_frame,
        na_reasons: param_na,
    };

    // 13. Package FeolsResult struct
    let mut fields = BTreeMap::new();
    fields.insert("model".to_string(), Value::String("feols".to_string()));
    fields.insert("response".to_string(), Value::String(y_col.clone()));
    fields.insert(
        "terms".to_string(),
        Value::Vector(VectorData::from_values(
            x_cols.iter().map(|s| Value::String(s.clone())).collect(),
        )),
    );
    fields.insert(
        "absorbed".to_string(),
        Value::Vector(VectorData::from_values(
            fe_cols.iter().map(|s| Value::String(s.clone())).collect(),
        )),
    );
    fields.insert("coefficients".to_string(), Value::Vector(VectorData::from_f64(beta)));
    fields.insert("std_errors".to_string(), Value::Vector(VectorData::from_f64(std_errors)));
    fields.insert("t_stats".to_string(), Value::Vector(VectorData::from_f64(t_stats)));
    fields.insert("p_values".to_string(), Value::Vector(VectorData::from_f64(p_values)));
    fields.insert("r2".to_string(), Value::F64(r2_overall));
    fields.insert("r2_within".to_string(), Value::F64(r2_within));
    fields.insert("n_obs".to_string(), Value::I64(n as i64));
    fields.insert("df_fe".to_string(), Value::I64(df_fe as i64));
    fields.insert("df_resid".to_string(), Value::I64(df_resid_signed));
    fields.insert("vcov_type".to_string(), Value::String(vcov_desc));
    fields.insert(
        "cluster_var".to_string(),
        match cluster_col_opt {
            Some(c) => Value::String(c),
            None => Value::Unit,
        },
    );
    fields.insert("parameters".to_string(), params_df);
    fields.insert("residuals".to_string(), Value::Vector(VectorData::from_f64(residuals)));
    fields.insert("fitted".to_string(), Value::Vector(VectorData::from_f64(fitted)));
    fields.insert(
        "vcov".to_string(),
        Value::Matrix {
            rows: k,
            cols: k,
            data: Arc::new(flat_vcov),
        },
    );

    Ok(Value::Struct {
        name: "FeolsResult".to_string(),
        fields: Arc::new(fields),
    })
}

// ---------------------------------------------------------------------------
// 4. Native Function Interface (`feols(formula, df, [cluster], [vcov_type])`)
// ---------------------------------------------------------------------------

/// Native GHL function `feols(formula, df, [vcov_opt], [cluster_opt])`.
pub fn native_feols(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`feols()` requires at least 2 arguments: `feols(formula, df, [cluster])`",
        ));
    }

    let formula_parts = match &args[0] {
        Value::Formula { response, parts, .. } => FormulaParts::new(response.clone(), parts.clone()),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `feols()` must be a Formula, found `{}`", other.type_name()),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `feols()` must be a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };

    // Determine vcov specification
    // Default: Clustered by first absorbed fixed effect factor (matching fixest::feols)
    let mut vcov_spec = if formula_parts.has_fixed_effects() && !formula_parts.absorbed.is_empty() {
        FeolsVcovSpec::Clustered(formula_parts.absorbed[0].clone())
    } else {
        FeolsVcovSpec::Classical
    };

    if args.len() >= 3 {
        match &args[2] {
            Value::String(s) => {
                let lower = s.to_lowercase();
                match lower.as_str() {
                    "classical" | "iid" => vcov_spec = FeolsVcovSpec::Classical,
                    "robust" | "hc1" => vcov_spec = FeolsVcovSpec::Robust,
                    "cluster" => {
                        if formula_parts.has_fixed_effects() && !formula_parts.absorbed.is_empty() {
                            vcov_spec = FeolsVcovSpec::Clustered(formula_parts.absorbed[0].clone());
                        }
                    }
                    custom_col => {
                        vcov_spec = FeolsVcovSpec::Clustered(custom_col.to_string());
                    }
                }
            }
            Value::Unit | Value::NA(_) => {}
            other => {
                return Err(Diagnostic::statistical_error(
                    "S0200",
                    format!("Third argument of `feols()` (vcov / cluster option) must be a String, found `{}`", other.type_name()),
                ));
            }
        }
    }

    if args.len() >= 4 {
        if let Value::String(s) = &args[3] {
            let lower = s.to_lowercase();
            if lower == "classical" || lower == "iid" {
                vcov_spec = FeolsVcovSpec::Classical;
            } else if lower == "robust" || lower == "hc1" {
                vcov_spec = FeolsVcovSpec::Robust;
            }
        }
    }

    fit_feols(&formula_parts, frame, na_reasons, vcov_spec)
}
