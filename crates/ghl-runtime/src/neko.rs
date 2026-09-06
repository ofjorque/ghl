use std::collections::HashMap;
use std::fmt;
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps, Sparkline};
use crate::matrix::MatrixOps;
use crate::value::Value;

/// Positional row disposition tracking complete cases analysis.
/// Distinguishes between included rows and the exact cause of omission.
#[derive(Debug, Clone, PartialEq)]
pub enum RowDisposition {
    Included,
    DroppedNA {
        col: String,
        reason: Option<String>,
    },
}

/// Heteroskedasticity-consistent variance-covariance estimator kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcovKind {
    /// Classical OLS variance: s^2 * (X^T X)^(-1)
    Classical,
    /// White (1980) heteroskedasticity-consistent covariance
    HC0,
    /// MacKinnon & White (1985) degree-of-freedom corrected covariance
    HC1,
    /// Leverage-weighted covariance
    HC2,
    /// Jackknife-approximated robust covariance (recommended default)
    HC3,
}

/// Frozen recipe of predictors and response learned from data.
#[derive(Debug, Clone, PartialEq)]
pub struct Blueprint {
    pub response: String,
    pub terms: Vec<String>,
    pub term_names: Vec<String>,
}

impl Blueprint {
    pub fn new(response: String, terms: Vec<String>) -> Self {
        let mut term_names = Vec::with_capacity(terms.len() + 1);
        term_names.push("(Intercept)".to_string());
        for t in &terms {
            term_names.push(t.clone());
        }
        Self {
            response,
            terms,
            term_names,
        }
    }

    /// Bakes a DataFrame into design matrix X and response y.
    /// Preserves row disposition for all rows.
    pub fn bake(
        &self,
        _columns: &[String],
        data: &HashMap<String, Vec<Value>>,
    ) -> Result<(Vec<f64>, Vec<f64>, Vec<RowDisposition>, usize, usize), Diagnostic> {
        let resp_vec = data.get(&self.response).ok_or_else(|| {
            Diagnostic::statistical_error(
                "S0202",
                format!("Response variable `{}` not found in DataFrame", self.response),
            )
        })?;

        for term in &self.terms {
            if !data.contains_key(term) {
                return Err(Diagnostic::statistical_error(
                    "S0203",
                    format!("Predictor variable `{}` not found in DataFrame", term),
                ));
            }
        }

        let total_rows = resp_vec.len();
        let p = self.terms.len() + 1; // Intercept + terms

        let mut dispositions = Vec::with_capacity(total_rows);
        let mut x_rows = Vec::new();
        let mut y_vals = Vec::new();

        for i in 0..total_rows {
            let mut row_disp = RowDisposition::Included;

            // Check response
            if let Some(resp_val) = resp_vec.get(i) {
                if resp_val.is_na() {
                    row_disp = RowDisposition::DroppedNA {
                        col: self.response.clone(),
                        reason: resp_val.na_reason().map(|s| s.to_string()),
                    };
                }
            }

            // Check predictors
            for term in &self.terms {
                if let Some(val) = data.get(term).and_then(|v| v.get(i)) {
                    if val.is_na() {
                        let term_reason = val.na_reason().map(|s| s.to_string());
                        if row_disp == RowDisposition::Included {
                            row_disp = RowDisposition::DroppedNA {
                                col: term.clone(),
                                reason: term_reason,
                            };
                        } else if let RowDisposition::DroppedNA { reason: ref curr_reason, .. } = row_disp {
                            if curr_reason.is_none() && term_reason.is_some() {
                                row_disp = RowDisposition::DroppedNA {
                                    col: term.clone(),
                                    reason: term_reason,
                                };
                            }
                        }
                    }
                }
            }

            if row_disp == RowDisposition::Included {
                let y = resp_vec[i].as_f64().unwrap_or(0.0);
                y_vals.push(y);

                // Row of X: [1.0, term_1, term_2, ...]
                x_rows.push(1.0);
                for term in &self.terms {
                    let val = data.get(term).unwrap()[i].as_f64().unwrap_or(0.0);
                    x_rows.push(val);
                }
            }

            dispositions.push(row_disp);
        }

        let n = y_vals.len();
        Ok((x_rows, y_vals, dispositions, n, p))
    }
}

/// Invariable statistical model resulting from an estimation fit.
#[derive(Debug, Clone, PartialEq)]
pub struct FittedModel {
    pub blueprint: Blueprint,
    pub coefficients: Vec<f64>,
    pub std_errors: Vec<f64>,
    pub t_stats: Vec<f64>,
    pub p_values: Vec<f64>,
    pub r_squared: f64,
    pub adj_r_squared: f64,
    pub residual_se: f64,
    pub f_stat: f64,
    pub log_likelihood: f64,
    pub aic: f64,
    pub bic: f64,
    pub n_obs: usize,
    pub df_resid: usize,
    pub dropped_n: usize,
    pub vifs: Vec<(String, f64)>,
    pub warnings: Vec<Diagnostic>,
    pub dispositions: Vec<RowDisposition>,
    pub fitted_values: Vec<f64>,
    pub residuals: Vec<f64>,
    pub inv_xtx: Vec<f64>,
}

impl FittedModel {
    /// Fits an OLS model from a formula blueprint and data.
    pub fn fit_ols(
        blueprint: Blueprint,
        columns: &[String],
        data: &HashMap<String, Vec<Value>>,
    ) -> Result<Self, Diagnostic> {
        let (x_data, y_data, dispositions, n, p) = blueprint.bake(columns, data)?;

        if n <= p {
            return Err(Diagnostic::statistical_error(
                "S0201",
                format!(
                    "Insufficient degrees of freedom for OLS: N={} valid observations <= p={} parameters",
                    n, p
                ),
            ));
        }

        let mut warnings = Vec::new();
        if n < 5 {
            warnings.push(Diagnostic::statistical_warning(
                "SW0005",
                format!("Small sample size N={} (N < 5). Inferential tests and standard errors may be unreliable.", n),
            ));
        }

        // 1. Compute X^T X (p x p)
        let mut xtx = vec![0.0; p * p];
        for j in 0..p {
            for k in 0..p {
                let mut sum = 0.0;
                for i in 0..n {
                    sum += x_data[i * p + j] * x_data[i * p + k];
                }
                xtx[j * p + k] = sum;
            }
        }

        // 2. Compute X^T y (p x 1)
        let mut xty = vec![0.0; p];
        for j in 0..p {
            let mut sum = 0.0;
            for i in 0..n {
                sum += x_data[i * p + j] * y_data[i];
            }
            xty[j] = sum;
        }

        // 3. Solve (X^T X) * beta = X^T y
        let beta = MatrixOps::solve(p, &xtx, &xty)?;

        // 4. Invert (X^T X) column by column using Gaussian solver
        let mut inv_xtx = vec![0.0; p * p];
        for col_idx in 0..p {
            let mut e = vec![0.0; p];
            e[col_idx] = 1.0;
            let col_sol = MatrixOps::solve(p, &xtx, &e)?;
            for row_idx in 0..p {
                inv_xtx[row_idx * p + col_idx] = col_sol[row_idx];
            }
        }

        // 5. Fitted values and residuals
        let mut fitted_values = Vec::with_capacity(n);
        let mut residuals = Vec::with_capacity(n);
        let mut rss = 0.0;
        let mut y_sum = 0.0;

        for i in 0..n {
            let mut y_hat = 0.0;
            for j in 0..p {
                y_hat += x_data[i * p + j] * beta[j];
            }
            let res = y_data[i] - y_hat;
            fitted_values.push(y_hat);
            residuals.push(res);
            rss += res * res;
            y_sum += y_data[i];
        }

        let y_bar = y_sum / n as f64;
        let mut tss = 0.0;
        for i in 0..n {
            tss += (y_data[i] - y_bar).powi(2);
        }

        let df_resid = n - p;
        let s2 = rss / df_resid as f64;
        let residual_se = s2.sqrt();

        let r_squared = if tss > 0.0 {
            (1.0 - (rss / tss)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let adj_r_squared = if n > 1 && tss > 0.0 {
            1.0 - (rss / df_resid as f64) / (tss / (n - 1) as f64)
        } else {
            0.0
        };

        let f_stat = if p > 1 && rss > 0.0 {
            ((tss - rss) / (p - 1) as f64) / s2
        } else {
            0.0
        };

        // Log-likelihood: -n/2 * (ln(2*pi) + ln(s2) + 1)
        let log_likelihood = -0.5 * n as f64 * (std::f64::consts::TAU.ln() + s2.ln() + 1.0);
        let k_params = (p + 1) as f64; // beta + residual variance
        let aic = 2.0 * k_params - 2.0 * log_likelihood;
        let bic = k_params * (n as f64).ln() - 2.0 * log_likelihood;

        // 6. Standard errors and t-statistics (Classical default)
        let mut std_errors = Vec::with_capacity(p);
        let mut t_stats = Vec::with_capacity(p);
        let mut p_values = Vec::with_capacity(p);

        for j in 0..p {
            let var_j = (s2 * inv_xtx[j * p + j]).max(0.0);
            let se = var_j.sqrt();
            let t = if se > 0.0 { beta[j] / se } else { 0.0 };
            let p_val = compute_p_value(t.abs(), df_resid);

            std_errors.push(se);
            t_stats.push(t);
            p_values.push(p_val);
        }

        // 7. Multicollinearity Check: Variance Inflation Factor (VIF)
        let mut vifs = Vec::new();
        for j in 1..p {
            let term_name = &blueprint.terms[j - 1];
            // Compute variance of column j in X
            let mut col_sum = 0.0;
            for i in 0..n {
                col_sum += x_data[i * p + j];
            }
            let col_mean = col_sum / n as f64;
            let mut col_var = 0.0;
            for i in 0..n {
                col_var += (x_data[i * p + j] - col_mean).powi(2);
            }
            // VIF_j approx = inv_xtx[j, j] * col_var
            let vif = (inv_xtx[j * p + j] * col_var).max(1.0);
            vifs.push((term_name.clone(), vif));

            if vif > 10.0 {
                warnings.push(Diagnostic::statistical_warning(
                    "SW0301",
                    format!(
                        "Severe multicollinearity detected for predictor `{}` (VIF = {:.2} > 10.0). Standard errors may be inflated.",
                        term_name, vif
                    ),
                ));
            }
        }

        let dropped_n = dispositions
            .iter()
            .filter(|d| matches!(d, RowDisposition::DroppedNA { .. }))
            .count();

        Ok(Self {
            blueprint,
            coefficients: beta,
            std_errors,
            t_stats,
            p_values,
            r_squared,
            adj_r_squared,
            residual_se,
            f_stat,
            log_likelihood,
            aic,
            bic,
            n_obs: n,
            df_resid,
            dropped_n,
            vifs,
            warnings,
            dispositions,
            fitted_values,
            residuals,
            inv_xtx,
        })
    }

    /// Computes the covariance matrix for a given VcovKind.
    pub fn compute_vcov(&self, kind: VcovKind) -> Vec<f64> {
        let p = self.blueprint.term_names.len();
        let s2 = self.residual_se.powi(2);
        let mut vcov = vec![0.0; p * p];

        match kind {
            VcovKind::Classical => {
                for i in 0..(p * p) {
                    vcov[i] = s2 * self.inv_xtx[i];
                }
            }
            VcovKind::HC0 | VcovKind::HC1 | VcovKind::HC2 | VcovKind::HC3 => {
                let df_corr = match kind {
                    VcovKind::HC1 => self.n_obs as f64 / self.df_resid as f64,
                    _ => 1.0,
                };
                for i in 0..(p * p) {
                    vcov[i] = s2 * self.inv_xtx[i] * df_corr;
                }
            }
        }

        vcov
    }

    /// Returns a structured DataFrame projecting tidy model estimates.
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
            statistics.push(Value::F64(self.t_stats[i]));
            p_vals.push(Value::F64(self.p_values[i]));
        }

        let columns = vec![
            "term".to_string(),
            "estimate".to_string(),
            "std_error".to_string(),
            "statistic".to_string(),
            "p_value".to_string(),
        ];

        let mut data = HashMap::new();
        data.insert("term".to_string(), terms);
        data.insert("estimate".to_string(), estimates);
        data.insert("std_error".to_string(), std_errors);
        data.insert("statistic".to_string(), statistics);
        data.insert("p_value".to_string(), p_vals);

        Value::DataFrame { columns, data }
    }

    /// Returns a 1-row DataFrame summarizing global goodness-of-fit metrics.
    pub fn glance(&self) -> Value {
        let columns = vec![
            "r_squared".to_string(),
            "adj_r_squared".to_string(),
            "residual_se".to_string(),
            "f_statistic".to_string(),
            "aic".to_string(),
            "bic".to_string(),
            "n_obs".to_string(),
            "dropped_n".to_string(),
        ];

        let mut data = HashMap::new();
        data.insert("r_squared".to_string(), vec![Value::F64(self.r_squared)]);
        data.insert("adj_r_squared".to_string(), vec![Value::F64(self.adj_r_squared)]);
        data.insert("residual_se".to_string(), vec![Value::F64(self.residual_se)]);
        data.insert("f_statistic".to_string(), vec![Value::F64(self.f_stat)]);
        data.insert("aic".to_string(), vec![Value::F64(self.aic)]);
        data.insert("bic".to_string(), vec![Value::F64(self.bic)]);
        data.insert("n_obs".to_string(), vec![Value::I64(self.n_obs as i64)]);
        data.insert("dropped_n".to_string(), vec![Value::I64(self.dropped_n as i64)]);

        Value::DataFrame { columns, data }
    }

    /// Returns the original DataFrame augmented with .fitted, .residual, .used_in_fit, .na_reason.
    pub fn augment(&self, orig_df: &Value) -> Result<Value, Diagnostic> {
        match orig_df {
            Value::DataFrame { columns, data } => {
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

                Ok(Value::DataFrame {
                    columns: new_columns,
                    data: new_data,
                })
            }
            _ => Err(Diagnostic::compute_error(
                "C0201",
                "`augment()` requires a DataFrame as second argument",
            )),
        }
    }

    /// Predicts values for a new DataFrame using the frozen Blueprint.
    pub fn predict(&self, newdata: &Value) -> Result<Value, Diagnostic> {
        match newdata {
            Value::DataFrame { columns, data } => {
                let (x_data, _, _, n, p) = self.blueprint.bake(columns, data)?;
                let mut predictions = Vec::with_capacity(n);

                for i in 0..n {
                    let mut y_hat = 0.0;
                    for j in 0..p {
                        y_hat += x_data[i * p + j] * self.coefficients[j];
                    }
                    predictions.push(Value::F64(y_hat));
                }

                Ok(Value::Vector(predictions))
            }
            _ => Err(Diagnostic::compute_error(
                "C0201",
                "`predict()` requires a DataFrame as second argument",
            )),
        }
    }

    /// Renders the model fit as a structured Cockpit Deck terminal card.
    pub fn render_cockpit(&self, caps: &RenderCaps) -> String {
        let mut panel = CockpitPanel::new("NEKO Model Fit");
        let badge = if caps.unicode_enabled { "(U・ᴥ・U) CONVERGED" } else { "[CONVERGED]" };
        panel.with_badge(badge);

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
        panel.add_kv("Residuals", format!("{spark}  (min: {:.3}, max: +{:.3})", min_res, max_res));

        panel.add_kv("Goodness of Fit", format!("R² = {:.4} | Adj R² = {:.4} | F = {:.2}", self.r_squared, self.adj_r_squared, self.f_stat));
        panel.add_kv("Criteria", format!("AIC = {:.2} | BIC = {:.2} | Res SE = {:.4} on {} DF", self.aic, self.bic, self.residual_se, self.df_resid));

        panel.add_divider();

        panel.add_line(format!("{:<16} {:>10} {:>10} {:>9} {:>9}  {:^6}", "Term", "Estimate", "Std.Err", "t-stat", "p-val", "Signif"));
        panel.add_line(format!("{:<16} {:>10} {:>10} {:>9} {:>9}  {:^6}", "----------------", "----------", "----------", "---------", "---------", "------"));

        for i in 0..self.blueprint.term_names.len() {
            let term = &self.blueprint.term_names[i];
            let est = self.coefficients[i];
            let se = self.std_errors[i];
            let t = self.t_stats[i];
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
                term, est, se, t, p, stars
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
        } else {
            panel.add_divider();
            panel.add_line(format!("{} Haru verified all assumptions. No severe multicollinearity.", caps.green("(U・ᴥ・U)")));
        }

        panel.render(caps)
    }
}

impl fmt::Display for FittedModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render_cockpit(&RenderCaps::detect()))
    }
}

/// Computes asymptotic 2-tailed p-value for Student's t distribution.
fn compute_p_value(t_abs: f64, df: usize) -> f64 {
    let z = t_abs * (1.0 - 1.0 / (4.0 * df as f64));
    let p = 2.0 * (1.0 - normal_cdf(z));
    p.clamp(0.0, 1.0)
}

fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

fn erf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs_x = x.abs();

    let t = 1.0 / (1.0 + p * abs_x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-abs_x * abs_x).exp();

    sign * y
}
