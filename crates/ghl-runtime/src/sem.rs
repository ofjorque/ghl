//! Structural Equation Modeling (SEM / CFA) Core Engine (RFC 15 & Roadmap 08).
//!
//! Provides the mathematical Layer 0 for SEM:
//! - Sample covariance matrix $S$ computation from DataFrames.
//! - Reticular Action Model (RAM) matrix representation: directed paths $A$,
//!   symmetric covariances/variances $S_{ram}$, and filter matrix $F$.
//! - Model-implied covariance matrix $\Sigma(\theta) = F (I - A)^{-1} S_{ram} (I - A)^{-T} F^T$.
//! - Wishart Maximum Likelihood discrepancy function:
//!   $F_{ML}(\theta) = \ln|\Sigma(\theta)| + \text{tr}(S \Sigma^{-1}(\theta)) - \ln|S| - p$.
//! - Numerical optimization via BFGS Quasi-Newton.
//! - Numerical Hessian inversion for parameter standard errors, z-statistics, and p-values.
//! - Baseline independence model evaluation for comparative fit indices.

use std::sync::Arc;
use std::collections::{BTreeMap, HashSet};
use faer::prelude::*;
use faer::Mat;
use polars_core::prelude::*;
use statrs::distribution::{ContinuousCDF, Normal, ChiSquared};
use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::FormulaOp;
use crate::value::Value;
use crate::optim::minimize_bfgs;

// ---------------------------------------------------------------------------
// 1. Sample Covariance Matrix Computation
// ---------------------------------------------------------------------------

/// Computes the sample covariance matrix $S$ for a list of column names in a DataFrame.
/// Performs listwise deletion of missing (NA / NaN) values.
/// Returns (N, col_names, flat_cov_matrix_row_major, log_det_S).
pub fn compute_sample_covariance(
    df: &DataFrame,
    columns: &[String],
) -> Result<(usize, Vec<String>, Vec<f64>, f64), Diagnostic> {
    let p = columns.len();
    if p == 0 {
        return Err(Diagnostic::statistical_error(
            "S0200",
            "`sample_cov()` requires at least one column",
        ));
    }

    // Extract columns and verify presence
    let mut series_vec = Vec::with_capacity(p);
    for col in columns {
        let s = df.column(col).map_err(|_| {
            Diagnostic::statistical_error(
                "S0200",
                format!("Variable `{col}` in SEM specification not found in DataFrame"),
            )
        })?;
        let ca = s.cast(&DataType::Float64).map_err(|e| {
            Diagnostic::compute_error("C0202", format!("Failed to cast column `{col}` to Float64: {e}"))
        })?;
        series_vec.push(ca);
    }

    let n_rows = df.height();
    let mut valid_rows = Vec::new();

    // Listwise missing-value filtering
    for i in 0..n_rows {
        let mut row_valid = true;
        for s in &series_vec {
            let ca = s.f64().unwrap();
            match ca.get(i) {
                Some(val) if !val.is_nan() => {}
                _ => {
                    row_valid = false;
                    break;
                }
            }
        }
        if row_valid {
            valid_rows.push(i);
        }
    }

    let n = valid_rows.len();
    if n <= p {
        return Err(Diagnostic::statistical_error(
            "S0101",
            format!(
                "Sample size N={n} must be strictly greater than the number of observed variables p={p} for SEM estimation"
            ),
        ));
    }

    // Compute column means
    let mut means = vec![0.0; p];
    for (j, s) in series_vec.iter().enumerate() {
        let ca = s.f64().unwrap();
        let mut sum = 0.0;
        for &r in &valid_rows {
            sum += ca.get(r).unwrap();
        }
        means[j] = sum / (n as f64);
    }

    // Compute sample covariance matrix S = (1/(N-1)) sum (x_i - mean)(x_i - mean)^T
    let mut s_mat = vec![0.0; p * p];
    for &r in &valid_rows {
        for j in 0..p {
            let val_j = series_vec[j].f64().unwrap().get(r).unwrap() - means[j];
            for k in 0..p {
                let val_k = series_vec[k].f64().unwrap().get(r).unwrap() - means[k];
                s_mat[j * p + k] += val_j * val_k;
            }
        }
    }

    let denom = (n - 1) as f64;
    for slot in s_mat.iter_mut() {
        *slot /= denom;
    }

    // Check positive-definiteness via Cholesky decomposition of S
    let faer_s = Mat::from_fn(p, p, |i, j| s_mat[i * p + j]);
    let llt = faer_s.llt(faer::Side::Lower).map_err(|_| {
        Diagnostic::statistical_error(
            "S0101",
            "Sample covariance matrix S is not positive-definite. Observed variables may be collinear.",
        )
    })?;

    let l = llt.L();
    let mut log_det_s = 0.0;
    for i in 0..p {
        let diag = l[(i, i)];
        if diag <= 0.0 || diag.is_nan() {
            return Err(Diagnostic::statistical_error(
                "S0101",
                "Singular or near-singular sample covariance matrix detected in SEM estimation",
            ));
        }
        log_det_s += 2.0 * diag.ln();
    }

    Ok((n, columns.to_vec(), s_mat, log_det_s))
}

// ---------------------------------------------------------------------------
// 2. SEM Model Parser & Reticular Action Model (RAM)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParamSpec {
    pub name: String,
    pub lhs: String,
    pub op: String,
    pub rhs: String,
    pub matrix: RamMatrix,
    pub row: usize,
    pub col: usize,
    pub fixed_value: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RamMatrix {
    A, // Directed paths (loadings and regressions)
    S, // Symmetric covariances and variances
}

#[derive(Debug, Clone)]
pub struct ParsedSemModel {
    pub observed_names: Vec<String>,
    pub latent_names: Vec<String>,
    pub all_variables: Vec<String>,
    pub fixed_params: Vec<ParamSpec>,
    pub free_params: Vec<ParamSpec>,
}

impl ParsedSemModel {
    pub fn parse(equations: &[Value]) -> Result<Self, Diagnostic> {
        let mut latent_set = Vec::new();
        let mut observed_set = Vec::new();

        // 1. First pass: identify latent variables (LHS of `=~`)
        for eq in equations {
            if let Value::Formula { op: FormulaOp::Measurement, response, .. } = eq {
                if !latent_set.contains(response) {
                    latent_set.push(response.clone());
                }
            }
        }

        // 2. Second pass: identify observed variables
        for eq in equations {
            match eq {
                Value::Formula { op: FormulaOp::Measurement, terms, .. } => {
                    for term in terms {
                        if !latent_set.contains(term) && !observed_set.contains(term) {
                            observed_set.push(term.clone());
                        }
                    }
                }
                Value::Formula { op: FormulaOp::Regression, response, terms, .. } => {
                    if !latent_set.contains(response) && !observed_set.contains(response) {
                        observed_set.push(response.clone());
                    }
                    for term in terms {
                        if !latent_set.contains(term) && !observed_set.contains(term) {
                            observed_set.push(term.clone());
                        }
                    }
                }
                Value::Formula { op: FormulaOp::Covariance, response, terms, .. } => {
                    if !latent_set.contains(response) && !observed_set.contains(response) {
                        observed_set.push(response.clone());
                    }
                    for term in terms {
                        if !latent_set.contains(term) && !observed_set.contains(term) {
                            observed_set.push(term.clone());
                        }
                    }
                }
                other => {
                    return Err(Diagnostic::statistical_error(
                        "S0200",
                        format!("SEM specification requires formula equations, found `{}`", other.type_name()),
                    ));
                }
            }
        }

        if observed_set.is_empty() {
            return Err(Diagnostic::statistical_error(
                "S0200",
                "SEM specification has no observed indicator variables",
            ));
        }

        let p = observed_set.len();
        let m = latent_set.len();
        let mut all_variables = Vec::with_capacity(p + m);
        all_variables.extend(observed_set.clone());
        all_variables.extend(latent_set.clone());

        let var_index = |name: &str| -> Result<usize, Diagnostic> {
            all_variables
                .iter()
                .position(|v| v == name)
                .ok_or_else(|| Diagnostic::statistical_error("S0200", format!("Unknown variable `{name}` in SEM spec")))
        };

        let mut fixed_params = Vec::new();
        let mut free_params = Vec::new();
        let mut explicit_variances = HashSet::new();

        // 3. Process equations
        for eq in equations {
            match eq {
                Value::Formula { op: FormulaOp::Measurement, response: latent, terms: indicators, .. } => {
                    let latent_idx = var_index(latent)?;
                    for (i, ind) in indicators.iter().enumerate() {
                        let ind_idx = var_index(ind)?;
                        let name = format!("{latent} =~ {ind}");
                        if i == 0 {
                            // Marker variable constraint: fix first indicator loading to 1.0
                            fixed_params.push(ParamSpec {
                                name,
                                lhs: latent.clone(),
                                op: "=~".to_string(),
                                rhs: ind.clone(),
                                matrix: RamMatrix::A,
                                row: ind_idx,
                                col: latent_idx,
                                fixed_value: Some(1.0),
                            });
                        } else {
                            // Free factor loading
                            free_params.push(ParamSpec {
                                name,
                                lhs: latent.clone(),
                                op: "=~".to_string(),
                                rhs: ind.clone(),
                                matrix: RamMatrix::A,
                                row: ind_idx,
                                col: latent_idx,
                                fixed_value: None,
                            });
                        }
                    }
                }
                Value::Formula { op: FormulaOp::Regression, response: lhs, terms: rhs_list, .. } => {
                    let lhs_idx = var_index(lhs)?;
                    for rhs in rhs_list {
                        let rhs_idx = var_index(rhs)?;
                        let name = format!("{lhs} ~ {rhs}");
                        free_params.push(ParamSpec {
                            name,
                            lhs: lhs.clone(),
                            op: "~".to_string(),
                            rhs: rhs.clone(),
                            matrix: RamMatrix::A,
                            row: lhs_idx,
                            col: rhs_idx,
                            fixed_value: None,
                        });
                    }
                }
                Value::Formula { op: FormulaOp::Covariance, response: v1, terms: v2_list, .. } => {
                    let v1_idx = var_index(v1)?;
                    for v2 in v2_list {
                        let v2_idx = var_index(v2)?;
                        let name = format!("{v1} ~~ {v2}");
                        if v1 == v2 {
                            explicit_variances.insert(v1.clone());
                        }
                        free_params.push(ParamSpec {
                            name,
                            lhs: v1.clone(),
                            op: "~~".to_string(),
                            rhs: v2.clone(),
                            matrix: RamMatrix::S,
                            row: v1_idx,
                            col: v2_idx,
                            fixed_value: None,
                        });
                    }
                }
                _ => {}
            }
        }

        // 4. Standard default variances:
        // (a) Unique residual variance for each observed variable if not explicitly declared
        for ind in &observed_set {
            if !explicit_variances.contains(ind) {
                let idx = var_index(ind)?;
                free_params.push(ParamSpec {
                    name: format!("{ind} ~~ {ind}"),
                    lhs: ind.clone(),
                    op: "~~".to_string(),
                    rhs: ind.clone(),
                    matrix: RamMatrix::S,
                    row: idx,
                    col: idx,
                    fixed_value: None,
                });
            }
        }

        // (b) Variance for each exogenous latent variable if not explicitly declared
        for latent in &latent_set {
            if !explicit_variances.contains(latent) {
                let idx = var_index(latent)?;
                free_params.push(ParamSpec {
                    name: format!("{latent} ~~ {latent}"),
                    lhs: latent.clone(),
                    op: "~~".to_string(),
                    rhs: latent.clone(),
                    matrix: RamMatrix::S,
                    row: idx,
                    col: idx,
                    fixed_value: None,
                });
            }
        }

        Ok(Self {
            observed_names: observed_set,
            latent_names: latent_set,
            all_variables,
            fixed_params,
            free_params,
        })
    }

    /// Number of observed variables p.
    pub fn p(&self) -> usize {
        self.observed_names.len()
    }

    /// Total variables k = p + m.
    pub fn k(&self) -> usize {
        self.all_variables.len()
    }

    /// Number of distinct elements in sample covariance: p*(p+1)/2.
    pub fn distinct_cov_elements(&self) -> usize {
        let p = self.p();
        p * (p + 1) / 2
    }

    /// Degrees of freedom df = p*(p+1)/2 - q.
    pub fn degrees_of_freedom(&self) -> i64 {
        self.distinct_cov_elements() as i64 - self.free_params.len() as i64
    }
}

// ---------------------------------------------------------------------------
// 3. Wishart ML Discrepancy Function
// ---------------------------------------------------------------------------

/// Computes the model-implied covariance matrix $\Sigma(\theta)$ of observed variables.
pub fn compute_model_implied_covariance(
    model: &ParsedSemModel,
    theta: &[f64],
) -> Result<Vec<f64>, Diagnostic> {
    let k = model.k();
    let p = model.p();

    let mut mat_a = vec![0.0; k * k];
    let mut mat_s = vec![0.0; k * k];

    // Fixed parameters
    for param in &model.fixed_params {
        if let Some(val) = param.fixed_value {
            match param.matrix {
                RamMatrix::A => mat_a[param.row * k + param.col] = val,
                RamMatrix::S => {
                    mat_s[param.row * k + param.col] = val;
                    mat_s[param.col * k + param.row] = val;
                }
            }
        }
    }

    // Free parameters
    for (i, param) in model.free_params.iter().enumerate() {
        let val = theta[i];
        match param.matrix {
            RamMatrix::A => mat_a[param.row * k + param.col] = val,
            RamMatrix::S => {
                mat_s[param.row * k + param.col] = val;
                mat_s[param.col * k + param.row] = val;
            }
        }
    }

    // Compute (I_k - A)
    let mut i_minus_a = vec![0.0; k * k];
    for r in 0..k {
        for c in 0..k {
            let ident = if r == c { 1.0 } else { 0.0 };
            i_minus_a[r * k + c] = ident - mat_a[r * k + c];
        }
    }

    let faer_ima = Mat::from_fn(k, k, |i, j| i_minus_a[i * k + j]);
    let lu = faer_ima.partial_piv_lu();
    let min_pivot = (0..k).map(|i| lu.U()[(i, i)].abs()).fold(f64::INFINITY, f64::min);
    if min_pivot < 1e-12 {
        return Err(Diagnostic::statistical_error(
            "S0101",
            "Non-invertible (I - A) matrix in SEM model. System contains cyclical non-recursive singularity.",
        ));
    }

    // M = (I_k - A)^{-1} by solving (I - A) M = I_k
    let ident_k = Mat::from_fn(k, k, |i, j| if i == j { 1.0 } else { 0.0 });
    let m_mat = lu.solve(&ident_k);

    // V = M * S_ram * M^T
    let faer_s = Mat::from_fn(k, k, |i, j| mat_s[i * k + j]);
    let ms = &m_mat * &faer_s;
    let m_trans = m_mat.transpose();
    let v_mat = &ms * &m_trans;

    // Extract top-left p x p block (observed covariance Sigma)
    let mut sigma = vec![0.0; p * p];
    for r in 0..p {
        for c in 0..p {
            // Enforce exact numerical symmetry
            let val = 0.5 * (v_mat[(r, c)] + v_mat[(c, r)]);
            sigma[r * p + c] = val;
        }
    }

    Ok(sigma)
}

/// Evaluates Wishart ML discrepancy $F_{ML}(\theta) = \ln|\Sigma(\theta)| + \text{tr}(S \Sigma^{-1}) - \ln|S| - p$.
pub fn evaluate_wishart_f_ml(
    model: &ParsedSemModel,
    s_mat: &[f64],
    log_det_s: f64,
    theta: &[f64],
) -> f64 {
    let p = model.p();

    // Heavy penalty for non-positive variance parameters
    let mut penalty = 0.0;
    for (i, param) in model.free_params.iter().enumerate() {
        if param.matrix == RamMatrix::S && param.row == param.col {
            if theta[i] <= 1e-5 {
                let diff = 1e-5 - theta[i];
                penalty += 10000.0 * diff * diff;
            }
        }
    }

    let sigma = match compute_model_implied_covariance(model, theta) {
        Ok(sig) => sig,
        Err(_) => return 1e9 + penalty,
    };

    let faer_sigma = Mat::from_fn(p, p, |i, j| sigma[i * p + j]);

    // Check positive-definiteness via Cholesky decomposition of Sigma
    let llt = match faer_sigma.llt(faer::Side::Lower) {
        Ok(llt) => llt,
        Err(_) => return 1e9 + penalty,
    };

    let l = llt.L();
    let mut log_det_sigma = 0.0;
    for i in 0..p {
        let diag = l[(i, i)];
        if diag <= 1e-8 || diag.is_nan() {
            return 1e9 + penalty;
        }
        log_det_sigma += 2.0 * diag.ln();
    }

    // Solve Sigma * Y = S -> Y = Sigma^{-1} S
    let faer_s = Mat::from_fn(p, p, |i, j| s_mat[i * p + j]);
    let y = llt.solve(&faer_s);

    // Compute trace(Y)
    let mut tr_y = 0.0;
    for i in 0..p {
        tr_y += y[(i, i)];
    }

    let f_val = log_det_sigma + tr_y - log_det_s - (p as f64) + penalty;
    if f_val.is_nan() {
        1e9
    } else if f_val < 0.0 {
        0.0
    } else {
        f_val
    }
}

// ---------------------------------------------------------------------------
// 4. SEM Model Fitting Execution
// ---------------------------------------------------------------------------

pub struct SemFitOutput {
    pub n_obs: usize,
    pub p: usize,
    pub q: usize,
    pub df: i64,
    pub f_min: f64,
    pub chisq: f64,
    pub p_value: f64,
    pub baseline_chisq: f64,
    pub baseline_df: i64,
    pub srmr: f64,
    pub parameters_df: DataFrame,
    pub sample_cov: Vec<f64>,
    pub implied_cov: Vec<f64>,
}

pub fn fit_sem_model(
    spec: &[Value],
    df: &DataFrame,
) -> Result<SemFitOutput, Diagnostic> {
    let model = ParsedSemModel::parse(spec)?;
    let p = model.p();
    let df_stat = model.degrees_of_freedom();

    if df_stat < 0 {
        return Err(Diagnostic::statistical_error(
            "S0201",
            format!(
                "Underidentified SEM model: degrees of freedom is negative (df = {df_stat} < 0). Free parameters ({}) exceed distinct covariance elements ({}).",
                model.free_params.len(),
                model.distinct_cov_elements()
            ),
        )
        .with_help("Fix factor loadings or constrain error variances to identify the model."));
    }

    // 1. Compute sample covariance matrix S
    let (n, _observed_names, s_mat, log_det_s) =
        compute_sample_covariance(df, &model.observed_names)?;

    // 2. Initial values for parameters
    let q = model.free_params.len();
    let mut init_theta = Vec::with_capacity(q);
    for param in &model.free_params {
        match param.matrix {
            RamMatrix::A => {
                // Factor loading or regression initial value
                let var_ind = s_mat[param.row * p + param.row];
                let var_marker = s_mat[0];
                let ratio = if var_marker > 0.0 { (var_ind / var_marker).sqrt() } else { 1.0 };
                init_theta.push(ratio.clamp(0.2, 5.0));
            }
            RamMatrix::S => {
                if param.row == param.col {
                    // Variance parameter initial value: half of sample variance
                    if param.row < p {
                        init_theta.push(0.5 * s_mat[param.row * p + param.row]);
                    } else {
                        // Latent variance: half of marker variable variance
                        init_theta.push(0.5 * s_mat[0]);
                    }
                } else {
                    // Covariance initial value: small non-zero
                    init_theta.push(0.0);
                }
            }
        }
    }

    // 3. Optimize Wishart ML discrepancy using BFGS
    let objective = |theta: &[f64]| -> f64 {
        evaluate_wishart_f_ml(&model, &s_mat, log_det_s, theta)
    };

    let opt_res = minimize_bfgs(objective, &init_theta, 1500, 1e-7);
    let hat_theta = opt_res.par;
    let f_min = opt_res.value;

    let chisq = ((n - 1) as f64) * f_min;
    let p_val = if df_stat > 0 {
        let chi_dist = ChiSquared::new(df_stat as f64).map_err(|e| {
            Diagnostic::compute_error("C0201", format!("Failed to create ChiSquared distribution: {e}"))
        })?;
        (1.0 - chi_dist.cdf(chisq)).clamp(0.0, 1.0)
    } else {
        1.0
    };

    // 4. Baseline independence model evaluation
    let mut baseline_log_det = 0.0;
    for i in 0..p {
        baseline_log_det += s_mat[i * p + i].ln();
    }
    let baseline_f = (baseline_log_det - log_det_s).max(0.0);
    let baseline_df = (p * (p - 1) / 2) as i64;
    let baseline_chisq = ((n - 1) as f64) * baseline_f;

    // 5. Model-implied covariance matrix at optimum
    let implied_cov = compute_model_implied_covariance(&model, &hat_theta)?;

    // 6. Compute SRMR (Standardized Root Mean Square Residual)
    let mut srmr_sq_sum = 0.0;
    for i in 0..p {
        let sd_s_i = s_mat[i * p + i].sqrt();
        let sd_imp_i = implied_cov[i * p + i].sqrt();
        for j in 0..=i {
            let sd_s_j = s_mat[j * p + j].sqrt();
            let sd_imp_j = implied_cov[j * p + j].sqrt();

            let corr_s = s_mat[i * p + j] / (sd_s_i * sd_s_j);
            let corr_imp = implied_cov[i * p + j] / (sd_imp_i * sd_imp_j);
            let diff = corr_s - corr_imp;
            srmr_sq_sum += diff * diff;
        }
    }
    let srmr = (2.0 * srmr_sq_sum / ((p * (p + 1)) as f64)).sqrt();

    // 7. Numerical Hessian inversion for standard errors
    let mut hessian = vec![0.0; q * q];
    for i in 0..q {
        let h_i = 1e-4 * (1.0 + hat_theta[i].abs());
        for j in 0..=i {
            let h_j = 1e-4 * (1.0 + hat_theta[j].abs());

            let mut pp = hat_theta.clone();
            pp[i] += h_i;
            pp[j] += h_j;
            let f_pp = objective(&pp);

            let mut pm = hat_theta.clone();
            pm[i] += h_i;
            pm[j] -= h_j;
            let f_pm = objective(&pm);

            let mut mp = hat_theta.clone();
            mp[i] -= h_i;
            mp[j] += h_j;
            let f_mp = objective(&mp);

            let mut mm = hat_theta.clone();
            mm[i] -= h_i;
            mm[j] -= h_j;
            let f_mm = objective(&mm);

            let val = (f_pp - f_pm - f_mp + f_mm) / (4.0 * h_i * h_j);
            hessian[i * q + j] = val;
            hessian[j * q + i] = val;
        }
    }

    // Asymptotic covariance: V(theta) = 2 / (N - 1) * H^{-1}
    // Add mild ridge damping on diagonal to prevent inversion singularity
    for i in 0..q {
        hessian[i * q + i] += 1e-8;
    }
    let faer_h = Mat::from_fn(q, q, |i, j| hessian[i * q + j]);
    let lu_h = faer_h.partial_piv_lu();
    let ident_q = Mat::from_fn(q, q, |i, j| if i == j { 1.0 } else { 0.0 });
    let h_inv = lu_h.solve(&ident_q);

    let scale_v = 2.0 / ((n - 1) as f64);
    let mut se_vec = Vec::with_capacity(q);
    let norm_dist = Normal::new(0.0, 1.0).unwrap();

    for i in 0..q {
        let var_i = (scale_v * h_inv[(i, i)]).max(1e-12);
        let se = var_i.sqrt();
        se_vec.push(se);
    }

    // 8. Assemble Parameters DataFrame (including fixed marker loadings)
    let mut col_term = Vec::new();
    let mut col_lhs = Vec::new();
    let mut col_op = Vec::new();
    let mut col_rhs = Vec::new();
    let mut col_est = Vec::new();
    let mut col_se: Vec<Option<f64>> = Vec::new();
    let mut col_stat: Vec<Option<f64>> = Vec::new();
    let mut col_pval: Vec<Option<f64>> = Vec::new();
    let mut col_ci_low: Vec<Option<f64>> = Vec::new();
    let mut col_ci_high: Vec<Option<f64>> = Vec::new();

    // Fixed parameters first (Marker loadings fixed to 1.0)
    for fixed in &model.fixed_params {
        col_term.push(fixed.name.clone());
        col_lhs.push(fixed.lhs.clone());
        col_op.push(fixed.op.clone());
        col_rhs.push(fixed.rhs.clone());
        col_est.push(fixed.fixed_value.unwrap_or(1.0));
        col_se.push(None);
        col_stat.push(None);
        col_pval.push(None);
        col_ci_low.push(None);
        col_ci_high.push(None);
    }

    // Free parameters
    for (i, param) in model.free_params.iter().enumerate() {
        let est = hat_theta[i];
        let se = se_vec[i];
        let z = est / se;
        let p = 2.0 * (1.0 - norm_dist.cdf(z.abs()));
        let ci_low = est - 1.96 * se;
        let ci_high = est + 1.96 * se;

        col_term.push(param.name.clone());
        col_lhs.push(param.lhs.clone());
        col_op.push(param.op.clone());
        col_rhs.push(param.rhs.clone());
        col_est.push(est);
        col_se.push(Some(se));
        col_stat.push(Some(z));
        col_pval.push(Some(p));
        col_ci_low.push(Some(ci_low));
        col_ci_high.push(Some(ci_high));
    }

    let parameters_df = df!(
        "term" => col_term,
        "lhs" => col_lhs,
        "op" => col_op,
        "rhs" => col_rhs,
        "estimate" => col_est,
        "std_error" => col_se,
        "statistic" => col_stat,
        "p_value" => col_pval,
        "conf_low" => col_ci_low,
        "conf_high" => col_ci_high,
    )
    .map_err(|e| Diagnostic::compute_error("C0202", format!("Failed to build parameters DataFrame: {e}")))?;

    Ok(SemFitOutput {
        n_obs: n,
        p,
        q,
        df: df_stat,
        f_min,
        chisq,
        p_value: p_val,
        baseline_chisq,
        baseline_df,
        srmr,
        parameters_df,
        sample_cov: s_mat,
        implied_cov,
    })
}

// ---------------------------------------------------------------------------
// 5. GHL Runtime Native Function Binding
// ---------------------------------------------------------------------------

/// `__sem_fit_core(spec, df)` — Computes sample covariance, fits SEM model via Wishart ML,
/// and returns the raw optimization and estimation record to GHL Layer 1.
pub fn native_sem_fit_core(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`sem(spec, df)` requires a SemSpec and a DataFrame",
        ));
    }

    let equations = match &args[0] {
        Value::SemSpec(eqs) => eqs,
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `sem()` must be a `sem_spec {{ ... }}`, found `{}`", other.type_name()),
            ));
        }
    };

    let (frame, _) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `sem()` must be a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };

    let fit = fit_sem_model(equations, frame)?;

    let mut record = BTreeMap::new();
    record.insert("n_obs".to_string(), Value::I64(fit.n_obs as i64));
    record.insert("p".to_string(), Value::I64(fit.p as i64));
    record.insert("df".to_string(), Value::I64(fit.df));
    record.insert("f_min".to_string(), Value::F64(fit.f_min));
    record.insert("chisq".to_string(), Value::F64(fit.chisq));
    record.insert("p_value".to_string(), Value::F64(fit.p_value));
    record.insert("baseline_chisq".to_string(), Value::F64(fit.baseline_chisq));
    record.insert("baseline_df".to_string(), Value::I64(fit.baseline_df));
    record.insert("srmr".to_string(), Value::F64(fit.srmr));
    record.insert("parameters".to_string(), Value::DataFrame {
        frame: fit.parameters_df,
        na_reasons: Arc::new(crate::na_reasons::NaReasonTable::new()),
    });
    record.insert("sample_cov".to_string(), Value::Matrix { rows: fit.p, cols: fit.p, data: Arc::new(fit.sample_cov) });
    record.insert("implied_cov".to_string(), Value::Matrix { rows: fit.p, cols: fit.p, data: Arc::new(fit.implied_cov) });

    Ok(Value::Record(Arc::new(record)))
}

/// `sample_cov(df, [cols])` — Computes and returns the sample covariance matrix as a `Matrix`.
pub fn native_sample_cov(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`sample_cov(df, [cols])` requires a DataFrame as its first argument",
        ));
    }

    let (frame, _) = match &args[0] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("`sample_cov()` requires a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };

    let cols: Vec<String> = if let Some(cols_arg) = args.get(1) {
        match cols_arg {
            Value::Vector(vd) => vd.iter().filter_map(|v| v.as_str().map(ToString::to_string)).collect(),
            other => {
                return Err(Diagnostic::compute_error(
                    "C0202",
                    format!("`sample_cov()` second argument must be a Vector of column names, found `{}`", other.type_name()),
                ));
            }
        }
    } else {
        frame.get_column_names().iter().map(|s| s.to_string()).collect()
    };

    let (_, _, s_mat, _) = compute_sample_covariance(frame, &cols)?;
    let p = cols.len();
    Ok(Value::Matrix { rows: p, cols: p, data: Arc::new(s_mat) })
}

/// `sem(spec, df)` — Main Structural Equation Modeling entrypoint.
/// Returns a `Value::Struct { name: "SemResult", fields }`.
pub fn native_sem(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`sem(spec, df)` requires 2 arguments: model specification and DataFrame",
        ));
    }

    let equations = match &args[0] {
        Value::SemSpec(eqs) => eqs.clone(),
        Value::Formula { .. } => vec![args[0].clone()],
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `sem()` must be a `sem_spec {{ ... }}` or Formula, found `{}`", other.type_name()),
            ));
        }
    };

    let frame = match &args[1] {
        Value::DataFrame { frame, .. } => frame,
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `sem()` must be a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };

    let fit = fit_sem_model(&equations, frame)?;

    // Global Fit Indices calculation:
    // CFI: 1 - max(chisq - df, 0) / max(baseline_chisq - baseline_df, chisq - df, 0)
    let d_model = (fit.chisq - fit.df as f64).max(0.0);
    let d_null = (fit.baseline_chisq - fit.baseline_df as f64).max(0.0);
    let cfi_raw = if d_null > 0.0 { 1.0 - (d_model / d_null) } else { 1.0 };
    let cfi = cfi_raw.clamp(0.0, 1.0);

    // TLI / NNFI: ((baseline_chisq / baseline_df) - (chisq / df)) / ((baseline_chisq / baseline_df) - 1)
    let null_ratio = if fit.baseline_df > 0 { fit.baseline_chisq / fit.baseline_df as f64 } else { 1.0 };
    let model_ratio = if fit.df > 0 { fit.chisq / fit.df as f64 } else { 1.0 };
    let tli_denom = null_ratio - 1.0;
    let tli = if tli_denom.abs() > 1e-12 { (null_ratio - model_ratio) / tli_denom } else { 1.0 };

    // RMSEA: sqrt(max(0, (chisq - df) / ((n_obs - 1) * df)))
    let rmsea_denom = ((fit.n_obs - 1) * fit.df.max(1) as usize) as f64;
    let rmsea = if rmsea_denom > 0.0 { (d_model / rmsea_denom).max(0.0).sqrt() } else { 0.0 };

    let mut fields = BTreeMap::new();
    fields.insert("chisq".to_string(), Value::F64(fit.chisq));
    fields.insert("df".to_string(), Value::I64(fit.df));
    fields.insert("p_value".to_string(), Value::F64(fit.p_value));
    fields.insert("cfi".to_string(), Value::F64(cfi));
    fields.insert("tli".to_string(), Value::F64(tli));
    fields.insert("rmsea".to_string(), Value::F64(rmsea));
    fields.insert("srmr".to_string(), Value::F64(fit.srmr));
    fields.insert("n_obs".to_string(), Value::I64(fit.n_obs as i64));
    fields.insert("f_min".to_string(), Value::F64(fit.f_min));
    fields.insert("baseline_chisq".to_string(), Value::F64(fit.baseline_chisq));
    fields.insert("baseline_df".to_string(), Value::I64(fit.baseline_df));
    fields.insert("parameters".to_string(), Value::DataFrame {
        frame: fit.parameters_df,
        na_reasons: Arc::new(crate::na_reasons::NaReasonTable::new()),
    });
    fields.insert("sample_cov".to_string(), Value::Matrix {
        rows: fit.p,
        cols: fit.p,
        data: Arc::new(fit.sample_cov),
    });
    fields.insert("implied_cov".to_string(), Value::Matrix {
        rows: fit.p,
        cols: fit.p,
        data: Arc::new(fit.implied_cov),
    });

    Ok(Value::Struct {
        name: "SemResult".to_string(),
        fields: Arc::new(fields),
    })
}
