//! NEKO statistical model native invocations (fit/ols/predict/tidy/...).
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use ghl_diagnostics::Diagnostic;
use crate::value::Value;
use crate::vector_data::VectorData;



// NEKO Native Invocations

pub(crate) fn native_fit_ols(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`fit()` requires Formula and DataFrame arguments: `fit(model, df)`",
        ));
    }

    let (response, terms) = match &args[0] {
        Value::Formula { response, terms } => (response.clone(), terms.clone()),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `fit()` must be a Formula, found `{}`", other.type_name()),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `fit()` must be a DataFrame, found `{}`", other.type_name()),
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
        Value::Formula { response, terms } => (response.clone(), terms.clone()),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `fit_logistic()` must be a Formula, found `{}`", other.type_name()),
            ));
        }
    };

    let (frame, na_reasons) = match &args[1] {
        Value::DataFrame { frame, na_reasons } => (frame, na_reasons),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `fit_logistic()` must be a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };
    let blueprint = crate::neko::Blueprint::new(response, terms);
    let model = crate::glm::FittedGlm::fit_logistic(blueprint, frame, na_reasons)?;
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
        _ => return Err(Diagnostic::statistical_error(
            "S0200",
            "Second argument of `fit_gmm` must be a positive integer k (number of clusters)",
        )),
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

pub(crate) fn native_summary(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`summary()` requires a ModelFit or printable object")
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
        other => {
            println!("{}", other);
            Ok(Value::Unit)
        }
    }
}

pub(crate) fn native_tidy(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`tidy()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(m.tidy()),
        Value::GlmFit(m) => Ok(m.tidy()),
        Value::GmmFit(m) => Ok(m.tidy()),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`tidy()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn native_glance(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`glance()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(m.glance()),
        Value::GlmFit(m) => Ok(m.glance()),
        Value::GmmFit(m) => Ok(m.glance()),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`glance()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn native_augment(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`augment()` requires ModelFit and DataFrame arguments: `augment(model, df)`",
        ));
    }

    match &args[0] {
        Value::ModelFit(m) => m.augment(&args[1]),
        Value::GlmFit(m) => m.augment(&args[1]),
        Value::GmmFit(m) => m.augment(&args[1]),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("First argument of `augment()` must be a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn native_predict(args: Vec<Value>) -> Result<Value, Diagnostic> {
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
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("First argument of `predict()` must be a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn native_residuals(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`residuals()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            Ok(Value::Vector(VectorData::from_f64(m.residuals.clone())))
        }
        Value::GlmFit(m) => {
            Ok(Value::Vector(VectorData::from_f64(m.residuals.clone())))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`residuals()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn native_coef(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`coef()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            Ok(Value::Vector(VectorData::from_f64(m.coefficients.clone())))
        }
        Value::GlmFit(m) => {
            Ok(Value::Vector(VectorData::from_f64(m.coefficients.clone())))
        }
        Value::GmmFit(m) => {
            Ok(Value::Matrix {
                rows: m.k,
                cols: m.dim,
                data: std::sync::Arc::new(m.means.clone()),
            })
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`coef()` requires a ModelFit, found `{}`", other.type_name()),
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

pub(crate) fn native_vcov(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`vcov()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            let kind = vcov_kind_from_arg(&args);
            let p = m.blueprint.term_names.len();
            let vcov_data = m.compute_vcov(kind)?;
            Ok(Value::Matrix { rows: p, cols: p, data: std::sync::Arc::new(vcov_data) })
        }
        Value::GlmFit(m) => {
            let kind = vcov_kind_from_arg(&args);
            let p = m.blueprint.term_names.len();
            let vcov_data = m.compute_vcov(kind)?;
            Ok(Value::Matrix { rows: p, cols: p, data: std::sync::Arc::new(vcov_data) })
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`vcov()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}
