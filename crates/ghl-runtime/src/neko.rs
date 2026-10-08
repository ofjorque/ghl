use crate::matrix::MatrixOps;
use crate::na_reasons::NaReasonTable;
use crate::value::Value;
use crate::vector_data::{VectorData, column_as_f64_view};
use ghl_diagnostics::{CockpitPanel, Diagnostic, RenderCaps, Sparkline};
use ghl_types::ContrastScheme;
use polars_core::prelude::*;
use rayon::prelude::*;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::sync::Arc;

/// Positional row disposition tracking complete cases analysis.
/// Distinguishes between included rows and the exact cause of omission.
#[derive(Debug, Clone, PartialEq)]
pub enum RowDisposition {
    Included,
    DroppedNA { col: String, reason: Option<String> },
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

/// Generates a k x (k - 1) contrast matrix for a given ContrastScheme.
pub fn contrast_matrix(scheme: ContrastScheme, k: usize) -> Vec<Vec<f64>> {
    if k < 2 {
        return Vec::new();
    }
    let cols = k - 1;
    let mut mat = vec![vec![0.0; cols]; k];

    match scheme {
        ContrastScheme::Treatment => {
            // Level 0 is baseline (all zeros).
            // Level i (1 <= i < k) has 1.0 at column i - 1.
            for i in 1..k {
                mat[i][i - 1] = 1.0;
            }
        }
        ContrastScheme::Sum => {
            // Levels 0..k-2 have 1.0 at diagonal.
            // Level k-1 has -1.0 across all columns.
            for i in 0..cols {
                mat[i][i] = 1.0;
            }
            for j in 0..cols {
                mat[k - 1][j] = -1.0;
            }
        }
        ContrastScheme::Helmert => {
            // Column j (0 <= j < cols):
            // rows 0..=j have -1.0
            // row j + 1 has (j + 1)
            // rows > j + 1 have 0.0
            for j in 0..cols {
                for i in 0..=j {
                    mat[i][j] = -1.0;
                }
                mat[j + 1][j] = (j + 1) as f64;
            }
        }
        ContrastScheme::Polynomial => {
            // Orthogonal polynomial contrast over k equally spaced points (centered).
            let x: Vec<f64> = (0..k).map(|i| i as f64 - (k - 1) as f64 / 2.0).collect();
            let mut basis: Vec<Vec<f64>> = Vec::with_capacity(cols + 1);
            basis.push(vec![1.0; k]); // degree 0 constant

            for d in 1..=cols {
                let mut v: Vec<f64> = x.iter().map(|&xi| xi.powi(d as i32)).collect();
                for b in &basis {
                    let dot_vb: f64 = v.iter().zip(b).map(|(vi, bi)| vi * bi).sum();
                    let dot_bb: f64 = b.iter().map(|bi| bi * bi).sum();
                    if dot_bb > 1e-12 {
                        let factor = dot_vb / dot_bb;
                        for i in 0..k {
                            v[i] -= factor * b[i];
                        }
                    }
                }
                let norm: f64 = v.iter().map(|vi| vi * vi).sum::<f64>().sqrt();
                if norm > 1e-12 {
                    for val in &mut v {
                        *val /= norm;
                    }
                }
                basis.push(v);
            }

            for j in 0..cols {
                let poly_col = &basis[j + 1];
                for i in 0..k {
                    mat[i][j] = poly_col[i];
                }
            }
        }
    }

    mat
}

/// Frozen recipe of predictors and response learned from data.
#[derive(Debug, Clone, PartialEq)]
pub struct Blueprint {
    pub response: String,
    pub terms: Vec<String>,
    pub term_names: Vec<String>,
    pub contrast: ContrastScheme,
    pub term_levels: HashMap<String, Vec<String>>,
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
            contrast: ContrastScheme::Treatment,
            term_levels: HashMap::new(),
        }
    }

    pub fn with_contrast(mut self, contrast: ContrastScheme) -> Self {
        self.contrast = contrast;
        self
    }

    /// Bakes a DataFrame into design matrix X and response y.
    /// Preserves row disposition for all rows, and expands categorical factor terms
    /// into contrast columns based on the configured ContrastScheme.
    pub fn bake(
        &self,
        frame: &DataFrame,
        na_reasons: &NaReasonTable,
    ) -> Result<
        (
            Vec<f64>,
            Vec<f64>,
            Vec<RowDisposition>,
            usize,
            usize,
            Vec<String>,
            HashMap<String, Vec<String>>,
        ),
        Diagnostic,
    > {
        let resp_col = frame.column(&self.response).map_err(|_| {
            Diagnostic::statistical_error(
                "S0202",
                format!(
                    "Response variable `{}` not found in DataFrame",
                    self.response
                ),
            )
        })?;
        let term_cols: Vec<&Column> = self
            .terms
            .iter()
            .map(|term| {
                frame.column(term).map_err(|_| {
                    Diagnostic::statistical_error(
                        "S0203",
                        format!("Predictor variable `{}` not found in DataFrame", term),
                    )
                })
            })
            .collect::<Result<_, _>>()?;

        let total_rows = frame.height();

        struct TermInfo {
            term: String,
            is_categorical: bool,
            levels: Vec<String>,
            contrast_mat: Vec<Vec<f64>>,
        }

        let mut term_infos = Vec::with_capacity(term_cols.len());
        let mut baked_term_names = Vec::new();
        let mut baked_term_levels = self.term_levels.clone();
        baked_term_names.push("(Intercept)".to_string());

        for (term, col) in self.terms.iter().zip(&term_cols) {
            let is_categorical = col.dtype() == &DataType::String || col.dtype().is_categorical();
            if is_categorical {
                let levels: Vec<String> = if let Some(existing) = self.term_levels.get(term) {
                    existing.clone()
                } else {
                    let mut levels_set = BTreeSet::new();
                    for r in 0..total_rows {
                        if let Ok(av) = col.get(r) {
                            match av {
                                AnyValue::String(s) => {
                                    levels_set.insert(s.to_string());
                                }
                                AnyValue::StringOwned(s) => {
                                    levels_set.insert(s.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                    levels_set.into_iter().collect()
                };

                if levels.len() < 2 {
                    return Err(Diagnostic::statistical_error(
                        "S0206",
                        format!(
                            "Categorical predictor `{}` must have at least 2 distinct levels, found {}",
                            term,
                            levels.len()
                        ),
                    ));
                }

                let k = levels.len();
                let contrast_mat = contrast_matrix(self.contrast, k);
                for j in 0..k - 1 {
                    let col_name = match self.contrast {
                        ContrastScheme::Treatment => format!("{}{}", term, levels[j + 1]),
                        ContrastScheme::Sum | ContrastScheme::Helmert => {
                            format!("{}{}", term, j + 1)
                        }
                        ContrastScheme::Polynomial => {
                            let suffix = match j + 1 {
                                1 => ".L",
                                2 => ".Q",
                                3 => ".C",
                                d => Box::leak(format!("^{}", d).into_boxed_str()),
                            };
                            format!("{}{}", term, suffix)
                        }
                    };
                    baked_term_names.push(col_name);
                }

                baked_term_levels.insert(term.clone(), levels.clone());
                term_infos.push(TermInfo {
                    term: term.clone(),
                    is_categorical: true,
                    levels,
                    contrast_mat,
                });
            } else {
                baked_term_names.push(term.clone());
                term_infos.push(TermInfo {
                    term: term.clone(),
                    is_categorical: false,
                    levels: Vec::new(),
                    contrast_mat: Vec::new(),
                });
            }
        }

        let p = baked_term_names.len();
        let any_nulls = resp_col.null_count() > 0 || term_cols.iter().any(|c| c.null_count() > 0);

        if !any_nulls && term_infos.iter().all(|info| !info.is_categorical) {
            let y_view = column_as_f64_view(resp_col)?;
            let term_views = term_cols
                .iter()
                .map(|c| column_as_f64_view(c))
                .collect::<Result<Vec<_>, _>>()?;
            let y_data = y_view.as_slice().to_vec();
            let mut x_rows = Vec::with_capacity(total_rows * p);
            for i in 0..total_rows {
                x_rows.push(1.0);
                for view in &term_views {
                    x_rows.push(view.as_slice()[i]);
                }
            }
            return Ok((
                x_rows,
                y_data,
                vec![RowDisposition::Included; total_rows],
                total_rows,
                p,
                baked_term_names,
                baked_term_levels,
            ));
        }

        let mut dispositions = Vec::with_capacity(total_rows);
        let mut x_rows = Vec::new();
        let mut y_vals = Vec::new();

        for i in 0..total_rows {
            let mut row_disp = RowDisposition::Included;

            let resp_av = resp_col.get(i).expect("row index is always in bounds");
            if resp_av.is_null() {
                row_disp = RowDisposition::DroppedNA {
                    col: self.response.clone(),
                    reason: na_reasons.get(&self.response, i).map(|s| s.to_string()),
                };
            }

            for (term_info, col) in term_infos.iter().zip(&term_cols) {
                let av = col.get(i).expect("row index is always in bounds");
                if av.is_null() {
                    let term_reason = na_reasons.get(&term_info.term, i).map(|s| s.to_string());
                    if row_disp == RowDisposition::Included {
                        row_disp = RowDisposition::DroppedNA {
                            col: term_info.term.clone(),
                            reason: term_reason,
                        };
                    } else if let RowDisposition::DroppedNA {
                        reason: ref curr_reason,
                        ..
                    } = row_disp
                    {
                        if curr_reason.is_none() && term_reason.is_some() {
                            row_disp = RowDisposition::DroppedNA {
                                col: term_info.term.clone(),
                                reason: term_reason,
                            };
                        }
                    }
                } else if term_info.is_categorical && row_disp == RowDisposition::Included {
                    // A value outside the known levels is only possible when baking
                    // against frozen, prediction-time levels (`self.term_levels`):
                    // during fitting, levels are discovered from this exact data, so
                    // every value matches by construction. Treat it like a missing
                    // value rather than silently mapping it to level 0 (the baseline
                    // category) - a real out-of-vocabulary category is not the same
                    // observation as the reference level, and predicting as if it
                    // were is a wrong answer with no indication anything was off.
                    let s = match &av {
                        AnyValue::String(s) => Some(*s),
                        AnyValue::StringOwned(s) => Some(s.as_str()),
                        _ => None,
                    };
                    if let Some(s) = s {
                        if !term_info.levels.iter().any(|l| l == s) {
                            row_disp = RowDisposition::DroppedNA {
                                col: term_info.term.clone(),
                                reason: Some(format!(
                                    "Unseen category `{}` for predictor `{}`",
                                    s, term_info.term
                                )),
                            };
                        }
                    }
                }
            }

            if row_disp == RowDisposition::Included {
                y_vals.push(any_value_as_f64(&resp_av));

                // Row of X: [1.0, term_1_cols..., term_2_cols..., ...]
                x_rows.push(1.0);
                for (term_info, col) in term_infos.iter().zip(&term_cols) {
                    let av = col.get(i).expect("row index is always in bounds");
                    if !term_info.is_categorical {
                        x_rows.push(any_value_as_f64(&av));
                    } else {
                        let s = match av {
                            AnyValue::String(s) => s,
                            AnyValue::StringOwned(ref s) => s.as_str(),
                            _ => "",
                        };
                        let level_idx = term_info.levels.iter().position(|l| l == s)
                            .expect("unseen categories are routed to RowDisposition::DroppedNA above and excluded from this branch");
                        for &val in &term_info.contrast_mat[level_idx] {
                            x_rows.push(val);
                        }
                    }
                }
            }

            dispositions.push(row_disp);
        }

        let n = y_vals.len();
        Ok((
            x_rows,
            y_vals,
            dispositions,
            n,
            p,
            baked_term_names,
            baked_term_levels,
        ))
    }
}

/// Coerces a single DataFrame cell to `f64`, mirroring exactly the coercion
/// `Value::as_f64()` already applies once a cell has been boxed (`AnyValue` -> numeric
/// `Value` variant -> `as_f64()`, `polars_bridge::any_value_to_plain_value`): any integer
/// width or `Float32`/`Float64` converts, anything else (bool, string, ...) falls back to
/// `0.0`. Used only by `Blueprint::bake`'s fallback path, to read a cell's numeric value
/// without ever boxing it into a `Value` first.
fn any_value_as_f64(av: &AnyValue) -> f64 {
    match av {
        AnyValue::Int8(n) => *n as f64,
        AnyValue::Int16(n) => *n as f64,
        AnyValue::Int32(n) => *n as f64,
        AnyValue::Int64(n) => *n as f64,
        AnyValue::UInt8(n) => *n as f64,
        AnyValue::UInt16(n) => *n as f64,
        AnyValue::UInt32(n) => *n as f64,
        AnyValue::UInt64(n) => *n as f64,
        AnyValue::Float32(x) => *x as f64,
        AnyValue::Float64(x) => *x,
        _ => 0.0,
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
    /// Row-major design matrix (n×p), kept so `compute_vcov` can build the real HC0-HC3
    /// sandwich estimator later -- it was previously discarded after the fit, which is
    /// exactly why HC0/HC2/HC3 couldn't be computed correctly at all (see `compute_vcov`).
    pub x_data: Vec<f64>,
}

impl FittedModel {
    /// Fits an OLS model from a formula blueprint and data.
    pub fn fit_ols(
        blueprint: Blueprint,
        frame: &DataFrame,
        na_reasons: &NaReasonTable,
    ) -> Result<Self, Diagnostic> {
        let (x_data, y_data, dispositions, n, p, baked_term_names, baked_term_levels) =
            blueprint.bake(frame, na_reasons)?;
        let mut blueprint = blueprint;
        blueprint.term_names = baked_term_names;
        blueprint.term_levels = baked_term_levels;

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

        // 1-2. Compute X^T X (p x p) and X^T y (p x 1) -- parallelized above
        // PARALLEL_THRESHOLD (shared with `glm::FittedGlm::fit_logistic`'s identical
        // per-IRLS-iteration assembly, weight=1.0/target=y here instead of weight=W/
        // target=z). Never parallelized before this point -- never needed to be, at
        // the scale OLS was used at so far, but IRLS repeats this same O(n*p^2) cost
        // MAX_ITER times, which is what actually justified writing this once and
        // sharing it instead of leaving OLS's copy sequential.
        let (xtx, xty) = assemble_weighted_normal_equations(n, p, &x_data, None, &y_data);

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
            let term_name = &blueprint.term_names[j];
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
            x_data,
        })
    }

    /// Computes the covariance matrix for a given VcovKind. `HC0`-`HC3` use the real
    /// sandwich estimator (`sandwich_vcov`, shared with `glm::FittedGlm`) -- they used
    /// to just rescale `s2 * inv_xtx` differently (HC0/HC2/HC3 were numerically
    /// identical to Classical, HC1 only applied a scalar df correction), which isn't
    /// the RFC's own formula (`docs/design/11-neko-statistical-modeling-framework.md`
    /// §5) and isn't real heteroskedasticity-robust inference -- `FittedModel` didn't
    /// even retain the design matrix needed to compute it correctly until now.
    pub fn compute_vcov(&self, kind: VcovKind) -> Result<Vec<f64>, Diagnostic> {
        let p = self.blueprint.term_names.len();

        match kind {
            VcovKind::Classical => {
                let s2 = self.residual_se.powi(2);
                Ok(self.inv_xtx.iter().map(|&v| v * s2).collect())
            }
            VcovKind::HC0 | VcovKind::HC1 | VcovKind::HC2 | VcovKind::HC3 => {
                let sq_resid: Vec<f64> = self.residuals.iter().map(|e| e * e).collect();
                sandwich_vcov(
                    p,
                    self.n_obs,
                    self.df_resid,
                    &self.x_data,
                    &sq_resid,
                    &self.inv_xtx,
                    kind,
                )
            }
        }
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

    /// Returns a 1-row DataFrame summarizing global goodness-of-fit metrics.
    pub fn glance(&self) -> Value {
        let cols = vec![
            ("r_squared".to_string(), vec![Value::F64(self.r_squared)]),
            (
                "adj_r_squared".to_string(),
                vec![Value::F64(self.adj_r_squared)],
            ),
            (
                "residual_se".to_string(),
                vec![Value::F64(self.residual_se)],
            ),
            ("f_statistic".to_string(), vec![Value::F64(self.f_stat)]),
            ("aic".to_string(), vec![Value::F64(self.aic)]),
            ("bic".to_string(), vec![Value::F64(self.bic)]),
            ("n_obs".to_string(), vec![Value::I64(self.n_obs as i64)]),
            (
                "dropped_n".to_string(),
                vec![Value::I64(self.dropped_n as i64)],
            ),
        ];

        let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)
            .expect("glance() builds a DataFrame from well-formed numeric vectors");
        Value::DataFrame { frame, na_reasons }
    }

    /// Returns the original DataFrame augmented with .fitted, .residual, .used_in_fit, .na_reason.
    ///
    /// Only the 4 new columns (O(n)) are built -- the original P columns are never reboxed
    /// through `Vec<Value>` (that used to cost O(n*p) for data the fit never even touches
    /// here, since it's already fitted). Same `frame.clone()` (cheap, Arc-shared column
    /// buffers) + `DataFrame::with_column` idiom `mutate()` already uses (`io.rs`).
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
                            reason_col
                                .push(reason.clone().unwrap_or_else(|| "unspecified".to_string()));
                        }
                    }
                }

                let mut new_frame = frame.clone();
                new_frame
                    .with_column(crate::polars_bridge::f64_opt_column(".fitted", fitted_col))
                    .map_err(|e| {
                        Diagnostic::compute_error("C0210", format!("`augment()` failed: {e}"))
                    })?;
                new_frame
                    .with_column(crate::polars_bridge::f64_opt_column(".residual", res_col))
                    .map_err(|e| {
                        Diagnostic::compute_error("C0210", format!("`augment()` failed: {e}"))
                    })?;
                new_frame
                    .with_column(crate::polars_bridge::bool_column(".used_in_fit", used_col))
                    .map_err(|e| {
                        Diagnostic::compute_error("C0210", format!("`augment()` failed: {e}"))
                    })?;
                new_frame
                    .with_column(crate::polars_bridge::string_column(
                        ".na_reason",
                        reason_col,
                    ))
                    .map_err(|e| {
                        Diagnostic::compute_error("C0210", format!("`augment()` failed: {e}"))
                    })?;

                // The 4 new columns never carry a real NA reason (`.fitted`/`.residual`
                // are `NA(None)` when dropped, `.used_in_fit`/`.na_reason` are never NA
                // themselves) -- the original table's entries are still valid as-is.
                Ok(Value::DataFrame {
                    frame: new_frame,
                    na_reasons: Arc::clone(na_reasons),
                })
            }
            _ => Err(Diagnostic::compute_error(
                "C0201",
                "`augment()` requires a DataFrame as second argument",
            )),
        }
    }

    /// Predicts values for a new DataFrame using the frozen Blueprint.
    ///
    /// The returned vector always has one entry per row of `newdata`, in the
    /// same order: a row `bake()` couldn't use (missing data, or a
    /// categorical value never seen while fitting) becomes `NA` with the same
    /// reason `bake()` recorded for it, rather than being silently dropped
    /// from the output - which used to desynchronize the result from the
    /// input whenever any row was excluded.
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
                let (x_data, _, dispositions, _, p, _, _) =
                    self.blueprint.bake(frame_ref, na_reasons)?;
                let mut predictions = Vec::with_capacity(dispositions.len());
                let mut included_idx = 0;

                for disp in &dispositions {
                    match disp {
                        RowDisposition::Included => {
                            let mut y_hat = 0.0;
                            for j in 0..p {
                                y_hat += x_data[included_idx * p + j] * self.coefficients[j];
                            }
                            predictions.push(Value::F64(y_hat));
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

    /// Renders the model fit as a structured Cockpit Deck terminal card.
    pub fn render_cockpit(&self, caps: &RenderCaps) -> String {
        let mut panel = CockpitPanel::new("NEKO Model Fit");
        let badge = if caps.unicode_enabled {
            "/ᐠ˵- ⩊ -˵マ ✧ CONVERGED"
        } else {
            "[CONVERGED]"
        };
        panel.with_badge(badge);

        panel.add_kv(
            "Formula",
            format!(
                "{} ~ {}",
                self.blueprint.response,
                self.blueprint.terms.join(" + ")
            ),
        );

        let obs_text = if self.dropped_n > 0 {
            format!(
                "{} valid ({} dropped due to NA)",
                self.n_obs, self.dropped_n
            )
        } else {
            format!("{} valid", self.n_obs)
        };
        panel.add_kv("Observations", obs_text);

        let spark = Sparkline::render(&self.residuals, Some(16), caps);
        let min_res = self.residuals.iter().copied().fold(f64::INFINITY, f64::min);
        let max_res = self
            .residuals
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        panel.add_kv(
            "Residuals",
            format!("{spark}  (min: {:.3}, max: +{:.3})", min_res, max_res),
        );

        panel.add_kv(
            "Goodness of Fit",
            format!(
                "R² = {:.4} | Adj R² = {:.4} | F = {:.2}",
                self.r_squared, self.adj_r_squared, self.f_stat
            ),
        );
        panel.add_kv(
            "Criteria",
            format!(
                "AIC = {:.2} | BIC = {:.2} | Res SE = {:.4} on {} DF",
                self.aic, self.bic, self.residual_se, self.df_resid
            ),
        );

        panel.add_divider();

        panel.add_line(format!(
            "{:<16} {:>10} {:>10} {:>9} {:>9}  {:^6}",
            "Term", "Estimate", "Std.Err", "t-stat", "p-val", "Signif"
        ));
        panel.add_line(format!(
            "{:<16} {:>10} {:>10} {:>9} {:>9}  {:^6}",
            "----------------", "----------", "----------", "---------", "---------", "------"
        ));

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
            let glyph = if caps.unicode_enabled {
                "/ᐠ˵- ⩊ -˵マ ✧"
            } else {
                "✔"
            };
            panel.add_line(format!(
                "{} All assumptions verified. No severe multicollinearity.",
                caps.green(glyph)
            ));
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

/// `pub(crate)`: reused directly by `glm::FittedGlm` for Wald z-statistic p-values
/// (asymptotically normal, unlike OLS's t-statistics -- no df correction needed).
pub(crate) fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

pub(crate) fn erf(x: f64) -> f64 {
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

/// Assembles the (possibly weighted) normal equations `X^T W X` (p×p) and `X^T W t`
/// (p×1) for a row-major design matrix -- shared between `FittedModel::fit_ols`
/// (`weights = None`, i.e. all 1.0, `t = y`) and `glm::FittedGlm::fit_logistic` (each
/// IRLS iteration: `weights = Some(&W)`, `t = z`, the working response). Parallelized
/// with rayon above `crate::eval::PARALLEL_THRESHOLD` (same threshold measured in Fase
/// 4) via `fold`+`reduce` (each parallel task accumulates into its own local p×p/p×1
/// buffer across many rows, not once per row, then the per-task buffers are summed --
/// avoids allocating a fresh buffer per row the way a naive `map` + `reduce` would).
/// Below the threshold, a single sequential pass. This is what actually answers Caso
/// 3.2's "eficiencia del solver matricial": IRLS repeats this exact O(n*p^2) assembly
/// once per iteration, so at N=1,000,000/P=40 this dominates the whole fit's cost far
/// more than the O(p^3) `MatrixOps::solve` step that follows it.
pub(crate) fn assemble_weighted_normal_equations(
    n: usize,
    p: usize,
    x_data: &[f64],
    weights: Option<&[f64]>,
    target: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    let accumulate = |acc: &mut (Vec<f64>, Vec<f64>), i: usize| {
        let row = &x_data[i * p..(i + 1) * p];
        let w = weights.map(|w| w[i]).unwrap_or(1.0);
        let t = target[i];
        for a in 0..p {
            acc.1[a] += w * row[a] * t;
            for b in 0..p {
                acc.0[a * p + b] += w * row[a] * row[b];
            }
        }
    };

    if n >= crate::eval::PARALLEL_THRESHOLD {
        (0..n)
            .into_par_iter()
            .fold(
                || (vec![0.0; p * p], vec![0.0; p]),
                |mut acc, i| {
                    accumulate(&mut acc, i);
                    acc
                },
            )
            .reduce(
                || (vec![0.0; p * p], vec![0.0; p]),
                |mut a, b| {
                    for i in 0..p * p {
                        a.0[i] += b.0[i];
                    }
                    for i in 0..p {
                        a.1[i] += b.1[i];
                    }
                    a
                },
            )
    } else {
        let mut acc = (vec![0.0; p * p], vec![0.0; p]);
        for i in 0..n {
            accumulate(&mut acc, i);
        }
        acc
    }
}

/// Shared heteroskedasticity-consistent "sandwich" covariance estimator
/// (`V = inv_bread · meat · inv_bread`, RFC §5), used by both `FittedModel::compute_vcov`
/// (OLS: `inv_bread = (X^TX)^{-1}`, weight = squared residual) and
/// `glm::FittedGlm::compute_vcov` (IRLS: `inv_bread = (X^TWX)^{-1}`, weight = squared
/// score `(y-mu)^2`) -- same math either way, only the "bread" and per-observation
/// weight differ, so it isn't duplicated between the two model types. `kind` must be
/// one of `HC0`/`HC1`/`HC2`/`HC3` (`Classical` doesn't go through this -- each caller
/// computes it directly, since it's just `s2 * inv_bread` with no sandwich at all).
pub(crate) fn sandwich_vcov(
    p: usize,
    n: usize,
    df_resid: usize,
    x_data: &[f64],
    sq_score: &[f64],
    inv_bread: &[f64],
    kind: VcovKind,
) -> Result<Vec<f64>, Diagnostic> {
    let mut meat = vec![0.0; p * p];
    for i in 0..n {
        let row = &x_data[i * p..(i + 1) * p];
        let w_i = match kind {
            VcovKind::HC0 | VcovKind::HC1 => sq_score[i],
            VcovKind::HC2 | VcovKind::HC3 => {
                // Leverage h_ii = row_i^T * inv_bread * row_i, using the already-
                // computed inverse "bread" matrix -- no separate hat-matrix pass needed.
                let mut h_ii = 0.0;
                for a in 0..p {
                    let mut tmp = 0.0;
                    for b in 0..p {
                        tmp += inv_bread[a * p + b] * row[b];
                    }
                    h_ii += row[a] * tmp;
                }
                match kind {
                    VcovKind::HC2 => sq_score[i] / (1.0 - h_ii),
                    VcovKind::HC3 => sq_score[i] / (1.0 - h_ii).powi(2),
                    _ => unreachable!(),
                }
            }
            VcovKind::Classical => unreachable!(
                "Classical is handled by the caller directly, never reaches sandwich_vcov"
            ),
        };
        for a in 0..p {
            for b in 0..p {
                meat[a * p + b] += w_i * row[a] * row[b];
            }
        }
    }

    if kind == VcovKind::HC1 {
        let corr = n as f64 / df_resid as f64;
        for v in meat.iter_mut() {
            *v *= corr;
        }
    }

    let (_, _, bread_meat) = MatrixOps::mul(p, p, inv_bread, p, p, &meat)?;
    let (_, _, sandwich) = MatrixOps::mul(p, p, &bread_meat, p, p, inv_bread)?;
    Ok(sandwich)
}
