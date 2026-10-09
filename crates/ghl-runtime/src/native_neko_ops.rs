//! NEKO statistical model native invocations (fit/ols/predict/tidy/...).
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use crate::value::Value;
use crate::vector_data::VectorData;
use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::FormulaOp;
use polars_core::prelude::NamedFrom;

// NEKO Native Invocations

pub(crate) fn native_fit_ols(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`fit()` requires Formula and DataFrame arguments: `fit(model, df)`",
        ));
    }

    let (response, terms) = match &args[0] {
        Value::Formula {
            op: FormulaOp::Regression,
            response,
            terms,
            ..
        } => (response.clone(), terms.clone()),
        Value::Formula { op, .. } => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "First argument of `fit()` must be a regression formula (`~`), found `{op}`"
                ),
            ));
        }
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "First argument of `fit()` must be a Formula, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "Second argument of `fit()` must be a DataFrame, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };
    let blueprint = crate::neko::Blueprint::new(response, terms);
    let model = crate::neko::FittedModel::fit_ols(blueprint, frame, na_reasons)?;
    Ok(Value::ModelFit(Box::new(model)))
}

/// `fit_logistic(y ~ x1 + ... + xP, df)` -- IRLS-fit binary logistic regression
/// (TODO.md Fase 6, Caso 3.2). Same argument shape/validation as `native_fit_ols`
/// above, deliberately not unified into one function with a family argument: the two
/// share `Blueprint`/`Blueprint::bake()` already, but a `Value::GlmFit` result needs
/// its own diagnostics (`glm::FittedGlm`), not OLS's.
pub(crate) fn native_fit_logistic(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`fit_logistic()` requires Formula and DataFrame arguments: `fit_logistic(model, df)`",
        ));
    }

    let (response, terms) = match &args[0] {
        Value::Formula {
            op: FormulaOp::Regression,
            response,
            terms,
            ..
        } => (response.clone(), terms.clone()),
        Value::Formula { op, .. } => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "First argument of `fit_logistic()` must be a regression formula (`~`), found `{op}`"
                ),
            ));
        }
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "First argument of `fit_logistic()` must be a Formula, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "Second argument of `fit_logistic()` must be a DataFrame, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };
    let blueprint = crate::neko::Blueprint::new(response, terms);
    let model = crate::glm::FittedGlm::fit_logistic(blueprint, frame, na_reasons)?;
    Ok(Value::GlmFit(Box::new(model)))
}

/// `poisson(y ~ x1 + ... + xP, df)` -- IRLS-fit Poisson regression (log link).
pub(crate) fn native_fit_poisson(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`poisson()` requires Formula and DataFrame arguments: `poisson(model, df)`",
        ));
    }

    let (response, terms) = match &args[0] {
        Value::Formula {
            op: FormulaOp::Regression,
            response,
            terms,
            ..
        } => (response.clone(), terms.clone()),
        Value::Formula { op, .. } => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "First argument of `poisson()` must be a regression formula (`~`), found `{op}`"
                ),
            ));
        }
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "First argument of `poisson()` must be a Formula, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "Second argument of `poisson()` must be a DataFrame, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };
    let blueprint = crate::neko::Blueprint::new(response, terms);
    let model = crate::glm::FittedGlm::fit_poisson(blueprint, frame, na_reasons)?;
    Ok(Value::GlmFit(Box::new(model)))
}

pub(crate) fn native_fit_gmm(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`fit_gmm(data, k, [max_iter], [tol])` requires at least 2 arguments: data (DataFrame or Matrix) and k (number of clusters)",
        ));
    }
    let k = match args.get(1) {
        Some(Value::I64(n)) if *n > 0 => *n as usize,
        Some(Value::F64(f)) if *f > 0.0 => *f as usize,
        _ => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                "Second argument of `fit_gmm` must be a positive integer k (number of clusters)",
            ));
        }
    };
    let max_iter = match args.get(2) {
        Some(Value::I64(n)) if *n > 0 => *n as usize,
        Some(Value::F64(f)) if *f > 0.0 => *f as usize,
        _ => 100,
    };
    let tol = match args.get(3) {
        Some(Value::F64(f)) if *f > 0.0 => *f,
        _ => 1e-4,
    };

    let model = crate::gmm::FittedGmm::fit(&args[0], k, Some(max_iter), Some(tol))?;
    Ok(Value::GmmFit(Box::new(model)))
}

pub(crate) fn native_summary(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`summary()` requires a ModelFit or printable object",
        )
    })?;

    match model_val {
        Value::ModelFit(m) => {
            println!("{}", m);
            Ok(Value::Unit)
        }
        Value::GlmFit(m) => {
            println!("{}", m);
            Ok(Value::Unit)
        }
        Value::GmmFit(m) => {
            println!("{}", m);
            Ok(Value::Unit)
        }
        Value::Struct { name, fields } => {
            if name == "SemResult" {
                println!("/ᐠ˵- ⩊ -˵マ ✧ CONVERGED (Wishart Maximum Likelihood)");
                println!("===============================================================");
                println!("Structural Equation Model (SEM / CFA) - Global Fit Indices");
                println!("---------------------------------------------------------------");
                if let (Some(n), Some(f_min), Some(chisq), Some(df), Some(p_val)) = (
                    fields.get("n_obs"),
                    fields.get("f_min"),
                    fields.get("chisq"),
                    fields.get("df"),
                    fields.get("p_value"),
                ) {
                    println!("Number of observations : {}", n);
                    println!("Minimum function value : {}", f_min);
                    println!(
                        "Model Chi-Square       : {} (df = {}, p-value = {})",
                        chisq, df, p_val
                    );
                }
                if let (Some(b_chisq), Some(b_df)) =
                    (fields.get("baseline_chisq"), fields.get("baseline_df"))
                {
                    println!("Baseline Chi-Square    : {} (df = {})", b_chisq, b_df);
                }
                if let Some(cfi) = fields.get("cfi") {
                    println!("CFI (Comparative Fit)  : {}", cfi);
                }
                if let Some(tli) = fields.get("tli") {
                    println!("TLI (Tucker-Lewis)     : {}", tli);
                }
                if let Some(rmsea) = fields.get("rmsea") {
                    println!("RMSEA                  : {}", rmsea);
                }
                if let Some(srmr) = fields.get("srmr") {
                    println!("SRMR                   : {}", srmr);
                }
                println!("---------------------------------------------------------------");
                println!("Parameter Estimates:");
                if let Some(params) = fields.get("parameters") {
                    println!("{}", params);
                }
                println!("===============================================================");
                return Ok(Value::Unit);
            }
            if name == "FeolsResult" {
                println!("/ᐠ˵- ⩊ -˵マ ✧ CONVERGED (High-Dimensional Fixed Effects OLS - feols)");
                println!("===============================================================");
                println!("Model: High-Dimensional Fixed Effects OLS (feols)");
                println!("---------------------------------------------------------------");
                if let Some(resp) = fields.get("response") {
                    println!("Dependent Variable : {}", resp);
                }
                if let Some(n) = fields.get("n_obs") {
                    println!("Observations       : {}", n);
                }
                if let (Some(abs), Some(df_fe)) = (fields.get("absorbed"), fields.get("df_fe")) {
                    println!("Absorbed FEs       : {} (df = {})", abs, df_fe);
                }
                if let Some(df_resid) = fields.get("df_resid") {
                    println!("Residual DF        : {}", df_resid);
                }
                if let Some(vcov_t) = fields.get("vcov_type") {
                    println!("VCOV Estimation    : {}", vcov_t);
                }
                if let (Some(r2), Some(r2_w)) = (fields.get("r2"), fields.get("r2_within")) {
                    println!("R-Squared          : {} (Within: {})", r2, r2_w);
                }
                println!("---------------------------------------------------------------");
                println!("Parameter Estimates:");
                if let Some(params) = fields.get("parameters") {
                    println!("{}", params);
                }
                println!("===============================================================");
                return Ok(Value::Unit);
            }
            if name == "IvResult" {
                println!("/ᐠ˵- ⩊ -˵マ ✧ CONVERGED (Two-Stage Least Squares - 2SLS / iv_regress)");
                println!("===============================================================");
                println!("Model: Two-Stage Least Squares (IV / 2SLS)");
                println!("---------------------------------------------------------------");
                if let Some(resp) = fields.get("response") {
                    println!("Dependent Variable : {}", resp);
                }
                if let Some(n) = fields.get("n_obs") {
                    println!("Observations       : {}", n);
                }
                if let Some(df_resid) = fields.get("df_resid") {
                    println!("Residual DF        : {}", df_resid);
                }
                if let Some(endog) = fields.get("endogenous") {
                    println!("Endogenous Vars    : {}", endog);
                }
                if let Some(instr) = fields.get("instruments") {
                    println!("Instruments        : {}", instr);
                }
                if let Some(vcov_t) = fields.get("vcov_type") {
                    println!("VCOV Estimation    : {}", vcov_t);
                }
                if let Some(r2) = fields.get("r2") {
                    println!("R-Squared          : {}", r2);
                }
                println!("---------------------------------------------------------------");
                println!("Diagnostic Tests:");
                if let Some(f_stat) = fields.get("first_stage_f") {
                    let weak_flag = if let Some(Value::Bool(true)) = fields.get("weak_instruments")
                    {
                        "[WARNING: F < 10, weak instruments]"
                    } else {
                        "[PASS: F >= 10]"
                    };
                    println!(
                        "  Weak Instruments (1st stage F) : {} {}",
                        f_stat, weak_flag
                    );
                }
                if let (Some(w_stat), Some(w_p)) =
                    (fields.get("wu_hausman_stat"), fields.get("wu_hausman_p"))
                {
                    println!(
                        "  Wu-Hausman Endogeneity Test    : F = {}, p-value = {}",
                        w_stat, w_p
                    );
                }
                if let (Some(s_stat), Some(s_df), Some(s_p)) = (
                    fields.get("sargan_stat"),
                    fields.get("sargan_df"),
                    fields.get("sargan_p"),
                ) {
                    if !s_stat.is_na() {
                        println!(
                            "  Sargan Overidentification Test : stat = {} (df = {}), p-value = {}",
                            s_stat, s_df, s_p
                        );
                    }
                }
                println!("---------------------------------------------------------------");
                println!("Parameter Estimates:");
                if let Some(params) = fields.get("parameters") {
                    println!("{}", params);
                }
                println!("===============================================================");
                return Ok(Value::Unit);
            }
            if name == "RegularizedResult" {
                let m_type = fields
                    .get("model_type")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "Regularized".to_string());
                println!(
                    "/ᐠ˵- ⩊ -˵マ ✧ CONVERGED (Penalized Regularized Regression - {})",
                    m_type
                );
                println!("===============================================================");
                println!("Model: Penalized Regularized Regression ({})", m_type);
                println!("---------------------------------------------------------------");
                if let Some(resp) = fields.get("response") {
                    println!("Dependent Variable : {}", resp);
                }
                if let Some(n) = fields.get("n_obs") {
                    println!("Observations       : {}", n);
                }
                if let (Some(n_feat), Some(n_sel)) =
                    (fields.get("n_features"), fields.get("n_selected"))
                {
                    println!(
                        "Features Selected  : {} / {} active (non-zero)",
                        n_sel, n_feat
                    );
                }
                if let Some(alpha) = fields.get("alpha") {
                    println!("Mixing Parameter α : {}", alpha);
                }
                if let Some(lam) = fields.get("lambda") {
                    println!("Penalty Weight λ   : {}", lam);
                }
                if let (Some(l_min), Some(l_1se)) =
                    (fields.get("lambda_min"), fields.get("lambda_1se"))
                {
                    if !l_min.is_na() {
                        println!(
                            "Cross-Validation   : Optimal λ_min = {}, λ_1se = {}",
                            l_min, l_1se
                        );
                    }
                }
                if let Some(r2) = fields.get("r2") {
                    println!("R-Squared          : {}", r2);
                }
                if let Some(mse) = fields.get("mse") {
                    println!("Mean Squared Error : {}", mse);
                }
                if let Some(iters) = fields.get("iterations") {
                    println!("Iterations         : {}", iters);
                }
                println!("---------------------------------------------------------------");
                println!("Parameter Estimates:");
                if let Some(params) = fields.get("parameters") {
                    println!("{}", params);
                }
                println!("===============================================================");
                return Ok(Value::Unit);
            }
            let method_key = format!("{}::summary", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                interp.call_value(fn_val, vec![model_val.clone()])
            } else {
                println!("{}", model_val);
                Ok(Value::Unit)
            }
        }
        other => {
            println!("{}", other);
            Ok(Value::Unit)
        }
    }
}

fn is_scalar_value(v: &Value) -> bool {
    matches!(
        v,
        Value::F64(_) | Value::I64(_) | Value::Bool(_) | Value::String(_) | Value::NA(_)
    )
}

fn extract_scalar_metric(
    interp: &mut crate::eval::Interpreter,
    model_val: &Value,
    fn_name: &str,
    capability_name: &str,
    field_aliases: &[&str],
) -> Result<Value, Diagnostic> {
    match model_val {
        Value::ModelFit(m) => match capability_name {
            "aic" => Ok(Value::F64(m.aic)),
            "bic" => Ok(Value::F64(m.bic)),
            "r_squared" => Ok(Value::F64(m.r_squared)),
            "adj_r_squared" => Ok(Value::F64(m.adj_r_squared)),
            "log_lik" => Ok(Value::F64(m.log_likelihood)),
            "nobs" => Ok(Value::I64(m.n_obs as i64)),
            _ => Err(Diagnostic::statistical_error(
                "E0502",
                format!("ModelFit does not have capability `{capability_name}`"),
            )),
        },
        Value::GlmFit(m) => match capability_name {
            "aic" => Ok(Value::F64(m.aic)),
            "bic" => Ok(Value::F64(m.bic)),
            "r_squared" => Ok(Value::F64(m.pseudo_r_squared)),
            "log_lik" => Ok(Value::F64(-0.5 * m.deviance)),
            "nobs" => Ok(Value::I64(m.n_obs as i64)),
            "deviance" => Ok(Value::F64(m.deviance)),
            _ => Err(Diagnostic::statistical_error(
                "E0502",
                format!("GlmFit does not have capability `{capability_name}`"),
            )),
        },
        Value::GmmFit(m) => match capability_name {
            "aic" => Ok(Value::F64(m.aic)),
            "bic" => Ok(Value::F64(m.bic)),
            "log_lik" => Ok(Value::F64(m.log_likelihood)),
            "nobs" => Ok(Value::I64(m.n_obs as i64)),
            _ => Err(Diagnostic::statistical_error(
                "E0502",
                format!("GmmFit does not have capability `{capability_name}`"),
            )),
        },
        Value::Struct { name, fields } => {
            for alias in field_aliases {
                if let Some(val) = fields.get(*alias) {
                    return Ok(val.clone());
                }
            }
            let method_key = format!("{}::{}", name, capability_name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }
            for alias in field_aliases {
                let alias_key = format!("{}::{}", name, alias);
                if let Some(fn_val) = interp.env.get(&alias_key) {
                    return interp.call_value(fn_val, vec![model_val.clone()]);
                }
            }
            Err(Diagnostic::statistical_error(
                "E0502",
                format!("Model or struct `{name}` does not have capability `{capability_name}`"),
            ))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "`{}()` requires a ModelFit or Struct, found `{}`",
                fn_name,
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn native_aic(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`aic()` requires a model or struct: `aic(m)`")
    })?;
    extract_scalar_metric(interp, model_val, "aic", "aic", &["aic", "AIC"])
}

pub(crate) fn native_bic(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`bic()` requires a model or struct: `bic(m)`")
    })?;
    extract_scalar_metric(interp, model_val, "bic", "bic", &["bic", "BIC"])
}

pub(crate) fn native_r_squared(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`r_squared()` requires a model or struct: `r_squared(m)`",
        )
    })?;
    extract_scalar_metric(
        interp,
        model_val,
        "r_squared",
        "r_squared",
        &[
            "r_squared",
            "r2",
            "pseudo_r_squared",
            "pseudo_r2",
            "r_sq",
            "r2_within",
        ],
    )
}

pub(crate) fn native_adj_r_squared(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`adj_r_squared()` requires a model or struct: `adj_r_squared(m)`",
        )
    })?;
    extract_scalar_metric(
        interp,
        model_val,
        "adj_r_squared",
        "adj_r_squared",
        &["adj_r_squared", "adj_r2", "r2_adj", "adjusted_r_squared"],
    )
}

pub(crate) fn native_log_lik(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`log_lik()` requires a model or struct: `log_lik(m)`",
        )
    })?;
    extract_scalar_metric(
        interp,
        model_val,
        "log_lik",
        "log_lik",
        &["log_lik", "log_likelihood", "log_partial_lik", "loglik"],
    )
}

pub(crate) fn native_nobs(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`nobs()` requires a model or struct: `nobs(m)`")
    })?;
    extract_scalar_metric(
        interp,
        model_val,
        "nobs",
        "nobs",
        &["nobs", "n_obs", "n", "sample_size", "n_observations"],
    )
}

pub(crate) fn native_fitted(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`fitted()` requires a model or struct: `fitted(m)`",
        )
    })?;
    match model_val {
        Value::ModelFit(m) => Ok(Value::Vector(VectorData::from_f64(m.fitted_values.clone()))),
        Value::GlmFit(m) => Ok(Value::Vector(VectorData::from_f64(m.fitted_values.clone()))),
        Value::Struct { name, fields } => {
            for alias in &["fitted", "fitted_values", "y_hat", "mu"] {
                if let Some(val) = fields.get(*alias) {
                    return Ok(val.clone());
                }
            }
            let method_key = format!("{}::fitted", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }
            let method_key2 = format!("{}::fitted_values", name);
            if let Some(fn_val) = interp.env.get(&method_key2) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }
            Err(Diagnostic::statistical_error(
                "E0502",
                format!("Model or struct `{name}` does not have capability `fitted`"),
            ))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "`fitted()` requires a ModelFit or Struct, found `{}`",
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn native_tidy(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`tidy()` requires a ModelFit or Struct")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(m.tidy()),
        Value::GlmFit(m) => Ok(m.tidy()),
        Value::GmmFit(m) => Ok(m.tidy()),
        Value::Struct { name, fields } => {
            // 1. Explicit method {name}::tidy
            let method_key = format!("{}::tidy", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }

            // 2. Existing built-in results with pre-baked parameter tables
            if name == "SemResult"
                || name == "FeolsResult"
                || name == "IvResult"
                || name == "RegularizedResult"
            {
                if let Some(params) = fields.get("parameters") {
                    return Ok(params.clone());
                }
            }
            if let Some(tidy_df) = fields.get("tidy").or_else(|| fields.get("parameters")) {
                if matches!(tidy_df, Value::DataFrame { .. }) {
                    return Ok(tidy_df.clone());
                }
            }

            // 3. Dynamic harvest of vector parameter estimates
            let coefs_opt = fields
                .get("coefficients")
                .or_else(|| fields.get("estimates"))
                .or_else(|| fields.get("betas"))
                .or_else(|| fields.get("ar_coeffs"));
            if let Some(Value::Vector(coef_vec)) = coefs_opt {
                let n_terms = coef_vec.len();
                let terms_vec: Vec<Value> = if let Some(Value::Vector(terms)) =
                    fields.get("covariates").or_else(|| fields.get("terms"))
                {
                    terms.iter().cloned().collect()
                } else {
                    (0..n_terms)
                        .map(|i| Value::String(format!("x{i}")))
                        .collect()
                };

                let estimates_vec: Vec<Value> = coef_vec.iter().cloned().collect();
                let se_vec: Vec<Value> = if let Some(Value::Vector(ses)) =
                    fields.get("std_errors").or_else(|| fields.get("std_err"))
                {
                    ses.iter().cloned().collect()
                } else {
                    vec![Value::NA(None); n_terms]
                };
                let stat_vec: Vec<Value> = if let Some(Value::Vector(stats)) = fields
                    .get("z_stats")
                    .or_else(|| fields.get("t_stats"))
                    .or_else(|| fields.get("statistics"))
                {
                    stats.iter().cloned().collect()
                } else {
                    vec![Value::NA(None); n_terms]
                };
                let p_vec: Vec<Value> = if let Some(Value::Vector(ps)) =
                    fields.get("p_values").or_else(|| fields.get("p_value"))
                {
                    ps.iter().cloned().collect()
                } else {
                    vec![Value::NA(None); n_terms]
                };

                let mut cols = vec![
                    ("term".to_string(), terms_vec),
                    ("estimate".to_string(), estimates_vec),
                    ("std_error".to_string(), se_vec),
                    ("statistic".to_string(), stat_vec),
                    ("p_value".to_string(), p_vec),
                ];
                if let Some(Value::Vector(lows)) =
                    fields.get("conf_low").or_else(|| fields.get("hr_ci_lower"))
                {
                    cols.push(("conf_low".to_string(), lows.iter().cloned().collect()));
                }
                if let Some(Value::Vector(highs)) = fields
                    .get("conf_high")
                    .or_else(|| fields.get("hr_ci_upper"))
                {
                    cols.push(("conf_high".to_string(), highs.iter().cloned().collect()));
                }

                let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)?;
                return Ok(Value::DataFrame { frame, na_reasons });
            }

            // 4. Dynamic harvest of single scalar focal estimate (e.g. DiD, Univariate Cox)
            let single_est_opt = fields
                .get("att")
                .or_else(|| fields.get("beta"))
                .or_else(|| fields.get("estimate"));
            if let Some(est_val) = single_est_opt {
                if est_val.as_f64().is_some() {
                    let term_name = fields
                        .get("term")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| name.clone());
                    let se_val = fields
                        .get("std_error")
                        .or_else(|| fields.get("std_err"))
                        .cloned()
                        .unwrap_or(Value::NA(None));
                    let stat_val = fields
                        .get("t_stat")
                        .or_else(|| fields.get("z_stat"))
                        .or_else(|| fields.get("statistic"))
                        .cloned()
                        .unwrap_or(Value::NA(None));
                    let p_val = fields.get("p_value").cloned().unwrap_or(Value::NA(None));

                    let mut cols = vec![
                        ("term".to_string(), vec![Value::String(term_name)]),
                        ("estimate".to_string(), vec![est_val.clone()]),
                        ("std_error".to_string(), vec![se_val]),
                        ("statistic".to_string(), vec![stat_val]),
                        ("p_value".to_string(), vec![p_val]),
                    ];
                    if let Some(low) = fields.get("conf_low").or_else(|| fields.get("hr_ci_lower"))
                    {
                        cols.push(("conf_low".to_string(), vec![low.clone()]));
                    }
                    if let Some(high) = fields
                        .get("conf_high")
                        .or_else(|| fields.get("hr_ci_upper"))
                    {
                        cols.push(("conf_high".to_string(), vec![high.clone()]));
                    }

                    let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)?;
                    return Ok(Value::DataFrame { frame, na_reasons });
                }
            }

            Err(Diagnostic::statistical_error(
                "E0501",
                format!(
                    "Model or struct `{name}` does not implement or provide parameter estimates for `tidy()`"
                ),
            ))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "`tidy()` requires a ModelFit or Struct, found `{}`",
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn native_glance(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`glance()` requires a ModelFit or Struct")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(m.glance()),
        Value::GlmFit(m) => Ok(m.glance()),
        Value::GmmFit(m) => Ok(m.glance()),
        Value::Struct { name, fields } => {
            // 1. Explicit method on struct: {name}::glance
            let method_key = format!("{}::glance", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }

            // 2. Builtin special cases for known internal structs
            if name == "SemResult" {
                let columns: Vec<(String, Vec<Value>)> = vec![
                    (
                        "chisq".to_string(),
                        vec![fields.get("chisq").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "df".to_string(),
                        vec![fields.get("df").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "p_value".to_string(),
                        vec![fields.get("p_value").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "cfi".to_string(),
                        vec![fields.get("cfi").cloned().unwrap_or(Value::F64(1.0))],
                    ),
                    (
                        "tli".to_string(),
                        vec![fields.get("tli").cloned().unwrap_or(Value::F64(1.0))],
                    ),
                    (
                        "rmsea".to_string(),
                        vec![fields.get("rmsea").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "srmr".to_string(),
                        vec![fields.get("srmr").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "nobs".to_string(),
                        vec![fields.get("n_obs").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "n_obs".to_string(),
                        vec![fields.get("n_obs").cloned().unwrap_or(Value::I64(0))],
                    ),
                ];
                let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&columns)?;
                return Ok(Value::DataFrame { frame, na_reasons });
            }
            if name == "FeolsResult" {
                let columns: Vec<(String, Vec<Value>)> = vec![
                    (
                        "r_squared".to_string(),
                        vec![fields.get("r2").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "r2".to_string(),
                        vec![fields.get("r2").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "r2_within".to_string(),
                        vec![fields.get("r2_within").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "nobs".to_string(),
                        vec![fields.get("n_obs").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "n_obs".to_string(),
                        vec![fields.get("n_obs").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "df_fe".to_string(),
                        vec![fields.get("df_fe").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "df_resid".to_string(),
                        vec![fields.get("df_resid").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "vcov_type".to_string(),
                        vec![
                            fields
                                .get("vcov_type")
                                .cloned()
                                .unwrap_or(Value::String("cluster".into())),
                        ],
                    ),
                ];
                let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&columns)?;
                return Ok(Value::DataFrame { frame, na_reasons });
            }
            if name == "IvResult" {
                let columns: Vec<(String, Vec<Value>)> = vec![
                    (
                        "r_squared".to_string(),
                        vec![fields.get("r2").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "r2".to_string(),
                        vec![fields.get("r2").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "nobs".to_string(),
                        vec![fields.get("n_obs").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "n_obs".to_string(),
                        vec![fields.get("n_obs").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "df_resid".to_string(),
                        vec![fields.get("df_resid").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "first_stage_f".to_string(),
                        vec![
                            fields
                                .get("first_stage_f")
                                .cloned()
                                .unwrap_or(Value::NA(None)),
                        ],
                    ),
                    (
                        "wu_hausman_p".to_string(),
                        vec![
                            fields
                                .get("wu_hausman_p")
                                .cloned()
                                .unwrap_or(Value::F64(1.0)),
                        ],
                    ),
                    (
                        "sargan_p".to_string(),
                        vec![fields.get("sargan_p").cloned().unwrap_or(Value::NA(None))],
                    ),
                    (
                        "vcov_type".to_string(),
                        vec![
                            fields
                                .get("vcov_type")
                                .cloned()
                                .unwrap_or(Value::String("classical".into())),
                        ],
                    ),
                ];
                let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&columns)?;
                return Ok(Value::DataFrame { frame, na_reasons });
            }
            if name == "RegularizedResult" {
                let columns: Vec<(String, Vec<Value>)> = vec![
                    (
                        "model_type".to_string(),
                        vec![
                            fields
                                .get("model_type")
                                .cloned()
                                .unwrap_or(Value::String("Lasso".into())),
                        ],
                    ),
                    (
                        "alpha".to_string(),
                        vec![fields.get("alpha").cloned().unwrap_or(Value::F64(1.0))],
                    ),
                    (
                        "lambda".to_string(),
                        vec![fields.get("lambda").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "nobs".to_string(),
                        vec![fields.get("n_obs").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "n_obs".to_string(),
                        vec![fields.get("n_obs").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "n_features".to_string(),
                        vec![fields.get("n_features").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "n_selected".to_string(),
                        vec![fields.get("n_selected").cloned().unwrap_or(Value::I64(0))],
                    ),
                    (
                        "r_squared".to_string(),
                        vec![fields.get("r2").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "r2".to_string(),
                        vec![fields.get("r2").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                    (
                        "mse".to_string(),
                        vec![fields.get("mse").cloned().unwrap_or(Value::F64(0.0))],
                    ),
                ];
                let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&columns)?;
                return Ok(Value::DataFrame { frame, na_reasons });
            }

            // 3. Dynamic harvest of scalar metrics across any model/struct
            let mut cols: Vec<(String, Vec<Value>)> = Vec::new();
            let mut extracted = std::collections::BTreeSet::new();

            let priority_candidates = [
                ("nobs", &["nobs", "n_obs", "n", "sample_size"][..]),
                (
                    "r_squared",
                    &["r_squared", "r2", "pseudo_r_squared", "pseudo_r2", "r_sq"][..],
                ),
                ("adj_r_squared", &["adj_r_squared", "adj_r2", "r2_adj"][..]),
                (
                    "log_lik",
                    &["log_lik", "log_likelihood", "log_partial_lik", "loglik"][..],
                ),
                ("aic", &["aic", "AIC"][..]),
                ("bic", &["bic", "BIC"][..]),
                ("sigma", &["sigma", "sigma2", "residual_se"][..]),
                ("deviance", &["deviance"][..]),
                ("null_deviance", &["null_deviance"][..]),
                ("chisq", &["chisq"][..]),
                ("df", &["df"][..]),
                ("p_value", &["p_value"][..]),
                ("statistic", &["statistic", "f_stat", "f_statistic"][..]),
                ("converged", &["converged"][..]),
                ("iterations", &["iterations"][..]),
            ];

            for (canonical_name, aliases) in &priority_candidates {
                for alias in *aliases {
                    if let Some(val) = fields.get(*alias) {
                        if is_scalar_value(val) && !extracted.contains(*canonical_name) {
                            cols.push((canonical_name.to_string(), vec![val.clone()]));
                            extracted.insert(canonical_name.to_string());
                            extracted.insert(alias.to_string());
                            break;
                        }
                    }
                }
            }

            // Also harvest remaining scalar fields in struct
            for (field_name, field_val) in fields.iter() {
                if !extracted.contains(field_name) && is_scalar_value(field_val) {
                    cols.push((field_name.clone(), vec![field_val.clone()]));
                    extracted.insert(field_name.clone());
                }
            }

            if cols.is_empty() {
                return Err(Diagnostic::statistical_error(
                    "E0501",
                    format!(
                        "Model or struct `{name}` does not implement or provide metrics for `glance()`"
                    ),
                ));
            }

            let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&cols)?;
            Ok(Value::DataFrame { frame, na_reasons })
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "`glance()` requires a ModelFit or Struct, found `{}`",
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn native_augment(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`augment()` requires a ModelFit or Struct: `augment(model, df)`",
        )
    })?;

    if args.len() == 1 {
        if let Value::Struct { name, fields } = model_val {
            if let Some(aug) = fields.get("augmented") {
                return Ok(aug.clone());
            }
            let method_key = format!("{}::augment", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, args);
            }
        }
        return Err(Diagnostic::compute_error(
            "C0201",
            "`augment()` requires a DataFrame argument: `augment(model, df)`",
        ));
    }

    match &args[0] {
        Value::ModelFit(m) => m.augment(&args[1]),
        Value::GlmFit(m) => m.augment(&args[1]),
        Value::GmmFit(m) => m.augment(&args[1]),
        Value::Struct { name, fields } => {
            if let Some(aug) = fields.get("augmented") {
                return Ok(aug.clone());
            }
            let method_key = format!("{}::augment", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, args);
            }
            if let Value::DataFrame { frame, na_reasons } = &args[1] {
                let mut new_frame = frame.clone();
                let mut augmented_any = false;
                if let Some(Value::Vector(vec_data)) =
                    fields.get("fitted").or_else(|| fields.get("fitted_values"))
                {
                    let f64s: Vec<Option<f64>> = vec_data.iter().map(|v| v.as_f64()).collect();
                    if f64s.len() == frame.height() {
                        new_frame
                            .with_column(crate::polars_bridge::f64_opt_column(".fitted", f64s))
                            .map_err(|e| Diagnostic::compute_error("C0210", format!("{e}")))?;
                        augmented_any = true;
                    }
                }
                if let Some(Value::Vector(vec_data)) =
                    fields.get("residuals").or_else(|| fields.get("resid"))
                {
                    let f64s: Vec<Option<f64>> = vec_data.iter().map(|v| v.as_f64()).collect();
                    if f64s.len() == frame.height() {
                        new_frame
                            .with_column(crate::polars_bridge::f64_opt_column(".resid", f64s))
                            .map_err(|e| Diagnostic::compute_error("C0210", format!("{e}")))?;
                        augmented_any = true;
                    }
                }
                if augmented_any {
                    return Ok(Value::DataFrame {
                        frame: new_frame,
                        na_reasons: std::sync::Arc::clone(na_reasons),
                    });
                }
            }
            Err(Diagnostic::statistical_error(
                "E0501",
                format!(
                    "Model or struct `{name}` does not implement or provide data for `augment()`"
                ),
            ))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "First argument of `augment()` must be a ModelFit or Struct, found `{}`",
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn native_predict(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`predict()` requires ModelFit and DataFrame arguments: `predict(model, df)`",
        ));
    }

    match &args[0] {
        Value::ModelFit(m) => m.predict(&args[1]),
        Value::GlmFit(m) => m.predict(&args[1]),
        Value::GmmFit(m) => m.predict(&args[1]),
        Value::Struct { name, fields } => {
            if name == "RegularizedResult" {
                match &args[1] {
                    Value::DataFrame { frame, .. } => {
                        return crate::regularized::predict_regularized(fields, frame);
                    }
                    other => {
                        return Err(Diagnostic::statistical_error(
                            "S0200",
                            format!(
                                "Second argument of `predict()` must be a DataFrame, found `{}`",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            let method_key = format!("{}::predict", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, args);
            }
            Err(Diagnostic::statistical_error(
                "S0200",
                format!("`predict()` is not implemented for struct `{}`", name),
            ))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "First argument of `predict()` must be a ModelFit or Model struct, found `{}`",
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn native_residuals(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`residuals()` requires a ModelFit or Struct")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(Value::Vector(VectorData::from_f64(m.residuals.clone()))),
        Value::GlmFit(m) => Ok(Value::Vector(VectorData::from_f64(m.residuals.clone()))),
        Value::Struct { name, fields } => {
            for alias in &["residuals", "resid", "resids", "errors"] {
                if let Some(val) = fields.get(*alias) {
                    match val {
                        Value::Vector(_) => return Ok(val.clone()),
                        Value::F64(f) => return Ok(Value::Vector(VectorData::from_f64(vec![*f]))),
                        _ => return Ok(val.clone()),
                    }
                }
            }
            let method_key = format!("{}::residuals", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }
            let method_key2 = format!("{}::resid", name);
            if let Some(fn_val) = interp.env.get(&method_key2) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }
            Err(Diagnostic::statistical_error(
                "E0502",
                format!("Model or struct `{name}` does not have capability `residuals`"),
            ))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "`residuals()` requires a ModelFit or Struct, found `{}`",
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn native_coef(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`coef()` requires a ModelFit or Struct")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(Value::Vector(VectorData::from_f64(m.coefficients.clone()))),
        Value::GlmFit(m) => Ok(Value::Vector(VectorData::from_f64(m.coefficients.clone()))),
        Value::GmmFit(m) => Ok(Value::Matrix {
            rows: m.k,
            cols: m.dim,
            data: std::sync::Arc::new(m.means.clone()),
        }),
        Value::Struct { name, fields } => {
            for alias in &["coefficients", "betas", "ar_coeffs", "coefs", "coef"] {
                if let Some(val) = fields.get(*alias) {
                    match val {
                        Value::Vector(_) | Value::Matrix { .. } => return Ok(val.clone()),
                        Value::F64(f) => return Ok(Value::Vector(VectorData::from_f64(vec![*f]))),
                        _ => return Ok(val.clone()),
                    }
                }
            }
            for alias in &["beta", "att", "estimate"] {
                if let Some(val) = fields.get(*alias) {
                    match val {
                        Value::Vector(_) | Value::Matrix { .. } => return Ok(val.clone()),
                        Value::F64(f) => return Ok(Value::Vector(VectorData::from_f64(vec![*f]))),
                        _ => return Ok(val.clone()),
                    }
                }
            }
            let method_key = format!("{}::coef", name);
            if let Some(fn_val) = interp.env.get(&method_key) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }
            let method_key2 = format!("{}::coefficients", name);
            if let Some(fn_val) = interp.env.get(&method_key2) {
                return interp.call_value(fn_val, vec![model_val.clone()]);
            }
            Err(Diagnostic::statistical_error(
                "E0502",
                format!("Model or struct `{name}` does not have capability `coef`"),
            ))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "`coef()` requires a ModelFit or Struct, found `{}`",
                other.type_name()
            ),
        )),
    }
}

pub(crate) fn vcov_kind_from_arg(args: &[Value]) -> crate::neko::VcovKind {
    if let Some(k_str) = args.get(1).and_then(|v| v.as_str()) {
        match k_str.to_uppercase().as_str() {
            "HC0" => crate::neko::VcovKind::HC0,
            "HC1" => crate::neko::VcovKind::HC1,
            "HC2" => crate::neko::VcovKind::HC2,
            "HC3" => crate::neko::VcovKind::HC3,
            _ => crate::neko::VcovKind::Classical,
        }
    } else {
        crate::neko::VcovKind::Classical
    }
}

pub(crate) fn native_vcov(
    interp: &mut crate::eval::Interpreter,
    args: Vec<Value>,
) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`vcov()` requires a ModelFit or Struct")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            let kind = vcov_kind_from_arg(&args);
            let p = m.blueprint.term_names.len();
            let vcov_data = m.compute_vcov(kind)?;
            Ok(Value::Matrix {
                rows: p,
                cols: p,
                data: std::sync::Arc::new(vcov_data),
            })
        }
        Value::GlmFit(m) => {
            let kind = vcov_kind_from_arg(&args);
            let p = m.blueprint.term_names.len();
            let vcov_data = m.compute_vcov(kind)?;
            Ok(Value::Matrix {
                rows: p,
                cols: p,
                data: std::sync::Arc::new(vcov_data),
            })
        }
        Value::Struct { name, fields } => {
            if name == "SemResult" {
                if let Some(implied_cov) = fields.get("implied_cov") {
                    return Ok(implied_cov.clone());
                }
            }
            if name == "FeolsResult" || name == "IvResult" {
                if let Some(vc) = fields.get("vcov") {
                    return Ok(vc.clone());
                }
            }
            let method_key = format!("{}::vcov", name);
            let fn_val = interp.env.get(&method_key).ok_or_else(|| {
                Diagnostic::statistical_error(
                    "S0200",
                    format!("Method `vcov()` is not implemented for struct `{}`", name),
                )
            })?;
            interp.call_value(fn_val, vec![model_val.clone()])
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "`vcov()` requires a ModelFit or Struct, found `{}`",
                other.type_name()
            ),
        )),
    }
}

/// `formula_parts(f)` -- extracts decomposed parts from a Formula into a `FormulaParts` struct.
pub(crate) fn native_formula_parts(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`formula_parts()` requires a Formula argument: `formula_parts(f)`",
        ));
    }

    match &args[0] {
        Value::Formula {
            response,
            terms,
            parts,
            ..
        } => {
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("response".to_string(), Value::String(response.clone()));
            fields.insert(
                "terms".to_string(),
                Value::Vector(VectorData::from_values(
                    terms.iter().map(|s| Value::String(s.clone())).collect(),
                )),
            );
            fields.insert(
                "parts".to_string(),
                Value::Vector(VectorData::from_values(
                    parts
                        .iter()
                        .map(|p| {
                            Value::Vector(VectorData::from_values(
                                p.iter().map(|s| Value::String(s.clone())).collect(),
                            ))
                        })
                        .collect(),
                )),
            );
            fields.insert(
                "absorbed".to_string(),
                Value::Vector(VectorData::from_values(
                    parts
                        .get(1)
                        .map(|p| p.iter().map(|s| Value::String(s.clone())).collect())
                        .unwrap_or_default(),
                )),
            );
            fields.insert(
                "instruments".to_string(),
                Value::Vector(VectorData::from_values(
                    parts
                        .get(2)
                        .map(|p| p.iter().map(|s| Value::String(s.clone())).collect())
                        .unwrap_or_default(),
                )),
            );
            fields.insert("parts_count".to_string(), Value::I64(parts.len() as i64));
            fields.insert(
                "has_fixed_effects".to_string(),
                Value::Bool(parts.len() >= 2),
            );
            fields.insert("has_instruments".to_string(), Value::Bool(parts.len() >= 3));

            Ok(Value::Struct {
                name: "FormulaParts".to_string(),
                fields: std::sync::Arc::new(fields),
            })
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!(
                "`formula_parts()` expects a Formula, found `{}`",
                other.type_name()
            ),
        )),
    }
}

/// `decompose_spec(spec)` -- extracts equations from a `sem_spec` or `irt_spec` into a DataFrame.
pub(crate) fn native_decompose_spec(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`decompose_spec()` requires a specification argument: `decompose_spec(spec)`",
        ));
    }

    let equations = match &args[0] {
        Value::SemSpec(eqs) => eqs.clone(),
        Value::Formula { .. } => vec![args[0].clone()],
        other => {
            return Err(Diagnostic::compute_error(
                "C0202",
                format!(
                    "`decompose_spec(spec)` expects `sem_spec`, `irt_spec`, or `Formula`, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };

    let mut lhs_vec: Vec<String> = Vec::with_capacity(equations.len());
    let mut op_vec: Vec<String> = Vec::with_capacity(equations.len());
    let mut rhs_vec: Vec<String> = Vec::with_capacity(equations.len());

    for eq in &equations {
        if let Value::Formula {
            op,
            response,
            terms,
            ..
        } = eq
        {
            lhs_vec.push(response.clone());
            op_vec.push(op.to_string());
            rhs_vec.push(terms.join(" + "));
        }
    }

    let frame = polars_core::frame::DataFrame::new(
        equations.len(),
        vec![
            polars_core::series::Series::new("lhs".into(), lhs_vec).into(),
            polars_core::series::Series::new("op".into(), op_vec).into(),
            polars_core::series::Series::new("rhs".into(), rhs_vec).into(),
        ],
    )
    .map_err(|e| Diagnostic::compute_error("C0105", format!("Failed to create DataFrame: {e}")))?;

    Ok(Value::DataFrame {
        frame,
        na_reasons: std::sync::Arc::new(crate::na_reasons::NaReasonTable::new()),
    })
}

/// `model_matrix(formula, df)` -- bakes a Formula and DataFrame into design matrix X and response y.
/// Reutilizes `Blueprint::bake()` (Roadmap 08, Parte F).
pub(crate) fn native_model_matrix(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`model_matrix()` requires Formula and DataFrame arguments: `model_matrix(formula, df)`",
        ));
    }

    let (response, terms) = match &args[0] {
        Value::Formula {
            response, terms, ..
        } => (response.clone(), terms.clone()),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "First argument of `model_matrix()` must be a Formula, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!(
                    "Second argument of `model_matrix()` must be a DataFrame, found `{}`",
                    other.type_name()
                ),
            ));
        }
    };

    let blueprint = crate::neko::Blueprint::new(response, terms);
    let (x_data, y_data, _disp, n_obs, p_cols, baked_names, _levels) =
        blueprint.bake(frame, na_reasons)?;

    let mut fields = std::collections::BTreeMap::new();
    fields.insert(
        "x".to_string(),
        Value::Matrix {
            rows: n_obs,
            cols: p_cols,
            data: std::sync::Arc::new(x_data),
        },
    );
    fields.insert("y".to_string(), Value::Vector(VectorData::from_f64(y_data)));
    fields.insert(
        "terms".to_string(),
        Value::Vector(VectorData::from_values(
            baked_names.into_iter().map(Value::String).collect(),
        )),
    );
    fields.insert("n_obs".to_string(), Value::I64(n_obs as i64));
    fields.insert("p_cols".to_string(), Value::I64(p_cols as i64));

    Ok(Value::Record(std::sync::Arc::new(fields)))
}
