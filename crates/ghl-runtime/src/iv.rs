//! Instrumental Variables (IV / 2SLS) Core Engine (Roadmap 08, Paso B.3).
//!
//! Provides the mathematical Layer 0 and native estimator for Two-Stage Least Squares:
//! - Equation partitioning into exogenous regressors, endogenous regressors, and excluded instruments.
//! - Necessary order condition check for identification (L_instr >= K_endog).
//! - Stage 1: Projection of endogenous regressors onto the full instrument space.
//! - First-stage F-statistic diagnostic for weak instruments (Stock-Yogo threshold F > 10).
//! - Stage 2: Structural parameter estimation via projected regressors.
//! - Correct structural residuals calculation using original regressor matrix X.
//! - Classical and Robust (HC1) covariance matrices.
//! - Wu-Hausman endogeneity specification test (evaluating consistency of OLS vs 2SLS).
//! - Sargan overidentification test for valid instruments (when L > K_endog).
//! - `IvResult` struct packaging with full NEKO contract (`summary`, `tidy`, `glance`,
//!   `vcov`, `coef`, `residuals`).

use std::sync::Arc;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use faer::prelude::*;
use faer::Mat;
use polars_core::prelude::*;
use statrs::distribution::{ContinuousCDF, StudentsT, FisherSnedecor, ChiSquared, Normal};
use ghl_diagnostics::Diagnostic;

use crate::value::{Value, FormulaParts};
use crate::vector_data::VectorData;
use crate::polars_bridge::pull_column_as_values;

// ---------------------------------------------------------------------------
// 1. Data Structures & Options
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IvVcovSpec {
    Classical,
    Robust,
}

// ---------------------------------------------------------------------------
// 2. Helper Functions: Numerical Extraction & Linear Regression
// ---------------------------------------------------------------------------

fn value_to_f64_opt(v: &Value) -> Option<f64> {
    match v {
        Value::F64(f) if !f.is_nan() => Some(*f),
        Value::I64(i) => Some(*i as f64),
        _ => None,
    }
}

/// Helper: Fits simple OLS `A * beta = y` using faer LU decomposition.
/// Returns (beta, ssr, residuals).
fn solve_ols_faer(
    x_mat: &Mat<f64>,
    y_vec: &Mat<f64>,
) -> Result<(Vec<f64>, f64, Vec<f64>), Diagnostic> {
    let n = x_mat.nrows();
    let k = x_mat.ncols();

    let xt = x_mat.transpose();
    let xtx = &xt * x_mat;
    let xty = &xt * y_vec;

    let lu = xtx.partial_piv_lu();
    let u = lu.U();
    let min_pivot = (0..k).map(|i| u[(i, i)].abs()).fold(f64::INFINITY, f64::min);
    if min_pivot < 1e-12 {
        return Err(Diagnostic::statistical_error(
            "S0101",
            "Collinear columns detected in design matrix: matrix (X'X) is singular and cannot be inverted",
        ));
    }

    let faer_beta = lu.solve(&xty);
    let beta: Vec<f64> = (0..k).map(|r| faer_beta[(r, 0)]).collect();

    let pred = x_mat * &faer_beta;
    let mut ssr = 0.0;
    let mut residuals = Vec::with_capacity(n);
    for i in 0..n {
        let res = y_vec[(i, 0)] - pred[(i, 0)];
        residuals.push(res);
        ssr += res * res;
    }

    Ok((beta, ssr, residuals))
}

// ---------------------------------------------------------------------------
// 3. Two-Stage Least Squares (2SLS) Core Engine
// ---------------------------------------------------------------------------

/// Fits Two-Stage Least Squares (IV / 2SLS) on a DataFrame.
pub fn fit_iv(
    formula_parts: &FormulaParts,
    frame: &DataFrame,
    na_reasons: &crate::na_reasons::NaReasonTable,
    vcov_spec: IvVcovSpec,
) -> Result<Value, Diagnostic> {
    // 1. Validate formula structure (must have 2 parts separated by `|`)
    if !formula_parts.has_fixed_effects() {
        return Err(Diagnostic::statistical_error(
            "S0100",
            "Formula for `iv_regress()` must contain two parts separated by `|`: `response ~ regressors | instruments` (e.g. `y ~ x_exog + x_endog | x_exog + z_instr`)",
        ).with_help("Specify the structural regressors before `|` and the instruments after `|`."));
    }

    let y_col = &formula_parts.response;
    let x_terms = &formula_parts.terms;
    let z_terms = &formula_parts.absorbed; // In 2-part formulas, part 1 is stored in absorbed

    if x_terms.is_empty() {
        return Err(Diagnostic::statistical_error(
            "S0100",
            "`iv_regress()` requires at least one regressor in the structural equation",
        ));
    }
    if z_terms.is_empty() {
        return Err(Diagnostic::statistical_error(
            "S0100",
            "`iv_regress()` requires at least one instrument in the instrument specification",
        ));
    }

    // 2. Identify Exogenous Regressors, Endogenous Regressors, and Excluded Instruments
    let x_set: BTreeSet<String> = x_terms.iter().cloned().collect();
    let z_set: BTreeSet<String> = z_terms.iter().cloned().collect();

    let x_exog: Vec<String> = x_terms.iter().filter(|t| z_set.contains(*t)).cloned().collect();
    let x_endog: Vec<String> = x_terms.iter().filter(|t| !z_set.contains(*t)).cloned().collect();
    let z_excluded: Vec<String> = z_terms.iter().filter(|t| !x_set.contains(*t)).cloned().collect();

    let k_endog = x_endog.len();
    let l_excluded = z_excluded.len();

    // 3. Order condition check for identification: L_excluded >= K_endog
    if l_excluded < k_endog {
        return Err(Diagnostic::statistical_error(
            "S0101",
            format!(
                "Order condition failed: model is underidentified. Found {} endogenous regressor(s) ({:?}) but only {} excluded instrument(s) ({:?}). At least {} more instrument(s) required for identification.",
                k_endog, x_endog, l_excluded, z_excluded, k_endog - l_excluded
            ),
        ).with_help("Add more excluded instruments to the right of `|` so that excluded instruments >= endogenous regressors."));
    }

    // 4. Gather all unique variable names for listwise missing value deletion
    let mut all_vars_set = BTreeSet::new();
    all_vars_set.insert(y_col.clone());
    for t in x_terms {
        all_vars_set.insert(t.clone());
    }
    for t in z_terms {
        all_vars_set.insert(t.clone());
    }
    let all_vars: Vec<String> = all_vars_set.into_iter().collect();

    // Extract columns from DataFrame
    let mut var_columns: HashMap<String, Vec<Value>> = HashMap::new();
    for v in &all_vars {
        let vals = pull_column_as_values(frame, na_reasons, v)?;
        var_columns.insert(v.clone(), vals);
    }

    let total_rows = frame.height();
    let mut valid_rows = Vec::new();

    // Listwise missing-value check
    for r in 0..total_rows {
        let mut row_ok = true;
        for v in &all_vars {
            let col = &var_columns[v];
            if value_to_f64_opt(&col[r]).is_none() {
                row_ok = false;
                break;
            }
        }
        if row_ok {
            valid_rows.push(r);
        }
    }

    let n = valid_rows.len();
    let k_regressors = 1 + x_terms.len(); // Including constant intercept
    let l_instruments = 1 + z_terms.len(); // Including constant intercept

    if n <= l_instruments {
        return Err(Diagnostic::statistical_error(
            "S0101",
            format!("Sample size N={n} must be strictly greater than total number of instruments l={l_instruments}"),
        ));
    }

    // 5. Build numeric matrices X, Z, y
    // y: (N x 1)
    let y_vals = &var_columns[y_col];
    let faer_y = Mat::<f64>::from_fn(n, 1, |r, _| value_to_f64_opt(&y_vals[valid_rows[r]]).unwrap());

    // X: (N x k_regressors), col 0 = 1.0 (Intercept)
    let faer_x = Mat::<f64>::from_fn(n, k_regressors, |r, c| {
        if c == 0 {
            1.0
        } else {
            let term = &x_terms[c - 1];
            value_to_f64_opt(&var_columns[term][valid_rows[r]]).unwrap()
        }
    });

    // Z: (N x l_instruments), col 0 = 1.0 (Intercept)
    let faer_z = Mat::<f64>::from_fn(n, l_instruments, |r, c| {
        if c == 0 {
            1.0
        } else {
            let term = &z_terms[c - 1];
            value_to_f64_opt(&var_columns[term][valid_rows[r]]).unwrap()
        }
    });

    // 6. Stage 1: Projection of X onto Z
    // Compute (Z'Z)^-1
    let zt = faer_z.transpose();
    let ztz = &zt * &faer_z;
    let lu_zz = ztz.partial_piv_lu();

    let u_zz = lu_zz.U();
    let min_piv_zz = (0..l_instruments).map(|i| u_zz[(i, i)].abs()).fold(f64::INFINITY, f64::min);
    if min_piv_zz < 1e-12 {
        return Err(Diagnostic::statistical_error(
            "S0101",
            "Instruments are collinear: matrix (Z'Z) is singular and cannot be inverted",
        ).with_help("Check for collinear instruments or redundant variables in the instrument specification."));
    }

    let ident_l = Mat::<f64>::from_fn(l_instruments, l_instruments, |r, c| if r == c { 1.0 } else { 0.0 });
    let zz_inv = lu_zz.solve(&ident_l);

    // Compute first-stage coefficients: Gamma = (Z'Z)^-1 (Z' X) of size (l x k)
    let ztx = &zt * &faer_x;
    let gamma = &zz_inv * &ztx;

    // Compute projected regressors: X_hat = Z * Gamma of size (N x k)
    let x_hat = &faer_z * &gamma;

    // First-stage residuals: U_hat = X - X_hat
    let u_hat = &faer_x - &x_hat;

    // Compute Weak Instruments Diagnostic (F-statistic for each endogenous variable)
    let mut first_stage_f_stats = Vec::new();
    if k_endog > 0 && l_excluded > 0 {
        // Construct X_exog matrix: (N x (1 + x_exog.len()))
        let k_exog = 1 + x_exog.len();
        let faer_x_exog = Mat::<f64>::from_fn(n, k_exog, |r, c| {
            if c == 0 {
                1.0
            } else {
                let term = &x_exog[c - 1];
                value_to_f64_opt(&var_columns[term][valid_rows[r]]).unwrap()
            }
        });

        for endog_var in &x_endog {
            // Find col in X
            let col_idx = x_terms.iter().position(|t| t == endog_var).unwrap() + 1;
            let endog_col = Mat::<f64>::from_fn(n, 1, |r, _| faer_x[(r, col_idx)]);

            // Unrestricted SSR: sum of squared residuals in U_hat for this column
            let ssr_unrestr: f64 = (0..n).map(|r| u_hat[(r, col_idx)].powi(2)).sum();

            // Restricted OLS: endog_col ~ X_exog
            let (_, ssr_restr, _) = solve_ols_faer(&faer_x_exog, &endog_col)?;

            let df1 = l_excluded as f64;
            let df2 = (n - l_instruments) as f64;
            let f_stat = if ssr_unrestr > 1e-15 && df2 > 0.0 {
                ((ssr_restr - ssr_unrestr) / df1) / (ssr_unrestr / df2)
            } else if ssr_restr > 1e-15 && df2 > 0.0 {
                // Zero residual sum of squares: perfect prediction -> infinite F
                1e9
            } else {
                0.0
            };
            first_stage_f_stats.push(f_stat.max(0.0));
        }
    }

    let min_f_stat = first_stage_f_stats.iter().copied().fold(f64::INFINITY, f64::min);
    let is_weak_instruments = min_f_stat < 10.0 && k_endog > 0;

    // 7. Stage 2: Second-stage OLS on instrumented regressors (X_hat)
    // Normal equations: A = X_hat' X_hat = X' P_Z X
    let x_hat_t = x_hat.transpose();
    let x_hat_tx_hat = &x_hat_t * &x_hat;
    let x_hat_ty = &x_hat_t * &faer_y;

    let lu_a = x_hat_tx_hat.partial_piv_lu();
    let u_a = lu_a.U();
    let min_piv_a = (0..k_regressors).map(|i| u_a[(i, i)].abs()).fold(f64::INFINITY, f64::min);
    if min_piv_a < 1e-12 {
        return Err(Diagnostic::statistical_error(
            "S0101",
            "Rank condition failed: matrix (X_hat' X_hat) is singular. Endogenous regressors cannot be identified by the instruments.",
        ).with_help("Verify that the instruments are sufficiently correlated with the endogenous regressors (weak instrument / rank failure)."));
    }

    let faer_beta = lu_a.solve(&x_hat_ty);
    let beta: Vec<f64> = (0..k_regressors).map(|r| faer_beta[(r, 0)]).collect();

    // Bread matrix Q = (X_hat' X_hat)^-1
    let ident_k = Mat::<f64>::from_fn(k_regressors, k_regressors, |r, c| if r == c { 1.0 } else { 0.0 });
    let q_mat = lu_a.solve(&ident_k);

    // 8. Compute Structural Residuals using ACTUAL X
    let pred_structural = &faer_x * &faer_beta;
    let mut residuals = Vec::with_capacity(n);
    let mut ssr = 0.0;
    for i in 0..n {
        let res = faer_y[(i, 0)] - pred_structural[(i, 0)];
        residuals.push(res);
        ssr += res * res;
    }

    let df_resid = (n as i64) - (k_regressors as i64);
    let df_resid_f64 = df_resid as f64;
    let s2 = ssr / df_resid_f64;

    let y_mean = (0..n).map(|r| faer_y[(r, 0)]).sum::<f64>() / (n as f64);
    let tss = (0..n).map(|r| (faer_y[(r, 0)] - y_mean).powi(2)).sum::<f64>();
    let r2 = if tss > 1e-15 {
        (1.0 - ssr / tss).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let mut fitted = Vec::with_capacity(n);
    for i in 0..n {
        fitted.push(pred_structural[(i, 0)]);
    }

    // 9. Covariance Matrix V(beta)
    let (vcov_faer, vcov_desc) = match vcov_spec {
        IvVcovSpec::Classical => {
            let v = &q_mat * s2;
            (v, "Classical (IID)".to_string())
        }
        IvVcovSpec::Robust => {
            let mut m_mat = Mat::<f64>::zeros(k_regressors, k_regressors);
            for i in 0..n {
                let res2 = residuals[i] * residuals[i];
                for r in 0..k_regressors {
                    let xr = x_hat[(i, r)]; // Robust sandwich uses x_hat
                    for c in 0..k_regressors {
                        m_mat[(r, c)] += res2 * xr * x_hat[(i, c)];
                    }
                }
            }
            let factor = (n as f64) / df_resid_f64;
            let v = &q_mat * (&m_mat * &q_mat) * factor;
            (v, "Robust (HC1)".to_string())
        }
    };

    // Standard errors, t-statistics, p-values
    let mut std_errors = Vec::with_capacity(k_regressors);
    let mut t_stats = Vec::with_capacity(k_regressors);
    let mut p_values = Vec::with_capacity(k_regressors);

    let t_dist = StudentsT::new(0.0, 1.0, df_resid_f64).ok();

    for j in 0..k_regressors {
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

    // 10. Diagnostic Tests
    // A. Wu-Hausman Endogeneity Test: Regress y ~ X + U_hat_endog
    let (wu_hausman_stat, wu_hausman_p) = if k_endog > 0 && n > (k_regressors + k_endog) {
        // Form augmented matrix X_aux = [X, U_hat_endog]
        let k_aux = k_regressors + k_endog;
        let faer_x_aux = Mat::<f64>::from_fn(n, k_aux, |r, c| {
            if c < k_regressors {
                faer_x[(r, c)]
            } else {
                let endog_idx = c - k_regressors;
                let endog_var = &x_endog[endog_idx];
                let col_idx = x_terms.iter().position(|t| t == endog_var).unwrap() + 1;
                u_hat[(r, col_idx)]
            }
        });

        // OLS plain y ~ X
        let ols_plain = solve_ols_faer(&faer_x, &faer_y);
        // Auxiliary OLS y ~ X_aux
        let ols_aux = solve_ols_faer(&faer_x_aux, &faer_y);

        if let (Ok((_, ssr_ols, _)), Ok((_, ssr_aux, _))) = (ols_plain, ols_aux) {
            let df1 = k_endog as f64;
            let df2 = (n - k_aux) as f64;
            let f_stat = if ssr_aux > 1e-15 && df2 > 0.0 {
                (((ssr_ols - ssr_aux) / df1) / (ssr_aux / df2)).max(0.0)
            } else {
                0.0
            };

            let p_val = if let Ok(f_dist) = FisherSnedecor::new(df1, df2) {
                (1.0 - f_dist.cdf(f_stat)).clamp(0.0, 1.0)
            } else {
                1.0
            };

            (f_stat, p_val)
        } else {
            // If auxiliary regression is singular/collinear (e.g. u_hat is ~0),
            // Wu-Hausman cannot reject exogeneity.
            (0.0, 1.0)
        }
    } else {
        (0.0, 1.0)
    };

    // B. Sargan Overidentification Test (when L_excluded > K_endog)
    let (sargan_stat, sargan_df, sargan_p) = if l_excluded > k_endog {
        let df_sargan = (l_excluded - k_endog) as i64;
        let res_vec = Mat::<f64>::from_fn(n, 1, |r, _| residuals[r]);
        match solve_ols_faer(&faer_z, &res_vec) {
            Ok((_, ssr_sargan, _)) => {
                let r2_sargan = if ssr > 1e-15 {
                    (1.0 - ssr_sargan / ssr).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                let stat = (n as f64) * r2_sargan;
                let p_val = if let Ok(chi_dist) = ChiSquared::new(df_sargan as f64) {
                    (1.0 - chi_dist.cdf(stat)).clamp(0.0, 1.0)
                } else {
                    1.0
                };

                (Value::F64(stat), Value::I64(df_sargan), Value::F64(p_val))
            }
            Err(_) => (Value::NA(None), Value::I64(df_sargan), Value::NA(None)),
        }
    } else {
        (Value::NA(None), Value::NA(None), Value::NA(None))
    };

    // 11. Build Parameters DataFrame
    let mut term_names = Vec::with_capacity(k_regressors);
    term_names.push("(Intercept)".to_string());
    for t in x_terms {
        term_names.push(t.clone());
    }

    let param_columns: Vec<(String, Vec<Value>)> = vec![
        ("term".to_string(), term_names.iter().map(|s| Value::String(s.clone())).collect()),
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

    // Flatten covariance matrix
    let mut flat_vcov = Vec::with_capacity(k_regressors * k_regressors);
    for r in 0..k_regressors {
        for c in 0..k_regressors {
            flat_vcov.push(vcov_faer[(r, c)]);
        }
    }

    // 12. Package IvResult struct
    let mut fields = BTreeMap::new();
    fields.insert("model".to_string(), Value::String("iv_regress".to_string()));
    fields.insert("response".to_string(), Value::String(y_col.clone()));
    fields.insert(
        "terms".to_string(),
        Value::Vector(VectorData::from_values(term_names.iter().map(|s| Value::String(s.clone())).collect())),
    );
    fields.insert(
        "endogenous".to_string(),
        Value::Vector(VectorData::from_values(x_endog.iter().map(|s| Value::String(s.clone())).collect())),
    );
    fields.insert(
        "instruments".to_string(),
        Value::Vector(VectorData::from_values(z_terms.iter().map(|s| Value::String(s.clone())).collect())),
    );
    fields.insert("coefficients".to_string(), Value::Vector(VectorData::from_f64(beta)));
    fields.insert("std_errors".to_string(), Value::Vector(VectorData::from_f64(std_errors)));
    fields.insert("t_stats".to_string(), Value::Vector(VectorData::from_f64(t_stats)));
    fields.insert("p_values".to_string(), Value::Vector(VectorData::from_f64(p_values)));
    fields.insert("r2".to_string(), Value::F64(r2));
    fields.insert("n_obs".to_string(), Value::I64(n as i64));
    fields.insert("df_resid".to_string(), Value::I64(df_resid));
    fields.insert("vcov_type".to_string(), Value::String(vcov_desc));
    fields.insert(
        "first_stage_f".to_string(),
        if min_f_stat.is_finite() { Value::F64(min_f_stat) } else { Value::NA(None) },
    );
    fields.insert("weak_instruments".to_string(), Value::Bool(is_weak_instruments));
    fields.insert("wu_hausman_stat".to_string(), Value::F64(wu_hausman_stat));
    fields.insert("wu_hausman_p".to_string(), Value::F64(wu_hausman_p));
    fields.insert("sargan_stat".to_string(), sargan_stat);
    fields.insert("sargan_df".to_string(), sargan_df);
    fields.insert("sargan_p".to_string(), sargan_p);
    fields.insert("parameters".to_string(), params_df);
    fields.insert("residuals".to_string(), Value::Vector(VectorData::from_f64(residuals)));
    fields.insert("fitted".to_string(), Value::Vector(VectorData::from_f64(fitted)));
    fields.insert(
        "vcov".to_string(),
        Value::Matrix {
            rows: k_regressors,
            cols: k_regressors,
            data: Arc::new(flat_vcov),
        },
    );

    Ok(Value::Struct {
        name: "IvResult".to_string(),
        fields: Arc::new(fields),
    })
}

// ---------------------------------------------------------------------------
// 4. Native Function Interface (`iv_regress(formula, df, [vcov_type])`)
// ---------------------------------------------------------------------------

/// Native GHL function `iv_regress(formula, df, [vcov_opt])`.
pub fn native_iv_regress(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`iv_regress()` requires at least 2 arguments: `iv_regress(formula, df, [vcov_type])`",
        ));
    }

    let formula_parts = match &args[0] {
        Value::Formula { response, parts, .. } => FormulaParts::new(response.clone(), parts.clone()),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `iv_regress()` must be a Formula, found `{}`", other.type_name()),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `iv_regress()` must be a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };

    let mut vcov_spec = IvVcovSpec::Classical;
    if args.len() >= 3 {
        if let Some(s) = args[2].as_str() {
            let lower = s.to_lowercase();
            if lower == "robust" || lower == "hc1" {
                vcov_spec = IvVcovSpec::Robust;
            }
        }
    }

    fit_iv(&formula_parts, frame, na_reasons, vcov_spec)
}
