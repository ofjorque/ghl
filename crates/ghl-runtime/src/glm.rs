//! GLM (logistic regression) via IRLS -- TODO.md Fase 6, Caso 3.2.
//!
//! Deliberately a separate type from `neko::FittedModel`, not a reuse of it: OLS's
//! diagnostics (`r_squared`, `f_stat`, t-statistics with a df correction) don't have a
//! correct, direct analog for logistic regression -- forcing values into those fields
//! would be exactly the kind of statistically meaningless "silent state" this project
//! avoids elsewhere. `FittedGlm` reports its own, GLM-appropriate diagnostics (deviance,
//! McFadden pseudo-R², Wald z-statistics) while reusing everything from `neko` that
//! *is* shared machinery: `Blueprint::bake()` for the design matrix, `MatrixOps::solve`
//! for the linear system each IRLS iteration, `assemble_weighted_normal_equations` for
//! the parallelized O(n*p^2) assembly, `sandwich_vcov` for robust covariance, and
//! `normal_cdf` for p-values.

use std::fmt;
use std::sync::Arc;
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps, Sparkline};
use polars_core::frame::DataFrame;
use rayon::prelude::*;
use crate::matrix::MatrixOps;
use crate::na_reasons::NaReasonTable;
use crate::neko::{assemble_weighted_normal_equations, normal_cdf, sandwich_vcov, Blueprint, RowDisposition, VcovKind};
use crate::value::Value;
use crate::vector_data::VectorData;

const MAX_IRLS_ITER: usize = 25;
const IRLS_TOL: f64 = 1e-8;
/// Keeps `mu` away from exactly 0/1 -- otherwise `W = mu(1-mu)` hits exactly 0 (making
/// `X^T W X` singular) and `ln(mu)`/`ln(1-mu)` in the deviance blow up to `-inf`.
const MU_EPS: f64 = 1e-10;

/// Numerically stable logistic sigmoid: naively computing `1/(1+exp(-x))` overflows
/// `exp()` for very negative `x` (large positive argument to `exp`). Standard trick:
/// for `x >= 0` compute it directly (exp's argument stays <= 0, no overflow risk); for
/// `x < 0`, rewrite as `exp(x)/(1+exp(x))` (exp's argument is negative there instead).
fn sigmoid_stable(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

/// Bernoulli log-likelihood contribution for a single observation, used by both the
/// per-iteration convergence bookkeeping and the final deviance -- `mu` is clamped away
/// from 0/1 first so `ln(0)` never happens even when a fit is nearly separating.
fn bernoulli_ll_term(y: f64, mu: f64) -> f64 {
    let mu = mu.clamp(MU_EPS, 1.0 - MU_EPS);
    y * mu.ln() + (1.0 - y) * (1.0 - mu).ln()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlmFamily {
    Binomial,
    Poisson,
}

impl fmt::Display for GlmFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GlmFamily::Binomial => write!(f, "Binomial(logit)"),
            GlmFamily::Poisson => write!(f, "Poisson(log)"),
        }
    }
}

/// Invariable GLM fit resulting from IRLS estimation.
#[derive(Debug, Clone, PartialEq)]
pub struct FittedGlm {
    pub blueprint: Blueprint,
    pub family: GlmFamily,
    pub coefficients: Vec<f64>,
    pub std_errors: Vec<f64>,
    pub z_stats: Vec<f64>,
    pub p_values: Vec<f64>,
    pub deviance: f64,
    pub null_deviance: f64,
    pub pseudo_r_squared: f64,
    pub aic: f64,
    pub bic: f64,
    pub n_obs: usize,
    pub df_resid: usize,
    pub dropped_n: usize,
    pub iterations: usize,
    pub warnings: Vec<Diagnostic>,
    pub dispositions: Vec<RowDisposition>,
    /// Fitted expected values `mu` (probabilities for Binomial, mean counts for Poisson).
    pub fitted_values: Vec<f64>,
    /// Deviance residuals (the standard GLM diagnostic residual), not `y - Xb`.
    pub residuals: Vec<f64>,
    pub inv_xtwx: Vec<f64>,
    pub x_data: Vec<f64>,
    pub y_data: Vec<f64>,
}

impl FittedGlm {
    /// Fits a binary logistic regression model via IRLS from a formula blueprint and
    /// data. Only the binary-logistic family -- other GLM families (Poisson, binomial
    /// counts, ...) are a separate, unrequested feature (TODO.md).
    pub fn fit_logistic(
        blueprint: Blueprint,
        frame: &DataFrame,
        na_reasons: &NaReasonTable,
    ) -> Result<Self, Diagnostic> {
        let (x_data, y_data, dispositions, n, p, baked_term_names, baked_term_levels) = blueprint.bake(frame, na_reasons)?;
        let mut blueprint = blueprint;
        blueprint.term_names = baked_term_names;
        blueprint.term_levels = baked_term_levels;

        if n <= p {
            return Err(Diagnostic::statistical_error(
                "S0201",
                format!(
                    "Insufficient degrees of freedom for GLM: N={} valid observations <= p={} parameters",
                    n, p
                ),
            ));
        }

        for &y in &y_data {
            if y != 0.0 && y != 1.0 {
                return Err(Diagnostic::statistical_error(
                    "S0204",
                    format!(
                        "`fit_logistic()` requires a binary 0/1 response, found `{}`. \
                         Other GLM families (Poisson, binomial counts, ...) aren't supported yet.",
                        y
                    ),
                ));
            }
        }

        let mut warnings = Vec::new();
        if n < 5 {
            warnings.push(Diagnostic::statistical_warning(
                "SW0005",
                format!("Small sample size N={} (N < 5). Inferential tests and standard errors may be unreliable.", n),
            ));
        }

        let mut beta = vec![0.0; p];
        let mut iterations = 0;
        let mut converged = false;

        for iter in 1..=MAX_IRLS_ITER {
            iterations = iter;

            // Working response `z` and weight `W` per observation, from the current beta.
            let compute_row = |i: usize| -> (f64, f64) {
                let row = &x_data[i * p..(i + 1) * p];
                let eta: f64 = row.iter().zip(&beta).map(|(&x, &b)| x * b).sum();
                let mu = sigmoid_stable(eta);
                let w = (mu * (1.0 - mu)).max(MU_EPS);
                let z = eta + (y_data[i] - mu) / w;
                (w, z)
            };
            let (weights, z): (Vec<f64>, Vec<f64>) = if n >= crate::eval::PARALLEL_THRESHOLD {
                (0..n).into_par_iter().map(compute_row).unzip()
            } else {
                (0..n).map(compute_row).unzip()
            };

            let (xtwx, xtwz) = assemble_weighted_normal_equations(n, p, &x_data, Some(&weights), &z);
            let beta_new = MatrixOps::solve(p, &xtwx, &xtwz)?;

            let max_delta = beta_new
                .iter()
                .zip(&beta)
                .map(|(&a, &b)| (a - b).abs())
                .fold(0.0_f64, f64::max);
            beta = beta_new;

            if max_delta < IRLS_TOL {
                converged = true;
                break;
            }
        }

        if !converged {
            return Err(Diagnostic::statistical_error(
                "S0205",
                format!("IRLS did not converge in {} iterations for `fit_logistic()`", MAX_IRLS_ITER),
            ));
        }

        // Final fitted probabilities, deviance residuals, and deviance -- one more pass
        // over the converged beta (not parallelized: O(n*p), not O(n*p^2), and only
        // runs once, not once per iteration).
        let mut fitted_values = Vec::with_capacity(n);
        let mut residuals = Vec::with_capacity(n);
        let mut deviance = 0.0;
        for i in 0..n {
            let row = &x_data[i * p..(i + 1) * p];
            let eta: f64 = row.iter().zip(&beta).map(|(&x, &b)| x * b).sum();
            let mu = sigmoid_stable(eta);
            let ll_i = bernoulli_ll_term(y_data[i], mu);
            deviance += -2.0 * ll_i;
            let dev_resid = (y_data[i] - mu).signum() * (-2.0 * ll_i).max(0.0).sqrt();
            fitted_values.push(mu);
            residuals.push(dev_resid);
        }

        // Null model: intercept-only fit, mu is the constant y_bar for every row (closed
        // form, no IRLS needed) -- deviance still sums bernoulli_ll_term per observation
        // since y_i itself varies row to row even though mu doesn't.
        let y_bar = y_data.iter().sum::<f64>() / n as f64;
        let null_deviance: f64 = y_data.iter().map(|&y| -2.0 * bernoulli_ll_term(y, y_bar)).sum();

        let df_resid = n - p;
        let pseudo_r_squared = if null_deviance > 0.0 {
            (1.0 - deviance / null_deviance).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let aic = deviance + 2.0 * p as f64;
        let bic = deviance + p as f64 * (n as f64).ln();

        // Invert (X^T W X) at convergence, same column-by-column trick as OLS
        // (`FittedModel::fit_ols`), reusing `MatrixOps::solve`. Needs its own weight
        // pass at the *converged* beta -- the loop's last `weights` were computed from
        // the second-to-last iterate, not this final one, and Fisher information must
        // be evaluated at the converged estimate for correct inference.
        let final_weight_at_row = |i: usize| -> f64 {
            let row = &x_data[i * p..(i + 1) * p];
            let eta: f64 = row.iter().zip(&beta).map(|(&x, &b)| x * b).sum();
            let mu = sigmoid_stable(eta);
            (mu * (1.0 - mu)).max(MU_EPS)
        };
        let final_weights: Vec<f64> = if n >= crate::eval::PARALLEL_THRESHOLD {
            (0..n).into_par_iter().map(final_weight_at_row).collect()
        } else {
            (0..n).map(final_weight_at_row).collect()
        };
        let (final_xtwx, _) = assemble_weighted_normal_equations(n, p, &x_data, Some(&final_weights), &vec![0.0; n]);
        let mut inv_xtwx = vec![0.0; p * p];
        for col_idx in 0..p {
            let mut e = vec![0.0; p];
            e[col_idx] = 1.0;
            let col_sol = MatrixOps::solve(p, &final_xtwx, &e)?;
            for row_idx in 0..p {
                inv_xtwx[row_idx * p + col_idx] = col_sol[row_idx];
            }
        }

        let mut std_errors = Vec::with_capacity(p);
        let mut z_stats = Vec::with_capacity(p);
        let mut p_values = Vec::with_capacity(p);
        for j in 0..p {
            let se = inv_xtwx[j * p + j].max(0.0).sqrt();
            let z = if se > 0.0 { beta[j] / se } else { 0.0 };
            let p_val = 2.0 * (1.0 - normal_cdf(z.abs()));
            std_errors.push(se);
            z_stats.push(z);
            p_values.push(p_val.clamp(0.0, 1.0));
        }

        let dropped_n = dispositions
            .iter()
            .filter(|d| matches!(d, RowDisposition::DroppedNA { .. }))
            .count();

        Ok(Self {
            blueprint,
            coefficients: beta,
            std_errors,
            z_stats,
            p_values,
            deviance,
            null_deviance,
            pseudo_r_squared,
            aic,
            bic,
            n_obs: n,
            df_resid,
            dropped_n,
            iterations,
            warnings,
            dispositions,
            fitted_values,
            residuals,
            inv_xtwx,
            x_data,
            y_data,
            family: GlmFamily::Binomial,
        })
    }

    /// Fits a Poisson regression model (log link) via IRLS from a formula blueprint and data.
    pub fn fit_poisson(
        blueprint: Blueprint,
        frame: &DataFrame,
        na_reasons: &NaReasonTable,
    ) -> Result<Self, Diagnostic> {
        let (x_data, y_data, dispositions, n, p, baked_term_names, baked_term_levels) = blueprint.bake(frame, na_reasons)?;
        let mut blueprint = blueprint;
        blueprint.term_names = baked_term_names;
        blueprint.term_levels = baked_term_levels;

        if n <= p {
            return Err(Diagnostic::statistical_error(
                "S0201",
                format!(
                    "Insufficient degrees of freedom for GLM Poisson: N={} valid observations <= p={} parameters",
                    n, p
                ),
            ));
        }

        for &y in &y_data {
            if y < 0.0 {
                return Err(Diagnostic::statistical_error(
                    "S0204",
                    format!(
                        "`poisson()` requires non-negative count response, found `{}`.",
                        y
                    ),
                ));
            }
        }

        let mut warnings = Vec::new();
        if n < 5 {
            warnings.push(Diagnostic::statistical_warning(
                "SW0005",
                format!("Small sample size N={} (N < 5). Inferential tests and standard errors may be unreliable.", n),
            ));
        }

        let y_bar = y_data.iter().sum::<f64>() / n as f64;
        let mut beta = vec![0.0; p];
        if p > 0 {
            beta[0] = (y_bar.max(0.1)).ln();
        }

        let mut iterations = 0;
        let mut converged = false;

        for iter in 1..=MAX_IRLS_ITER {
            iterations = iter;

            let compute_row = |i: usize| -> (f64, f64) {
                let row = &x_data[i * p..(i + 1) * p];
                let eta: f64 = row.iter().zip(&beta).map(|(&x, &b)| x * b).sum();
                let eta_clamped = eta.clamp(-30.0, 30.0);
                let mu = eta_clamped.exp().max(MU_EPS);
                let w = mu;
                let z = eta + (y_data[i] - mu) / mu;
                (w, z)
            };

            let (weights, z): (Vec<f64>, Vec<f64>) = if n >= crate::eval::PARALLEL_THRESHOLD {
                (0..n).into_par_iter().map(compute_row).unzip()
            } else {
                (0..n).map(compute_row).unzip()
            };

            let (xtwx, xtwz) = assemble_weighted_normal_equations(n, p, &x_data, Some(&weights), &z);
            let beta_new = MatrixOps::solve(p, &xtwx, &xtwz)?;

            let max_delta = beta_new
                .iter()
                .zip(&beta)
                .map(|(&a, &b)| (a - b).abs())
                .fold(0.0_f64, f64::max);
            beta = beta_new;

            if max_delta < IRLS_TOL {
                converged = true;
                break;
            }
        }

        if !converged {
            return Err(Diagnostic::statistical_error(
                "S0205",
                format!("IRLS did not converge in {} iterations for `poisson()`", MAX_IRLS_ITER),
            ));
        }

        let mut fitted_values = Vec::with_capacity(n);
        let mut residuals = Vec::with_capacity(n);
        let mut deviance = 0.0;
        for i in 0..n {
            let row = &x_data[i * p..(i + 1) * p];
            let eta: f64 = row.iter().zip(&beta).map(|(&x, &b)| x * b).sum();
            let mu = eta.clamp(-30.0, 30.0).exp().max(MU_EPS);
            let y = y_data[i];
            let d_i = if y > 0.0 {
                2.0 * (y * (y / mu).ln() - (y - mu))
            } else {
                2.0 * mu
            };
            deviance += d_i;
            let dev_resid = (y - mu).signum() * d_i.max(0.0).sqrt();
            fitted_values.push(mu);
            residuals.push(dev_resid);
        }

        let mu_null = y_bar.max(MU_EPS);
        let null_deviance: f64 = y_data.iter().map(|&y| {
            if y > 0.0 {
                2.0 * (y * (y / mu_null).ln() - (y - mu_null))
            } else {
                2.0 * mu_null
            }
        }).sum();

        let df_resid = n - p;
        let pseudo_r_squared = if null_deviance > 0.0 {
            (1.0 - deviance / null_deviance).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let aic = deviance + 2.0 * p as f64;
        let bic = deviance + p as f64 * (n as f64).ln();

        let final_weight_at_row = |i: usize| -> f64 {
            let row = &x_data[i * p..(i + 1) * p];
            let eta: f64 = row.iter().zip(&beta).map(|(&x, &b)| x * b).sum();
            eta.clamp(-30.0, 30.0).exp().max(MU_EPS)
        };
        let final_weights: Vec<f64> = if n >= crate::eval::PARALLEL_THRESHOLD {
            (0..n).into_par_iter().map(final_weight_at_row).collect()
        } else {
            (0..n).map(final_weight_at_row).collect()
        };
        let (final_xtwx, _) = assemble_weighted_normal_equations(n, p, &x_data, Some(&final_weights), &vec![0.0; n]);
        let mut inv_xtwx = vec![0.0; p * p];
        for col_idx in 0..p {
            let mut e = vec![0.0; p];
            e[col_idx] = 1.0;
            let col_sol = MatrixOps::solve(p, &final_xtwx, &e)?;
            for row_idx in 0..p {
                inv_xtwx[row_idx * p + col_idx] = col_sol[row_idx];
            }
        }

        let mut std_errors = Vec::with_capacity(p);
        let mut z_stats = Vec::with_capacity(p);
        let mut p_values = Vec::with_capacity(p);
        for j in 0..p {
            let se = inv_xtwx[j * p + j].max(0.0).sqrt();
            let z = if se > 0.0 { beta[j] / se } else { 0.0 };
            let p_val = 2.0 * (1.0 - normal_cdf(z.abs()));
            std_errors.push(se);
            z_stats.push(z);
            p_values.push(p_val.clamp(0.0, 1.0));
        }

        let dropped_n = dispositions
            .iter()
            .filter(|d| matches!(d, RowDisposition::DroppedNA { .. }))
            .count();

        Ok(Self {
            blueprint,
            family: GlmFamily::Poisson,
            coefficients: beta,
            std_errors,
            z_stats,
            p_values,
            deviance,
            null_deviance,
            pseudo_r_squared,
            aic,
            bic,
            n_obs: n,
            df_resid,
            dropped_n,
            iterations,
            warnings,
            dispositions,
            fitted_values,
            residuals,
            inv_xtwx,
            x_data,
            y_data,
        })
    }

    /// Covariance matrix for a given `VcovKind`. `Classical` is the Fisher-information
    /// covariance `(X^T W X)^{-1}` at convergence -- already `inv_xtwx`, no extra work.
    /// `HC0`-`HC3` reuse `neko::sandwich_vcov` with `(X^T W X)^{-1}` as the "bread" and
    /// the GLM score `(y_i - mu_i)^2` as the per-observation weight -- for canonical
    /// links (logistic and Poisson), the score contribution is `(y - mu) * x`.
    pub fn compute_vcov(&self, kind: VcovKind) -> Result<Vec<f64>, Diagnostic> {
        let p = self.blueprint.term_names.len();
        match kind {
            VcovKind::Classical => Ok(self.inv_xtwx.clone()),
            VcovKind::HC0 | VcovKind::HC1 | VcovKind::HC2 | VcovKind::HC3 => {
                let sq_score: Vec<f64> = self
                    .y_data
                    .iter()
                    .zip(&self.fitted_values)
                    .map(|(&y, &mu)| (y - mu).powi(2))
                    .collect();
                sandwich_vcov(p, self.n_obs, self.df_resid, &self.x_data, &sq_score, &self.inv_xtwx, kind)
            }
        }
    }

    pub fn tidy(&self) -> Value {
        let mut terms = Vec::new();
        let mut estimates = Vec::new();
        let mut std_errors = Vec::new();
        let mut statistics = Vec::new();
        let mut p_vals = Vec::new();

        for i in 0..self.blueprint.term_names.len() {
            terms.push(Value::String(self.blueprint.term_names[i].clone()));
            estimates.push(Value::F64(self.coefficients[i]));
            std_errors.push(Value::F64(self.std_errors[i]));
            statistics.push(Value::F64(self.z_stats[i]));
            p_vals.push(Value::F64(self.p_values[i]));
        }

        let cols = vec![
            ("term".to_string(), terms),
            ("estimate".to_string(), estimates),
            ("std_error".to_string(), std_errors),
            ("statistic".to_string(), statistics),
            ("p_value".to_string(), p_vals),
        ];

        let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)
            .expect("tidy() builds a DataFrame from well-formed numeric/string vectors");
        Value::DataFrame { frame, na_reasons }
    }

    pub fn glance(&self) -> Value {
        let cols = vec![
            ("deviance".to_string(), vec![Value::F64(self.deviance)]),
            ("null_deviance".to_string(), vec![Value::F64(self.null_deviance)]),
            ("pseudo_r_squared".to_string(), vec![Value::F64(self.pseudo_r_squared)]),
            ("aic".to_string(), vec![Value::F64(self.aic)]),
            ("bic".to_string(), vec![Value::F64(self.bic)]),
            ("n_obs".to_string(), vec![Value::I64(self.n_obs as i64)]),
            ("dropped_n".to_string(), vec![Value::I64(self.dropped_n as i64)]),
            ("iterations".to_string(), vec![Value::I64(self.iterations as i64)]),
        ];

        let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)
            .expect("glance() builds a DataFrame from well-formed numeric vectors");
        Value::DataFrame { frame, na_reasons }
    }

    pub fn augment(&self, orig_df: &Value) -> Result<Value, Diagnostic> {
        match orig_df {
            Value::DataFrame { frame, na_reasons } => {
                let mut fitted_col = Vec::with_capacity(self.dispositions.len());
                let mut res_col = Vec::with_capacity(self.dispositions.len());
                let mut used_col = Vec::with_capacity(self.dispositions.len());
                let mut reason_col = Vec::with_capacity(self.dispositions.len());

                let mut fit_idx = 0;
                for disp in &self.dispositions {
                    match disp {
                        RowDisposition::Included => {
                            fitted_col.push(Some(self.fitted_values[fit_idx]));
                            res_col.push(Some(self.residuals[fit_idx]));
                            used_col.push(true);
                            reason_col.push("none".to_string());
                            fit_idx += 1;
                        }
                        RowDisposition::DroppedNA { reason, .. } => {
                            fitted_col.push(None);
                            res_col.push(None);
                            used_col.push(false);
                            reason_col.push(reason.clone().unwrap_or_else(|| "unspecified".to_string()));
                        }
                    }
                }

                let mut new_frame = frame.clone();
                new_frame.with_column(crate::polars_bridge::f64_opt_column(".fitted", fitted_col)).map_err(|e| {
                    Diagnostic::compute_error("C0210", format!("`augment()` failed: {e}"))
                })?;
                new_frame.with_column(crate::polars_bridge::f64_opt_column(".residual", res_col)).map_err(|e| {
                    Diagnostic::compute_error("C0210", format!("`augment()` failed: {e}"))
                })?;
                new_frame.with_column(crate::polars_bridge::bool_column(".used_in_fit", used_col)).map_err(|e| {
                    Diagnostic::compute_error("C0210", format!("`augment()` failed: {e}"))
                })?;
                new_frame.with_column(crate::polars_bridge::string_column(".na_reason", reason_col)).map_err(|e| {
                    Diagnostic::compute_error("C0210", format!("`augment()` failed: {e}"))
                })?;

                Ok(Value::DataFrame { frame: new_frame, na_reasons: Arc::clone(na_reasons) })
            }
            _ => Err(Diagnostic::compute_error(
                "C0201",
                "`augment()` requires a DataFrame as second argument",
            )),
        }
    }

    /// Predicts fitted values on the response scale (`sigmoid(X*beta)` for Binomial,
    /// `exp(X*beta)` for Poisson, not the raw linear predictor) for a new DataFrame
    /// using the frozen `Blueprint`.
    pub fn predict(&self, newdata: &Value) -> Result<Value, Diagnostic> {
        match newdata {
            Value::DataFrame { frame, na_reasons } => {
                let temp_frame;
                let frame_ref = if frame.column(&self.blueprint.response).is_err() {
                    let mut cloned = frame.clone();
                    cloned
                        .with_column(crate::polars_bridge::f64_opt_column(
                            &self.blueprint.response,
                            vec![Some(0.0); frame.height()],
                        ))
                        .map_err(|e| {
                            Diagnostic::compute_error(
                                "C0210",
                                format!("Failed to create dummy response column for predict: {e}"),
                            )
                        })?;
                    temp_frame = cloned;
                    &temp_frame
                } else {
                    frame
                };
                let (x_data, _, dispositions, _, p, _, _) = self.blueprint.bake(frame_ref, na_reasons)?;
                let mut predictions = Vec::with_capacity(dispositions.len());
                let mut included_idx = 0;

                for disp in &dispositions {
                    match disp {
                        RowDisposition::Included => {
                            let mut eta = 0.0;
                            for j in 0..p {
                                eta += x_data[included_idx * p + j] * self.coefficients[j];
                            }
                            let pred = match self.family {
                                GlmFamily::Binomial => sigmoid_stable(eta),
                                GlmFamily::Poisson => eta.clamp(-30.0, 30.0).exp(),
                            };
                            predictions.push(Value::F64(pred));
                            included_idx += 1;
                        }
                        RowDisposition::DroppedNA { reason, .. } => {
                            predictions.push(Value::NA(reason.clone()));
                        }
                    }
                }

                Ok(Value::Vector(VectorData::from_values(predictions)))
            }
            _ => Err(Diagnostic::compute_error(
                "C0201",
                "`predict()` requires a DataFrame as second argument",
            )),
        }
    }

    pub fn render_cockpit(&self, caps: &RenderCaps) -> String {
        let family_title = match self.family {
            GlmFamily::Binomial => "NEKO GLM Fit (Logistic)",
            GlmFamily::Poisson => "NEKO GLM Fit (Poisson)",
        };
        let mut panel = CockpitPanel::new(family_title);
        let badge = if caps.unicode_enabled {
            format!("/ᐠ˵- ⩊ -˵マ ✧ IRLS CONVERGED in {} iterations", self.iterations)
        } else {
            format!("[CONVERGED in {} iterations]", self.iterations)
        };

        panel.with_badge(&badge);

        panel.add_kv("Formula", format!("{} ~ {}", self.blueprint.response, self.blueprint.terms.join(" + ")));

        let obs_text = if self.dropped_n > 0 {
            format!("{} valid ({} dropped due to NA)", self.n_obs, self.dropped_n)
        } else {
            format!("{} valid", self.n_obs)
        };
        panel.add_kv("Observations", obs_text);

        let spark = Sparkline::render(&self.residuals, Some(16), caps);
        let min_res = self.residuals.iter().copied().fold(f64::INFINITY, f64::min);
        let max_res = self.residuals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        panel.add_kv("Deviance Residuals", format!("{spark}  (min: {:.3}, max: +{:.3})", min_res, max_res));

        panel.add_kv(
            "Goodness of Fit",
            format!("Deviance = {:.4} | Null Deviance = {:.4} | Pseudo R2 = {:.4}", self.deviance, self.null_deviance, self.pseudo_r_squared),
        );
        panel.add_kv("Criteria", format!("AIC = {:.2} | BIC = {:.2} on {} DF", self.aic, self.bic, self.df_resid));

        panel.add_divider();

        panel.add_line(format!("{:<16} {:>10} {:>10} {:>9} {:>9}  {:^6}", "Term", "Estimate", "Std.Err", "z-stat", "p-val", "Signif"));
        panel.add_line(format!("{:<16} {:>10} {:>10} {:>9} {:>9}  {:^6}", "----------------", "----------", "----------", "---------", "---------", "------"));

        for i in 0..self.blueprint.term_names.len() {
            let term = &self.blueprint.term_names[i];
            let est = self.coefficients[i];
            let se = self.std_errors[i];
            let z = self.z_stats[i];
            let p = self.p_values[i];
            let stars = if p < 0.001 {
                "***"
            } else if p < 0.01 {
                "**"
            } else if p < 0.05 {
                "*"
            } else if p < 0.1 {
                "."
            } else {
                " "
            };

            panel.add_line(format!(
                "{:<16} {:>10.4} {:>10.4} {:>9.2} {:>9.4}  {:^6}",
                term, est, se, z, p, stars
            ));
        }

        panel.add_divider();
        panel.add_line(caps.dim("Signif. codes:  0 '***' 0.001 '**' 0.01 '*' 0.05 '.' 0.1 ' ' 1"));

        if !self.warnings.is_empty() {
            panel.add_divider();
            panel.add_line(caps.yellow(&caps.bold("Diagnostic Warnings:")));
            for w in &self.warnings {
                panel.add_line(w.render_with_caps(caps));
            }
        }

        panel.render(caps)
    }
}

impl fmt::Display for FittedGlm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render_cockpit(&RenderCaps::detect()))
    }
}
