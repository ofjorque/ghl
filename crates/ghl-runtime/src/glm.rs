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

use std::collections::HashMap;
use std::fmt;
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps, Sparkline};
use rayon::prelude::*;
use crate::matrix::MatrixOps;
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

/// Invariable GLM fit resulting from IRLS estimation.
#[derive(Debug, Clone, PartialEq)]
pub struct FittedGlm {
    pub blueprint: Blueprint,
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
    /// Fitted probabilities `mu`, not a linear `y_hat` -- this is a probability model.
    pub fitted_values: Vec<f64>,
    /// Deviance residuals (the standard GLM diagnostic residual), not `y - Xb`.
    pub residuals: Vec<f64>,
    pub inv_xtwx: Vec<f64>,
    pub x_data: Vec<f64>,
}

impl FittedGlm {
    /// Fits a binary logistic regression model via IRLS from a formula blueprint and
    /// data. Only the binary-logistic family -- other GLM families (Poisson, binomial
    /// counts, ...) are a separate, unrequested feature (TODO.md).
    pub fn fit_logistic(
        blueprint: Blueprint,
        columns: &[String],
        data: &HashMap<String, Vec<Value>>,
    ) -> Result<Self, Diagnostic> {
        let (x_data, y_data, dispositions, n, p) = blueprint.bake(columns, data)?;

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
        })
    }

    /// Covariance matrix for a given `VcovKind`. `Classical` is the Fisher-information
    /// covariance `(X^T W X)^{-1}` at convergence -- already `inv_xtwx`, no extra work.
    /// `HC0`-`HC3` reuse `neko::sandwich_vcov` with `(X^T W X)^{-1}` as the "bread" and
    /// the GLM score `(y_i - mu_i)^2` as the per-observation weight -- for a canonical
    /// link (logistic regression's is canonical), the score contribution really is
    /// `(y-mu)*x`, occupying exactly the position the squared residual occupies in
    /// OLS's own sandwich formula, so the same helper applies unchanged.
    pub fn compute_vcov(&self, kind: VcovKind) -> Result<Vec<f64>, Diagnostic> {
        let p = self.blueprint.term_names.len();
        match kind {
            VcovKind::Classical => Ok(self.inv_xtwx.clone()),
            VcovKind::HC0 | VcovKind::HC1 | VcovKind::HC2 | VcovKind::HC3 => {
                let sq_score: Vec<f64> = self
                    .fitted_values
                    .iter()
                    .enumerate()
                    .map(|(i, &mu)| {
                        // Recover y_i from the deviance residual's sign and mu (y in
                        // {0,1}, so y - mu determines y's sign relative to mu).
                        let y_i = if self.residuals[i] >= 0.0 { 1.0 } else { 0.0 };
                        (y_i - mu).powi(2)
                    })
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
                let (columns, data) = crate::polars_bridge::dataframe_to_columns_and_data(frame, na_reasons)?;
                let mut new_columns = columns.clone();
                new_columns.push(".fitted".to_string());
                new_columns.push(".residual".to_string());
                new_columns.push(".used_in_fit".to_string());
                new_columns.push(".na_reason".to_string());

                let mut new_data = data.clone();
                let mut fitted_col = Vec::new();
                let mut res_col = Vec::new();
                let mut used_col = Vec::new();
                let mut reason_col = Vec::new();

                let mut fit_idx = 0;
                for disp in &self.dispositions {
                    match disp {
                        RowDisposition::Included => {
                            fitted_col.push(Value::F64(self.fitted_values[fit_idx]));
                            res_col.push(Value::F64(self.residuals[fit_idx]));
                            used_col.push(Value::Bool(true));
                            reason_col.push(Value::String("none".to_string()));
                            fit_idx += 1;
                        }
                        RowDisposition::DroppedNA { reason, .. } => {
                            fitted_col.push(Value::NA(None));
                            res_col.push(Value::NA(None));
                            used_col.push(Value::Bool(false));
                            let r_str = reason.clone().unwrap_or_else(|| "unspecified".to_string());
                            reason_col.push(Value::String(r_str));
                        }
                    }
                }

                new_data.insert(".fitted".to_string(), fitted_col);
                new_data.insert(".residual".to_string(), res_col);
                new_data.insert(".used_in_fit".to_string(), used_col);
                new_data.insert(".na_reason".to_string(), reason_col);

                let cols: Vec<(String, Vec<Value>)> = new_columns.iter()
                    .map(|c| (c.clone(), new_data.remove(c).unwrap_or_default()))
                    .collect();
                let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)?;
                Ok(Value::DataFrame { frame, na_reasons })
            }
            _ => Err(Diagnostic::compute_error(
                "C0201",
                "`augment()` requires a DataFrame as second argument",
            )),
        }
    }

    /// Predicts fitted probabilities (`sigmoid(X*beta)`, not the raw linear predictor)
    /// for a new DataFrame using the frozen `Blueprint`.
    pub fn predict(&self, newdata: &Value) -> Result<Value, Diagnostic> {
        match newdata {
            Value::DataFrame { frame, na_reasons } => {
                let (columns, data) = crate::polars_bridge::dataframe_to_columns_and_data(frame, na_reasons)?;
                let (x_data, _, _, n, p) = self.blueprint.bake(&columns, &data)?;
                let mut predictions = Vec::with_capacity(n);

                for i in 0..n {
                    let mut eta = 0.0;
                    for j in 0..p {
                        eta += x_data[i * p + j] * self.coefficients[j];
                    }
                    predictions.push(Value::F64(sigmoid_stable(eta)));
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
        let mut panel = CockpitPanel::new("NEKO GLM Fit (Logistic)");
        let badge = if caps.unicode_enabled {
            format!("(U・ᴥ・U) IRLS CONVERGED in {} iterations", self.iterations)
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
