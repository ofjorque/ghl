//! GMM (Gaussian Mixture Model) via EM -- NEKO Statistical Modeling Framework
//!
//! Provides native, high-performance Expectation-Maximization estimation for multivariate
//! Gaussian mixtures with diagonal covariance, integrated into NEKO:
//! - Visual cockpit in console with Kaomojis and ASCII tables (`render_cockpit`)
//! - Information criteria: log-likelihood, AIC, BIC
//! - Semantic projections: `summary()`, `tidy()`, `glance()`, `augment()`, `predict()`, `coef()`

use crate::value::Value;
use crate::vector_data::VectorData;
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps};
use std::fmt;
use std::sync::Arc;

const DEFAULT_MAX_ITER: usize = 100;
const DEFAULT_TOL: f64 = 1e-6;
const RIDGE_EPS: f64 = 1e-4;

/// Invariable GMM fit resulting from EM estimation.
#[derive(Debug, Clone, PartialEq)]
pub struct FittedGmm {
    pub k: usize,
    pub dim: usize,
    pub n_obs: usize,
    pub column_names: Vec<String>,
    pub weights: Vec<f64>,
    pub means: Vec<f64>,
    pub variances: Vec<f64>,
    pub log_likelihood: f64,
    pub aic: f64,
    pub bic: f64,
    pub iterations: usize,
    pub converged: bool,
    pub cluster_assignments: Vec<i64>,
    pub max_posterior_probs: Vec<f64>,
    pub responsibilities: Vec<f64>,
}

impl FittedGmm {
    /// Fits a Gaussian Mixture Model from either a DataFrame or a Matrix.
    pub fn fit(
        data_val: &Value,
        k: usize,
        max_iter: Option<usize>,
        tol: Option<f64>,
    ) -> Result<Self, Diagnostic> {
        if k == 0 {
            return Err(Diagnostic::statistical_error(
                "S0201",
                "`fit_gmm()`: number of clusters K must be >= 1",
            ));
        }

        let max_iter = max_iter.unwrap_or(DEFAULT_MAX_ITER);
        let tol = tol.unwrap_or(DEFAULT_TOL);

        let (x_data, n_obs, dim, col_names) = match data_val {
            Value::Matrix { rows, cols, data } => {
                let names: Vec<String> = (0..*cols).map(|d| format!("V{d}")).collect();
                (data.clone(), *rows, *cols, names)
            }
            Value::DataFrame {
                frame,
                na_reasons: _,
            } => {
                let n = frame.height();
                let cols = frame.columns();
                let mut valid_col_names = Vec::new();
                let mut col_views = Vec::new();

                for c in cols {
                    if let Ok(view) = crate::vector_data::column_as_f64_view(c) {
                        valid_col_names.push(c.name().to_string());
                        col_views.push(view);
                    }
                }

                let d = valid_col_names.len();
                if d == 0 {
                    return Err(Diagnostic::statistical_error(
                        "S0200",
                        "`fit_gmm()` requires numeric columns in DataFrame",
                    ));
                }

                let mut row_major = Vec::with_capacity(n * d);
                for i in 0..n {
                    for j in 0..d {
                        row_major.push(col_views[j].as_slice()[i]);
                    }
                }

                (std::sync::Arc::new(row_major), n, d, valid_col_names)
            }
            other => {
                return Err(Diagnostic::statistical_error(
                    "S0200",
                    format!(
                        "First argument of `fit_gmm()` must be a DataFrame or Matrix, found `{}`",
                        other.type_name()
                    ),
                ));
            }
        };

        if n_obs < k {
            return Err(Diagnostic::statistical_error(
                "S0201",
                format!(
                    "`fit_gmm()`: number of observations ({n_obs}) must be >= number of clusters ({k})"
                ),
            ));
        }

        // --- Initialization ---
        let mut weights = vec![1.0 / (k as f64); k];
        let mut means = vec![0.0; k * dim];
        let step = n_obs / k;
        for ki in 0..k {
            let sample_idx = (ki * step).min(n_obs - 1);
            for di in 0..dim {
                means[ki * dim + di] = x_data[sample_idx * dim + di];
            }
        }

        // Initialize variances to sample variance per dimension across all observations
        let mut vars = vec![1.0; k * dim];
        for di in 0..dim {
            let mut sum_d = 0.0;
            let mut sum_sq_d = 0.0;
            for i in 0..n_obs {
                let val = x_data[i * dim + di];
                sum_d += val;
                sum_sq_d += val * val;
            }
            let mean_d = sum_d / (n_obs as f64);
            let var_d = ((sum_sq_d / (n_obs as f64)) - mean_d * mean_d).max(RIDGE_EPS);
            for ki in 0..k {
                vars[ki * dim + di] = var_d;
            }
        }

        let two_pi = std::f64::consts::PI * 2.0;
        let log_two_pi_d = (dim as f64) * two_pi.ln();

        let mut gamma = vec![0.0; n_obs * k];
        let mut prev_ll = f64::NEG_INFINITY;
        let mut total_ll = f64::NEG_INFINITY;
        let mut converged = false;
        let mut iters_done = 0;

        for iter in 0..max_iter {
            iters_done = iter + 1;
            total_ll = 0.0;

            // --- E-step ---
            for i in 0..n_obs {
                let xi = &x_data[i * dim..(i + 1) * dim];
                let mut log_p = vec![0.0; k];

                for ki in 0..k {
                    let muk = &means[ki * dim..(ki + 1) * dim];
                    let vark = &vars[ki * dim..(ki + 1) * dim];
                    let mut mahal = 0.0;
                    let mut log_det = 0.0;

                    for di in 0..dim {
                        let diff = xi[di] - muk[di];
                        let v = vark[di].max(RIDGE_EPS);
                        mahal += (diff * diff) / v;
                        log_det += v.ln();
                    }

                    let log_gauss = -0.5 * (log_two_pi_d + log_det + mahal);
                    log_p[ki] = weights[ki].ln() + log_gauss;
                }

                // log-sum-exp
                let max_lp = log_p.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let sum_exp: f64 = log_p.iter().map(|&lp| (lp - max_lp).exp()).sum();
                let lse = max_lp + sum_exp.ln();
                total_ll += lse;

                for ki in 0..k {
                    gamma[i * k + ki] = (log_p[ki] - lse).exp();
                }
            }

            if (total_ll - prev_ll).abs() < tol {
                converged = true;
                break;
            }
            prev_ll = total_ll;

            // --- M-step ---
            for ki in 0..k {
                let mut n_k = 0.0;
                for i in 0..n_obs {
                    n_k += gamma[i * k + ki];
                }
                n_k = n_k.max(1e-10);
                weights[ki] = n_k / (n_obs as f64);

                // Update means
                for di in 0..dim {
                    let mut sum_x = 0.0;
                    for i in 0..n_obs {
                        sum_x += gamma[i * k + ki] * x_data[i * dim + di];
                    }
                    means[ki * dim + di] = sum_x / n_k;
                }

                // Update variances
                for di in 0..dim {
                    let mut sum_sq = 0.0;
                    let muk_d = means[ki * dim + di];
                    for i in 0..n_obs {
                        let diff = x_data[i * dim + di] - muk_d;
                        sum_sq += gamma[i * k + ki] * (diff * diff);
                    }
                    vars[ki * dim + di] = (sum_sq / n_k) + RIDGE_EPS;
                }
            }
        }

        // Compute cluster assignments and max posteriors
        let mut cluster_assignments = Vec::with_capacity(n_obs);
        let mut max_posterior_probs = Vec::with_capacity(n_obs);
        for i in 0..n_obs {
            let mut best_k = 0;
            let mut best_p = f64::NEG_INFINITY;
            for ki in 0..k {
                let p = gamma[i * k + ki];
                if p > best_p {
                    best_p = p;
                    best_k = ki;
                }
            }
            cluster_assignments.push(best_k as i64);
            max_posterior_probs.push(best_p);
        }

        // Information criteria
        // Parameters: (K - 1) weights + K*D means + K*D variances
        let p_params = (k - 1) + k * dim + k * dim;
        let aic = 2.0 * (p_params as f64) - 2.0 * total_ll;
        let bic = (p_params as f64) * (n_obs as f64).ln() - 2.0 * total_ll;

        Ok(Self {
            k,
            dim,
            n_obs,
            column_names: col_names,
            weights,
            means,
            variances: vars,
            log_likelihood: total_ll,
            aic,
            bic,
            iterations: iters_done,
            converged,
            cluster_assignments,
            max_posterior_probs,
            responsibilities: gamma,
        })
    }

    /// `summary(model)` cockpit report for terminal.
    pub fn render_cockpit(&self, caps: &RenderCaps) -> String {
        let mut panel = CockpitPanel::new("NEKO GMM: Gaussian Mixture Model");
        let badge = if self.converged {
            if caps.unicode_enabled {
                format!(
                    "/ᐠ˵- ⩊ -˵マ ✧ EM CONVERGED in {} iterations",
                    self.iterations
                )
            } else {
                format!("[CONVERGED in {} iterations]", self.iterations)
            }
        } else {
            format!("[MAX ITER REACHED: {} iterations]", self.iterations)
        };
        panel.with_badge(&badge);

        panel.add_kv("Components (K)", format!("{}", self.k));
        panel.add_kv("Dimensions (D)", format!("{}", self.dim));
        panel.add_kv("Observations", format!("{} observations", self.n_obs));
        panel.add_kv(
            "Fit Metrics",
            format!(
                "Log-Likelihood = {:.4} | AIC = {:.2} | BIC = {:.2}",
                self.log_likelihood, self.aic, self.bic
            ),
        );

        panel.add_divider();

        // Components table
        panel.add_line(format!(
            "{:<8} {:>10} {:>10}  {:<35}",
            "Cluster", "Weight", "Est.Size", "Means (first dimensions)"
        ));
        panel.add_line(format!(
            "{:<8} {:>10} {:>10}  {:<35}",
            "-------", "------", "--------", "-------------------------"
        ));

        for ki in 0..self.k {
            let w = self.weights[ki];
            let est_size = (w * (self.n_obs as f64)).round() as usize;

            let num_to_show = self.dim.min(3);
            let mut mean_strs = Vec::new();
            for di in 0..num_to_show {
                mean_strs.push(format!("{:.3}", self.means[ki * self.dim + di]));
            }
            if self.dim > 3 {
                mean_strs.push("...".to_string());
            }
            let mean_repr = format!("[{}]", mean_strs.join(", "));

            panel.add_line(format!(
                "#{:<7} {:>10.4} {:>10}  {:<35}",
                ki, w, est_size, mean_repr
            ));
        }

        panel.add_divider();
        if caps.unicode_enabled {
            panel.add_line(caps.haru("ฅ(•⩊ •マ Haru clustered all observations successfully!"));
        } else {
            panel.add_line("[GMM clustering completed]");
        }

        panel.render(caps)
    }

    /// `tidy(m)` — DataFrame of component statistics.
    pub fn tidy(&self) -> Value {
        let mut components = Vec::with_capacity(self.k);
        let mut weights_val = Vec::with_capacity(self.k);
        let mut est_sizes = Vec::with_capacity(self.k);

        for ki in 0..self.k {
            components.push(Value::I64(ki as i64));
            weights_val.push(Value::F64(self.weights[ki]));
            est_sizes.push(Value::I64(
                (self.weights[ki] * (self.n_obs as f64)).round() as i64
            ));
        }

        let mut cols = vec![
            ("component".to_string(), components),
            ("weight".to_string(), weights_val),
            ("est_size".to_string(), est_sizes),
        ];

        // Add columns for means of each dimension
        for di in 0..self.dim {
            let col_name = if di < self.column_names.len() {
                format!("mean_{}", self.column_names[di])
            } else {
                format!("mean_d{di}")
            };
            let mut d_means = Vec::with_capacity(self.k);
            for ki in 0..self.k {
                d_means.push(Value::F64(self.means[ki * self.dim + di]));
            }
            cols.push((col_name, d_means));
        }

        let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)
            .expect("tidy() builds a DataFrame from well-formed vectors");
        Value::DataFrame { frame, na_reasons }
    }

    /// `glance(m)` — 1-row DataFrame of global fit statistics.
    pub fn glance(&self) -> Value {
        let cols = vec![
            (
                "log_likelihood".to_string(),
                vec![Value::F64(self.log_likelihood)],
            ),
            ("log_lik".to_string(), vec![Value::F64(self.log_likelihood)]),
            ("aic".to_string(), vec![Value::F64(self.aic)]),
            ("bic".to_string(), vec![Value::F64(self.bic)]),
            ("k".to_string(), vec![Value::I64(self.k as i64)]),
            ("dim".to_string(), vec![Value::I64(self.dim as i64)]),
            ("nobs".to_string(), vec![Value::I64(self.n_obs as i64)]),
            ("n_obs".to_string(), vec![Value::I64(self.n_obs as i64)]),
            (
                "iterations".to_string(),
                vec![Value::I64(self.iterations as i64)],
            ),
            ("converged".to_string(), vec![Value::Bool(self.converged)]),
        ];

        let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)
            .expect("glance() builds a DataFrame from well-formed vectors");
        Value::DataFrame { frame, na_reasons }
    }

    /// `predict(m, newdata)` — predicts hard cluster assignment (0..K-1) for observations.
    pub fn predict(&self, newdata: &Value) -> Result<Value, Diagnostic> {
        let (x_data, n, d) = match newdata {
            Value::Matrix { rows, cols, data } => {
                if *cols != self.dim {
                    return Err(Diagnostic::statistical_error(
                        "S0412",
                        format!(
                            "`predict()`: dimension mismatch, model expects {} dims, found {}",
                            self.dim, cols
                        ),
                    ));
                }
                (data.clone(), *rows, *cols)
            }
            Value::DataFrame { frame, .. } => {
                let n = frame.height();
                let mut data = Vec::with_capacity(n * self.dim);
                for i in 0..n {
                    for name in &self.column_names {
                        let c = frame.column(name).map_err(|_| {
                            Diagnostic::statistical_error(
                                "S0200",
                                format!("Column `{name}` not found in predict dataframe"),
                            )
                        })?;
                        let v = c
                            .get(i)
                            .map_err(|e| Diagnostic::compute_error("C0210", format!("{e}")))?;
                        let val: f64 = match v {
                            polars_core::datatypes::AnyValue::Float64(f) => f,
                            polars_core::datatypes::AnyValue::Int64(n) => n as f64,
                            _ => 0.0,
                        };
                        data.push(val);
                    }
                }
                (std::sync::Arc::new(data), n, self.dim)
            }
            other => {
                return Err(Diagnostic::compute_error(
                    "C0201",
                    format!(
                        "`predict()` requires a DataFrame or Matrix, found `{}`",
                        other.type_name()
                    ),
                ));
            }
        };

        let two_pi = std::f64::consts::PI * 2.0;
        let log_two_pi_d = (d as f64) * two_pi.ln();
        let mut predictions = Vec::with_capacity(n);

        for i in 0..n {
            let xi = &x_data[i * d..(i + 1) * d];
            let mut best_k = 0;
            let mut best_lp = f64::NEG_INFINITY;

            for ki in 0..self.k {
                let muk = &self.means[ki * d..(ki + 1) * d];
                let vark = &self.variances[ki * d..(ki + 1) * d];
                let mut mahal = 0.0;
                let mut log_det = 0.0;

                for di in 0..d {
                    let diff = xi[di] - muk[di];
                    let v = vark[di].max(RIDGE_EPS);
                    mahal += (diff * diff) / v;
                    log_det += v.ln();
                }

                let log_gauss = -0.5 * (log_two_pi_d + log_det + mahal);
                let log_p = self.weights[ki].ln() + log_gauss;
                if log_p > best_lp {
                    best_lp = log_p;
                    best_k = ki;
                }
            }
            predictions.push(Value::I64(best_k as i64));
        }

        Ok(Value::Vector(VectorData::from_values(predictions)))
    }

    /// `augment(m, data)` — enriches original DataFrame with `.cluster` and `.probability`.
    pub fn augment(&self, orig_df: &Value) -> Result<Value, Diagnostic> {
        match orig_df {
            Value::DataFrame { frame, na_reasons } => {
                if frame.height() != self.n_obs {
                    return Err(Diagnostic::statistical_error(
                        "S0200",
                        format!(
                            "`augment()`: DataFrame height ({}) does not match model fit observations ({})",
                            frame.height(),
                            self.n_obs
                        ),
                    ));
                }

                let mut new_frame = frame.clone();
                let cluster_col: Vec<i64> = self.cluster_assignments.clone();
                let prob_col: Vec<Option<f64>> =
                    self.max_posterior_probs.iter().map(|&p| Some(p)).collect();

                new_frame
                    .with_column(crate::polars_bridge::i64_column(".cluster", cluster_col))
                    .map_err(|e| Diagnostic::compute_error("C0210", format!("{e}")))?;

                new_frame
                    .with_column(crate::polars_bridge::f64_opt_column(
                        ".probability",
                        prob_col,
                    ))
                    .map_err(|e| Diagnostic::compute_error("C0210", format!("{e}")))?;

                Ok(Value::DataFrame {
                    frame: new_frame,
                    na_reasons: Arc::clone(na_reasons),
                })
            }
            other => Err(Diagnostic::compute_error(
                "C0201",
                format!(
                    "`augment()` requires a DataFrame as second argument, found `{}`",
                    other.type_name()
                ),
            )),
        }
    }
}

impl fmt::Display for FittedGmm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let caps = RenderCaps::detect();
        write!(f, "{}", self.render_cockpit(&caps))
    }
}
