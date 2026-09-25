//! Penalized Regularized Regression (Lasso, Ridge, ElasticNet) Kernel & Estimator.
//!
//! Implements Layer 0 (Rust heavy mathematics) coordinate descent algorithm
//! following the Friedman, Hastie & Tibshirani (2010) `glmnet` formulation,
//! as well as Layer 1 integration with NEKO verbs, K-Fold cross-validation,
//! and `RegularizedResult` packaging (Roadmap 08, Paso B.4).

use std::collections::BTreeMap;
use std::sync::Arc;
use ghl_diagnostics::Diagnostic;
use polars_core::prelude::*;

use crate::neko::Blueprint;
use crate::value::Value;
use crate::vector_data::VectorData;

// ---------------------------------------------------------------------------
// 1. Core Structures and Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegularizationKind {
    Lasso,
    Ridge,
    ElasticNet,
}

impl RegularizationKind {
    pub fn default_alpha(&self) -> f64 {
        match self {
            RegularizationKind::Lasso => 1.0,
            RegularizationKind::Ridge => 0.0,
            RegularizationKind::ElasticNet => 0.5,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            RegularizationKind::Lasso => "Lasso",
            RegularizationKind::Ridge => "Ridge",
            RegularizationKind::ElasticNet => "ElasticNet",
        }
    }
}

/// Options controlling coordinate descent convergence and standardization.
#[derive(Debug, Clone)]
pub struct RegularizedOptions {
    pub alpha: f64,
    pub lambda: Option<f64>,
    pub standardize: bool,
    pub tol: f64,
    pub max_iter: usize,
    pub n_lambda: usize,
    pub n_folds: usize,
}

impl Default for RegularizedOptions {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            lambda: None,
            standardize: true,
            tol: 1e-7,
            max_iter: 10000,
            n_lambda: 50,
            n_folds: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Mathematical Kernel: Coordinate Descent Engine (glmnet algorithm)
// ---------------------------------------------------------------------------

/// Fitted parameters for a single lambda value on standardized/centered data.
#[derive(Debug, Clone)]
struct SingleLambdaFit {
    beta: Vec<f64>,        // beta for predictors (standardized scale if standardize=true)
    beta_orig: Vec<f64>,   // beta for predictors on original scale
    intercept: f64,        // unpenalized intercept on original scale
    converged: bool,
    iterations: usize,
}

/// Fits coordinate descent for a single fixed lambda and alpha.
///
/// Objective function:
///   (1 / 2N) * || y_tilde - Z * beta ||_2^2 + lambda * (alpha * ||beta||_1 + (1 - alpha)/2 * ||beta||_2^2)
///
/// Arguments:
/// - `z_mat`: N x p matrix of standardized/centered predictors (row-major flat vec).
/// - `y_tilde`: N centered response values (y - mean(y)).
/// - `weights_z`: p diagonal terms (1/N) * sum_i z_ij^2 (1.0 if standardized).
/// - `scales`: p scale factors (std dev if standardized, 1.0 otherwise).
/// - `means_x`: p means of original predictors.
/// - `mean_y`: mean of original response.
/// - `lambda`: penalty weight.
/// - `alpha`: mixing parameter between L1 and L2.
/// - `beta_init`: warm-start initial coefficients of length p.
fn cd_fit_single(
    n: usize,
    p: usize,
    z_mat: &[f64],
    y_tilde: &[f64],
    weights_z: &[f64],
    scales: &[f64],
    means_x: &[f64],
    mean_y: f64,
    lambda: f64,
    alpha: f64,
    beta_init: &[f64],
    tol: f64,
    max_iter: usize,
) -> SingleLambdaFit {
    let mut beta = beta_init.to_vec();
    if beta.len() != p {
        beta = vec![0.0; p];
    }

    // Initialize residuals: r_i = y_tilde_i - sum_j z_ij * beta_j
    let mut r = Vec::with_capacity(n);
    for i in 0..n {
        let mut pred = 0.0;
        let row_offset = i * p;
        for j in 0..p {
            pred += z_mat[row_offset + j] * beta[j];
        }
        r.push(y_tilde[i] - pred);
    }

    let threshold = lambda * alpha;
    let l2_penalty = lambda * (1.0 - alpha);
    let n_f64 = n as f64;

    let mut converged = false;
    let mut iterations = 0;

    for iter in 0..max_iter {
        iterations = iter + 1;
        let mut max_delta = 0.0;

        for j in 0..p {
            let w_j = weights_z[j];
            if w_j < 1e-12 {
                // Constant column, skip
                continue;
            }

            // Compute gradient correlation: rho_j = (1/N) * sum_i(z_ij * r_i) + w_j * beta_j
            let mut dot = 0.0;
            for i in 0..n {
                dot += z_mat[i * p + j] * r[i];
            }
            let rho_j = dot / n_f64 + w_j * beta[j];

            // Soft-thresholding operator S(rho_j, threshold)
            let abs_rho = rho_j.abs();
            let s_rho = if abs_rho > threshold {
                rho_j.signum() * (abs_rho - threshold)
            } else {
                0.0
            };

            // Denominator: w_j + lambda * (1 - alpha)
            let denom = w_j + l2_penalty;
            let beta_new = if denom > 1e-15 { s_rho / denom } else { 0.0 };

            let delta = beta_new - beta[j];
            if delta.abs() > 1e-15 {
                // Residual update in O(N): r_i -= z_ij * delta
                for i in 0..n {
                    r[i] -= z_mat[i * p + j] * delta;
                }
                beta[j] = beta_new;
                let abs_d = delta.abs();
                if abs_d > max_delta {
                    max_delta = abs_d;
                }
            }
        }

        if max_delta < tol {
            converged = true;
            break;
        }
    }

    // Back-transform to original scale
    let mut beta_orig = Vec::with_capacity(p);
    let mut sum_x_beta = 0.0;
    for j in 0..p {
        let b_orig = if scales[j] > 1e-12 {
            beta[j] / scales[j]
        } else {
            0.0
        };
        beta_orig.push(b_orig);
        sum_x_beta += means_x[j] * b_orig;
    }
    let intercept = mean_y - sum_x_beta;

    SingleLambdaFit {
        beta,
        beta_orig,
        intercept,
        converged,
        iterations,
    }
}

// ---------------------------------------------------------------------------
// 3. Lambda Path Generation and Warm-Start Optimization
// ---------------------------------------------------------------------------

/// Computes a log-spaced grid of lambda values from lambda_max down to lambda_min.
fn compute_lambda_sequence(
    n: usize,
    p: usize,
    z_mat: &[f64],
    y_tilde: &[f64],
    alpha: f64,
    n_lambda: usize,
) -> Vec<f64> {
    let n_f64 = n as f64;
    let mut max_corr = 0.0;

    for j in 0..p {
        let mut dot = 0.0;
        for i in 0..n {
            dot += z_mat[i * p + j] * y_tilde[i];
        }
        let corr = (dot / n_f64).abs();
        if corr > max_corr {
            max_corr = corr;
        }
    }

    let effective_alpha = if alpha > 0.001 { alpha } else { 0.001 };
    let mut lambda_max = max_corr / effective_alpha;
    if lambda_max < 1e-6 {
        lambda_max = 1.0;
    }

    let lambda_ratio = if n >= p { 0.0001 } else { 0.01 };
    let lambda_min = lambda_max * lambda_ratio;

    let log_max = lambda_max.ln();
    let log_min = lambda_min.ln();
    let steps = if n_lambda > 1 { n_lambda - 1 } else { 1 };

    let mut sequence = Vec::with_capacity(n_lambda);
    for k in 0..n_lambda {
        let fraction = (k as f64) / (steps as f64);
        let log_lam = log_max + fraction * (log_min - log_max);
        sequence.push(log_lam.exp());
    }

    sequence
}

/// Fits an entire regularization path along a lambda sequence with warm starts.
fn fit_regularization_path(
    n: usize,
    p: usize,
    z_mat: &[f64],
    y_tilde: &[f64],
    weights_z: &[f64],
    scales: &[f64],
    means_x: &[f64],
    mean_y: f64,
    alpha: f64,
    lambdas: &[f64],
    tol: f64,
    max_iter: usize,
) -> Vec<SingleLambdaFit> {
    let mut fits = Vec::with_capacity(lambdas.len());
    let mut warm_beta = vec![0.0; p];

    for &lambda in lambdas {
        let fit = cd_fit_single(
            n,
            p,
            z_mat,
            y_tilde,
            weights_z,
            scales,
            means_x,
            mean_y,
            lambda,
            alpha,
            &warm_beta,
            tol,
            max_iter,
        );
        warm_beta = fit.beta.clone();
        fits.push(fit);
    }

    fits
}

// ---------------------------------------------------------------------------
// 4. K-Fold Cross-Validation (cv_glmnet)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CvResult {
    lambda_min: f64,
    lambda_1se: f64,
    mean_mse: Vec<f64>,
    se_mse: Vec<f64>,
    lambdas: Vec<f64>,
}

fn run_cv_glmnet(
    n: usize,
    p: usize,
    raw_x: &[f64],
    raw_y: &[f64],
    alpha: f64,
    lambdas: &[f64],
    n_folds: usize,
    standardize: bool,
    tol: f64,
    max_iter: usize,
) -> CvResult {
    let k_folds = n_folds.clamp(2, n);
    let mut fold_mse_matrix = vec![vec![0.0; lambdas.len()]; k_folds];

    for k in 0..k_folds {
        // Partition train and test indices
        let mut train_indices = Vec::new();
        let mut test_indices = Vec::new();

        for i in 0..n {
            if i % k_folds == k {
                test_indices.push(i);
            } else {
                train_indices.push(i);
            }
        }

        let n_train = train_indices.len();
        let n_test = test_indices.len();
        if n_train == 0 || n_test == 0 {
            continue;
        }

        // Compute train statistics
        let n_train_f64 = n_train as f64;
        let mut mean_y_train = 0.0;
        for &idx in &train_indices {
            mean_y_train += raw_y[idx];
        }
        mean_y_train /= n_train_f64;

        let mut y_tilde_train = Vec::with_capacity(n_train);
        for &idx in &train_indices {
            y_tilde_train.push(raw_y[idx] - mean_y_train);
        }

        let mut means_x_train = vec![0.0; p];
        let mut scales_train = vec![1.0; p];
        let mut weights_z_train = vec![1.0; p];

        for j in 0..p {
            let mut sum_xj = 0.0;
            for &idx in &train_indices {
                sum_xj += raw_x[idx * p + j];
            }
            let mean_j = sum_xj / n_train_f64;
            means_x_train[j] = mean_j;

            let mut sum_sq = 0.0;
            for &idx in &train_indices {
                let diff = raw_x[idx * p + j] - mean_j;
                sum_sq += diff * diff;
            }
            let var_j = sum_sq / n_train_f64;
            let std_j = var_j.sqrt();

            if standardize && std_j > 1e-12 {
                scales_train[j] = std_j;
                weights_z_train[j] = 1.0;
            } else {
                scales_train[j] = 1.0;
                weights_z_train[j] = if var_j > 1e-12 { var_j } else { 0.0 };
            }
        }

        // Standardize train predictors
        let mut z_train = vec![0.0; n_train * p];
        for (i_sub, &idx) in train_indices.iter().enumerate() {
            let row_offset = i_sub * p;
            for j in 0..p {
                let diff = raw_x[idx * p + j] - means_x_train[j];
                z_train[row_offset + j] = if scales_train[j] > 1e-12 {
                    diff / scales_train[j]
                } else {
                    0.0
                };
            }
        }

        // Fit path on fold train
        let fold_fits = fit_regularization_path(
            n_train,
            p,
            &z_train,
            &y_tilde_train,
            &weights_z_train,
            &scales_train,
            &means_x_train,
            mean_y_train,
            alpha,
            lambdas,
            tol,
            max_iter,
        );

        // Evaluate out-of-sample MSE on fold test
        for (m, fit) in fold_fits.iter().enumerate() {
            let mut sse = 0.0;
            for &idx in &test_indices {
                let mut pred = fit.intercept;
                for j in 0..p {
                    pred += raw_x[idx * p + j] * fit.beta_orig[j];
                }
                let err = raw_y[idx] - pred;
                sse += err * err;
            }
            fold_mse_matrix[k][m] = sse / (n_test as f64);
        }
    }

    // Compute mean and standard error of MSE across folds
    let num_lambdas = lambdas.len();
    let mut mean_mse = vec![0.0; num_lambdas];
    let mut se_mse = vec![0.0; num_lambdas];
    let k_f64 = k_folds as f64;

    for m in 0..num_lambdas {
        let mut sum_m = 0.0;
        for k in 0..k_folds {
            sum_m += fold_mse_matrix[k][m];
        }
        let mean_m = sum_m / k_f64;
        mean_mse[m] = mean_m;

        let mut sum_sq_diff = 0.0;
        for k in 0..k_folds {
            let diff = fold_mse_matrix[k][m] - mean_m;
            sum_sq_diff += diff * diff;
        }
        let var_m = if k_folds > 1 {
            sum_sq_diff / (k_f64 - 1.0)
        } else {
            0.0
        };
        se_mse[m] = var_m.sqrt() / k_f64.sqrt();
    }

    // Find lambda_min: index minimizing mean_mse
    let mut min_idx = 0;
    let mut min_mse = f64::INFINITY;
    for (m, &mse) in mean_mse.iter().enumerate() {
        if mse < min_mse {
            min_mse = mse;
            min_idx = m;
        }
    }
    let lambda_min = lambdas[min_idx];

    // Find lambda_1se: largest lambda (earliest in sequence) with mean_mse <= min_mse + se_mse[min_idx]
    let threshold_1se = min_mse + se_mse[min_idx];
    let mut idx_1se = min_idx;
    for m in 0..=min_idx {
        if mean_mse[m] <= threshold_1se {
            idx_1se = m;
            break;
        }
    }
    let lambda_1se = lambdas[idx_1se];

    CvResult {
        lambda_min,
        lambda_1se,
        mean_mse,
        se_mse,
        lambdas: lambdas.to_vec(),
    }
}

// ---------------------------------------------------------------------------
// 5. High-Level Fit Orchestration & Packaging
// ---------------------------------------------------------------------------

/// Orchestrates fitting Lasso, Ridge, or ElasticNet models.
pub fn fit_regularized(
    formula_val: &Value,
    frame_val: &DataFrame,
    na_reasons: &crate::na_reasons::NaReasonTable,
    kind: RegularizationKind,
    options: RegularizedOptions,
) -> Result<Value, Diagnostic> {
    // 1. Extract formula response and predictors
    let (response_var, predictor_vars) = match formula_val {
        Value::Formula { response, parts, .. } => {
            if response.is_empty() {
                return Err(Diagnostic::statistical_error(
                    "S0100",
                    "Formula requires a response variable: `response ~ predictors`",
                ));
            }
            if parts.is_empty() || parts[0].is_empty() {
                return Err(Diagnostic::statistical_error(
                    "S0100",
                    "Formula requires at least one predictor variable: `response ~ predictors`",
                ));
            }
            (response.clone(), parts[0].clone())
        }
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Expected a Formula, found `{}`", other.type_name()),
            ));
        }
    };

    // 2. Bake DataFrame using Blueprint
    let bp = Blueprint::new(response_var.clone(), predictor_vars);
    let (x_data, y_data, _dispositions, n_obs, p_cols, term_names, _term_levels) =
        bp.bake(frame_val, na_reasons)?;

    if p_cols <= 1 {
        return Err(Diagnostic::statistical_error(
            "S0100",
            "At least one predictor variable is required for regularized regression",
        ));
    }
    if n_obs < 3 {
        return Err(Diagnostic::statistical_error(
            "S0100",
            format!("At least 3 valid observations required, found {}", n_obs),
        ));
    }

    let p = p_cols - 1;
    let n = n_obs;
    let n_f64 = n as f64;

    // 3. Extract raw X (N x p) and raw Y (N)
    let mut raw_x = vec![0.0; n * p];
    for i in 0..n {
        let x_offset = i * p_cols;
        let dest_offset = i * p;
        for j in 0..p {
            raw_x[dest_offset + j] = x_data[x_offset + j + 1];
        }
    }
    let raw_y = y_data;

    // 4. Compute column statistics for centering/standardization
    let mut mean_y = 0.0;
    for &y in &raw_y {
        mean_y += y;
    }
    mean_y /= n_f64;

    let mut y_tilde = Vec::with_capacity(n);
    for &y in &raw_y {
        y_tilde.push(y - mean_y);
    }

    let mut means_x = vec![0.0; p];
    let mut scales = vec![1.0; p];
    let mut weights_z = vec![1.0; p];

    for j in 0..p {
        let mut sum_xj = 0.0;
        for i in 0..n {
            sum_xj += raw_x[i * p + j];
        }
        let mean_j = sum_xj / n_f64;
        means_x[j] = mean_j;

        let mut sum_sq = 0.0;
        for i in 0..n {
            let diff = raw_x[i * p + j] - mean_j;
            sum_sq += diff * diff;
        }
        let var_j = sum_sq / n_f64;
        let std_j = var_j.sqrt();

        if options.standardize && std_j > 1e-12 {
            scales[j] = std_j;
            weights_z[j] = 1.0;
        } else {
            scales[j] = 1.0;
            weights_z[j] = if var_j > 1e-12 { var_j } else { 0.0 };
        }
    }

    let mut z_mat = vec![0.0; n * p];
    for i in 0..n {
        let row_offset = i * p;
        for j in 0..p {
            let diff = raw_x[row_offset + j] - means_x[j];
            z_mat[row_offset + j] = if scales[j] > 1e-12 {
                diff / scales[j]
            } else {
                0.0
            };
        }
    }

    let alpha = options.alpha.clamp(0.0, 1.0);

    // 5. Check if user provided lambda or if CV is required
    let (chosen_lambda, cv_res_opt) = match options.lambda {
        Some(lam) => (lam.max(1e-12), None),
        None => {
            // Compute lambda path and run K-fold cross validation
            let lambdas = compute_lambda_sequence(n, p, &z_mat, &y_tilde, alpha, options.n_lambda);
            let cv = run_cv_glmnet(
                n,
                p,
                &raw_x,
                &raw_y,
                alpha,
                &lambdas,
                options.n_folds,
                options.standardize,
                options.tol,
                options.max_iter,
            );
            let opt_lam = cv.lambda_min;
            (opt_lam, Some(cv))
        }
    };

    // 6. Fit final model on full dataset at chosen_lambda
    let final_fit = cd_fit_single(
        n,
        p,
        &z_mat,
        &y_tilde,
        &weights_z,
        &scales,
        &means_x,
        mean_y,
        chosen_lambda,
        alpha,
        &vec![0.0; p],
        options.tol,
        options.max_iter,
    );

    // 7. Assemble full coefficients: [intercept, beta_orig_1, ..., beta_orig_p]
    let mut full_coefficients = Vec::with_capacity(p + 1);
    full_coefficients.push(final_fit.intercept);
    for &b in &final_fit.beta_orig {
        full_coefficients.push(b);
    }

    // 8. Fitted values, residuals, and goodness of fit
    let mut fitted = Vec::with_capacity(n);
    let mut residuals = Vec::with_capacity(n);
    let mut ssr = 0.0;
    let mut tss = 0.0;

    for i in 0..n {
        let mut y_hat = final_fit.intercept;
        let row_offset = i * p;
        for j in 0..p {
            y_hat += raw_x[row_offset + j] * final_fit.beta_orig[j];
        }
        let res = raw_y[i] - y_hat;
        fitted.push(y_hat);
        residuals.push(res);
        ssr += res * res;
        let diff_mean = raw_y[i] - mean_y;
        tss += diff_mean * diff_mean;
    }

    let r2 = if tss > 1e-15 {
        (1.0 - ssr / tss).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let mse = ssr / n_f64;

    // Identify active (non-zero) terms
    let mut active_terms = Vec::new();
    let mut is_selected_col = Vec::with_capacity(p + 1);
    is_selected_col.push(Value::Bool(true)); // intercept always included

    let mut n_selected = 0;
    for j in 0..p {
        let b = final_fit.beta_orig[j];
        let selected = b.abs() > 1e-10;
        if selected {
            active_terms.push(term_names[j + 1].clone());
            n_selected += 1;
        }
        is_selected_col.push(Value::Bool(selected));
    }

    // 9. Build Parameters DataFrame
    let param_terms: Vec<Value> = term_names.iter().map(|s| Value::String(s.clone())).collect();
    let param_estimates: Vec<Value> = full_coefficients.iter().map(|&x| Value::F64(x)).collect();

    let param_columns: Vec<(String, Vec<Value>)> = vec![
        ("term".to_string(), param_terms),
        ("estimate".to_string(), param_estimates),
        ("is_selected".to_string(), is_selected_col),
    ];
    let (param_frame, param_na) = crate::polars_bridge::build_dataframe(&param_columns)?;
    let params_df = Value::DataFrame {
        frame: param_frame,
        na_reasons: param_na,
    };

    // 10. Build CV metrics DataFrame if cross-validation was run
    let cv_metrics_val = if let Some(ref cv) = cv_res_opt {
        let cv_columns: Vec<(String, Vec<Value>)> = vec![
            ("lambda".to_string(), cv.lambdas.iter().map(|&x| Value::F64(x)).collect()),
            ("mean_mse".to_string(), cv.mean_mse.iter().map(|&x| Value::F64(x)).collect()),
            ("se_mse".to_string(), cv.se_mse.iter().map(|&x| Value::F64(x)).collect()),
        ];
        let (cv_frame, cv_na) = crate::polars_bridge::build_dataframe(&cv_columns)?;
        Value::DataFrame {
            frame: cv_frame,
            na_reasons: cv_na,
        }
    } else {
        Value::NA(None)
    };

    let (lambda_min_val, lambda_1se_val) = if let Some(ref cv) = cv_res_opt {
        (Value::F64(cv.lambda_min), Value::F64(cv.lambda_1se))
    } else {
        (Value::NA(None), Value::NA(None))
    };

    // 11. Package into RegularizedResult struct
    let mut fields = BTreeMap::new();
    fields.insert("model".to_string(), Value::String(kind.display_name().to_lowercase()));
    fields.insert("model_type".to_string(), Value::String(kind.display_name().to_string()));
    fields.insert("response".to_string(), Value::String(response_var));
    fields.insert(
        "terms".to_string(),
        Value::Vector(VectorData::from_values(term_names.iter().map(|s| Value::String(s.clone())).collect())),
    );
    fields.insert(
        "active_terms".to_string(),
        Value::Vector(VectorData::from_values(active_terms.iter().map(|s| Value::String(s.clone())).collect())),
    );
    fields.insert(
        "coefficients".to_string(),
        Value::Vector(VectorData::from_f64(full_coefficients)),
    );
    fields.insert("alpha".to_string(), Value::F64(alpha));
    fields.insert("lambda".to_string(), Value::F64(chosen_lambda));
    fields.insert("lambda_min".to_string(), lambda_min_val);
    fields.insert("lambda_1se".to_string(), lambda_1se_val);
    fields.insert("r2".to_string(), Value::F64(r2));
    fields.insert("mse".to_string(), Value::F64(mse));
    fields.insert("n_obs".to_string(), Value::I64(n as i64));
    fields.insert("n_features".to_string(), Value::I64(p as i64));
    fields.insert("n_selected".to_string(), Value::I64(n_selected as i64));
    fields.insert("converged".to_string(), Value::Bool(final_fit.converged));
    fields.insert("iterations".to_string(), Value::I64(final_fit.iterations as i64));
    fields.insert("parameters".to_string(), params_df);
    fields.insert("cv_metrics".to_string(), cv_metrics_val);
    fields.insert("residuals".to_string(), Value::Vector(VectorData::from_f64(residuals)));
    fields.insert("fitted".to_string(), Value::Vector(VectorData::from_f64(fitted)));

    Ok(Value::Struct {
        name: "RegularizedResult".to_string(),
        fields: Arc::new(fields),
    })
}

// ---------------------------------------------------------------------------
// 6. Prediction Engine for New DataFrames
// ---------------------------------------------------------------------------

/// Computes linear predictions X_new * beta + intercept for a new DataFrame.
pub fn predict_regularized(
    fields: &BTreeMap<String, Value>,
    new_frame: &DataFrame,
) -> Result<Value, Diagnostic> {
    let coef_val = fields.get("coefficients").ok_or_else(|| {
        Diagnostic::compute_error("C0201", "Missing coefficients in RegularizedResult")
    })?;
    let terms_val = fields.get("terms").ok_or_else(|| {
        Diagnostic::compute_error("C0201", "Missing terms in RegularizedResult")
    })?;

    let coefs = match coef_val {
        Value::Vector(v) => {
            let mut out = Vec::with_capacity(v.len());
            for item in v.iter() {
                out.push(item.as_f64().unwrap_or(0.0));
            }
            out
        }
        _ => return Err(Diagnostic::compute_error("C0201", "Invalid coefficients")),
    };

    let terms = match terms_val {
        Value::Vector(v) => {
            let mut out = Vec::with_capacity(v.len());
            for item in v.iter() {
                out.push(item.as_str().unwrap_or("").to_string());
            }
            out
        }
        _ => return Err(Diagnostic::compute_error("C0201", "Invalid terms")),
    };

    let intercept = coefs[0];
    let n_new = new_frame.height();
    let mut predictions = vec![intercept; n_new];

    for (j, term) in terms.iter().enumerate().skip(1) {
        let b = coefs[j];
        if b.abs() < 1e-15 {
            continue;
        }

        let col = new_frame.column(term).map_err(|_| {
            Diagnostic::statistical_error(
                "S0203",
                format!("Predictor variable `{}` not found in new DataFrame", term),
            )
        })?;

        for i in 0..n_new {
            if let Ok(av) = col.get(i) {
                let val = match av {
                    AnyValue::Float64(f) => f,
                    AnyValue::Float32(f) => f as f64,
                    AnyValue::Int64(i_val) => i_val as f64,
                    AnyValue::Int32(i_val) => i_val as f64,
                    _ => 0.0,
                };
                predictions[i] += b * val;
            }
        }
    }

    Ok(Value::Vector(VectorData::from_f64(predictions)))
}

// ---------------------------------------------------------------------------
// 7. Native Functions Interface
// ---------------------------------------------------------------------------

fn extract_formula_and_df<'a>(
    func_name: &str,
    args: &'a [Value],
) -> Result<(&'a Value, &'a DataFrame, &'a crate::na_reasons::NaReasonTable), Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`{}()` requires at least 2 arguments: `{}(formula, df, ...)`", func_name, func_name),
        ));
    }

    match &args[0] {
        Value::Formula { .. } => {}
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `{}()` must be a Formula, found `{}`", func_name, other.type_name()),
            ));
        }
    }

    match &args[1] {
        Value::DataFrame { frame, na_reasons } => Ok((&args[0], frame, na_reasons)),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("Second argument of `{}()` must be a DataFrame, found `{}`", func_name, other.type_name()),
        )),
    }
}

/// Native function: `lasso(formula, df, [lambda], [standardize])`
pub fn native_lasso(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (formula, frame, na_reasons) = extract_formula_and_df("lasso", &args)?;
    let mut options = RegularizedOptions {
        alpha: 1.0,
        ..Default::default()
    };

    if args.len() >= 3 {
        match &args[2] {
            Value::F64(lam) => options.lambda = Some(*lam),
            Value::I64(lam) => options.lambda = Some(*lam as f64),
            Value::Bool(b) => options.standardize = *b,
            _ => {}
        }
    }
    if args.len() >= 4 {
        if let Value::Bool(b) = &args[3] {
            options.standardize = *b;
        }
    }

    fit_regularized(formula, frame, na_reasons, RegularizationKind::Lasso, options)
}

/// Native function: `ridge(formula, df, [lambda], [standardize])`
pub fn native_ridge(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (formula, frame, na_reasons) = extract_formula_and_df("ridge", &args)?;
    let mut options = RegularizedOptions {
        alpha: 0.0,
        ..Default::default()
    };

    if args.len() >= 3 {
        match &args[2] {
            Value::F64(lam) => options.lambda = Some(*lam),
            Value::I64(lam) => options.lambda = Some(*lam as f64),
            Value::Bool(b) => options.standardize = *b,
            _ => {}
        }
    }
    if args.len() >= 4 {
        if let Value::Bool(b) = &args[3] {
            options.standardize = *b;
        }
    }

    fit_regularized(formula, frame, na_reasons, RegularizationKind::Ridge, options)
}

/// Native function: `elastic_net(formula, df, [lambda], [alpha], [standardize])`
pub fn native_elastic_net(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (formula, frame, na_reasons) = extract_formula_and_df("elastic_net", &args)?;
    let mut options = RegularizedOptions {
        alpha: 0.5,
        ..Default::default()
    };

    if args.len() >= 3 {
        match &args[2] {
            Value::F64(lam) => options.lambda = Some(*lam),
            Value::I64(lam) => options.lambda = Some(*lam as f64),
            _ => {}
        }
    }
    if args.len() >= 4 {
        match &args[3] {
            Value::F64(a) => options.alpha = *a,
            Value::I64(a) => options.alpha = *a as f64,
            _ => {}
        }
    }
    if args.len() >= 5 {
        if let Value::Bool(b) = &args[4] {
            options.standardize = *b;
        }
    }

    fit_regularized(formula, frame, na_reasons, RegularizationKind::ElasticNet, options)
}

/// Native function: `cv_glmnet(formula, df, [alpha], [n_folds], [n_lambda], [standardize])`
pub fn native_cv_glmnet(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (formula, frame, na_reasons) = extract_formula_and_df("cv_glmnet", &args)?;
    let mut options = RegularizedOptions::default();

    if args.len() >= 3 {
        match &args[2] {
            Value::F64(a) => options.alpha = *a,
            Value::I64(a) => options.alpha = *a as f64,
            _ => {}
        }
    }
    if args.len() >= 4 {
        if let Value::I64(k) = &args[3] {
            options.n_folds = (*k as usize).max(2);
        }
    }
    if args.len() >= 5 {
        if let Value::I64(nl) = &args[4] {
            options.n_lambda = (*nl as usize).max(5);
        }
    }
    if args.len() >= 6 {
        if let Value::Bool(b) = &args[5] {
            options.standardize = *b;
        }
    }

    let kind = if options.alpha >= 0.999 {
        RegularizationKind::Lasso
    } else if options.alpha <= 0.001 {
        RegularizationKind::Ridge
    } else {
        RegularizationKind::ElasticNet
    };

    fit_regularized(formula, frame, na_reasons, kind, options)
}
