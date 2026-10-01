//! Scalar + `Vector[f64]` math helpers, NaN-safe (never panics; NaN -> NA).
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use ghl_diagnostics::Diagnostic;
use rayon::prelude::*;
use crate::value::Value;
use crate::vector_data::VectorData;


// =========================================================================
// Math helpers — scalar + `Vector[f64]`, NaN-safe (never panics; NaN -> NA)
// =========================================================================

pub(crate) fn map_numeric_fn(v: &Value, f: impl Fn(f64) -> f64 + Clone + Sync) -> Value {
    match v {
        Value::NA(r) => Value::NA(r.clone()),
        // Fast path: no NA in the input -> read/write straight through `&[f64]`, zero
        // `Value` boxing. Can't use `VectorData::from_f64` for the output, though: `f`
        // can still produce a NaN even from a clean input (`pow(-8.0, 0.5)`), so the
        // output needs `from_f64_opt`'s `Option<f64>` to represent that per-element NA
        // without falling back to the boxed path entirely.
        Value::Vector(vd) if vd.null_count() == 0 => {
            match vd.as_f64_view() {
                Ok(view) => {
                    let base = view.as_slice();
                    let compute = |&x: &f64| -> Option<f64> {
                        let y = f(x);
                        if y.is_nan() { None } else { Some(y) }
                    };
                    let data: Vec<Option<f64>> = if base.len() >= crate::eval::PARALLEL_THRESHOLD {
                        base.par_iter().map(compute).collect()
                    } else {
                        base.iter().map(compute).collect()
                    };
                    Value::Vector(VectorData::from_f64_opt(data))
                }
                // Not actually numeric (e.g. Vector[String]) -- fall through to the
                // generic boxed path below, which already reports `NotNumeric` per
                // element rather than failing the whole call.
                Err(_) => Value::Vector(VectorData::from_values(vd.iter().map(|it| map_numeric_fn(it, f.clone())).collect())),
            }
        }
        Value::Vector(vd) => Value::Vector(VectorData::from_values(vd.iter().map(|it| map_numeric_fn(it, f.clone())).collect())),
        Value::Matrix { rows, cols, data } => {
            let slice = data.as_slice();
            let compute = |&x: &f64| -> f64 {
                let y = f(x);
                if y.is_nan() { 0.0 } else { y }
            };
            let out_data: Vec<f64> = if slice.len() >= crate::eval::PARALLEL_THRESHOLD {
                slice.par_iter().map(compute).collect()
            } else {
                slice.iter().map(compute).collect()
            };
            Value::matrix(*rows, *cols, out_data)
        }
        other => match other.as_f64() {
            Some(x) => {
                let y = f(x);
                if y.is_nan() { Value::NA(Some("NaN".into())) } else { Value::F64(y) }
            }
            None => Value::NA(Some(format!("NotNumeric:{}", other.type_name()))),
        },
    }
}

macro_rules! native_math_fn {
    ($name:ident, $f:expr) => {
        pub(crate) fn $name(args: Vec<Value>) -> Result<Value, Diagnostic> {
            let v = args.first().ok_or_else(|| {
                Diagnostic::compute_error("C0201", concat!("`", stringify!($name), "()` requires 1 argument"))
            })?;
            Ok(map_numeric_fn(v, $f))
        }
    };
}

native_math_fn!(native_log, f64::ln);
native_math_fn!(native_log2, f64::log2);
native_math_fn!(native_log10, f64::log10);
native_math_fn!(native_exp, f64::exp);
native_math_fn!(native_sqrt, f64::sqrt);
native_math_fn!(native_abs, f64::abs);
native_math_fn!(native_floor, f64::floor);
native_math_fn!(native_ceil, f64::ceil);
// `sin`/`cos` didn't exist before Punto 3 -- added because Caso 1.4's own reference
// expression (`log(1.0 + exp(-abs(xi))) + sin(xi)`) needs `sin` to actually run
// end-to-end, not a simplified stand-in for it.
native_math_fn!(native_sin, f64::sin);
native_math_fn!(native_cos, f64::cos);

pub(crate) fn native_pow(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let base = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`pow()` requires 2 arguments"))?;
    let exp = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`pow()` second argument must be numeric")
    })?;
    Ok(map_numeric_fn(base, move |x| x.powf(exp)))
}

pub(crate) fn native_round(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`round()` requires at least 1 argument"))?;
    let digits = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
    let factor = 10f64.powi(digits as i32);
    Ok(map_numeric_fn(v, move |x| (x * factor).round() / factor))
}

pub(crate) fn native_clamp(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let v = args.first().ok_or_else(|| Diagnostic::compute_error("C0201", "`clamp()` requires 3 arguments"))?;
    let lo = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`clamp()` second argument (lo) must be numeric")
    })?;
    let hi = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`clamp()` third argument (hi) must be numeric")
    })?;
    Ok(map_numeric_fn(v, move |x| x.max(lo).min(hi)))
}
