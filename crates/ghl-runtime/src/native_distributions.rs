//! Native statistical probability distributions from `statrs` (RFC 15 / Roadmap 08 Parte F).
//!
//! Exposes canonical quadruple interface:
//! - `pdf` / `pmf`
//! - `cdf`
//! - `quantile` (inverse CDF)
//! - `sample` / `random_*`
//!
//! Supporting Continuous (Normal, StudentT, FisherSnedecor, ChiSquared, Gamma, Beta, Uniform, Exp)
//! and Discrete (Binomial, Poisson) distributions.

use crate::native_core::sample_distribution;
use crate::value::Value;
use crate::vector_data::VectorData;
use ghl_diagnostics::Diagnostic;
use statrs::distribution::{
    Beta, ChiSquared, Continuous, ContinuousCDF, Discrete, DiscreteCDF, Exp, FisherSnedecor, Gamma,
    Normal, StudentsT, Uniform,
};

// ============================================================================
// Continuous: Normal (quantile extension)
// ============================================================================

/// `normal_quantile(p, mean, sd)` -- quantile (inverse CDF) function for Normal(mean, sd).
pub(crate) fn native_normal_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let p = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`normal_quantile()` requires a probability first argument (p in [0, 1])",
        )
    })?;
    let mean = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`normal_quantile()` second argument (mean) must be numeric",
        )
    })?;
    let sd = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`normal_quantile()` third argument (sd) must be numeric",
        )
    })?;
    let dist = Normal::new(mean, sd)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`normal_quantile()`: {e}")))?;
    Ok(Value::F64(dist.inverse_cdf(p)))
}

// ============================================================================
// Continuous: Student-t
// ============================================================================

fn parse_student_t_args(func_name: &str, args: &[Value]) -> Result<(f64, StudentsT), Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` requires a numeric first argument"),
        )
    })?;
    let df = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` second argument (df) must be numeric"),
        )
    })?;
    let loc = args.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let scale = args.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0);
    let dist = StudentsT::new(loc, scale, df)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`{func_name}()`: {e}")))?;
    Ok((x, dist))
}

/// `student_t_pdf(x, df, [loc=0, scale=1])` -- probability density function for Student-t.
pub(crate) fn native_student_t_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_student_t_args("student_t_pdf", &args)?;
    Ok(Value::F64(dist.pdf(x)))
}

/// `student_t_cdf(x, df, [loc=0, scale=1])` -- cumulative distribution function for Student-t.
pub(crate) fn native_student_t_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_student_t_args("student_t_cdf", &args)?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `student_t_quantile(p, df, [loc=0, scale=1])` -- inverse CDF (quantile) function for Student-t.
pub(crate) fn native_student_t_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (p, dist) = parse_student_t_args("student_t_quantile", &args)?;
    Ok(Value::F64(dist.inverse_cdf(p)))
}

/// `random_student_t(n, df, [loc=0, scale=1, seed=None])` -- random sampling from Student-t.
pub(crate) fn native_random_student_t(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_student_t()` requires an integer length as first argument",
        )
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_student_t()` length must be non-negative, found {n}"),
        ));
    }
    let df = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_student_t()` second argument (df) must be numeric",
        )
    })?;
    let loc = args.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let scale = args.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0);
    let seed = args.get(4).and_then(|v| v.as_i64());
    let dist = StudentsT::new(loc, scale, df)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`random_student_t()`: {e}")))?;
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

// ============================================================================
// Continuous: Fisher-Snedecor (F)
// ============================================================================

fn parse_f_dist_args(func_name: &str, args: &[Value]) -> Result<(f64, FisherSnedecor), Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` requires a numeric first argument"),
        )
    })?;
    let df1 = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` second argument (df1) must be numeric"),
        )
    })?;
    let df2 = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` third argument (df2) must be numeric"),
        )
    })?;
    let dist = FisherSnedecor::new(df1, df2)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`{func_name}()`: {e}")))?;
    Ok((x, dist))
}

/// `f_dist_pdf(x, df1, df2)` -- probability density function for Fisher-Snedecor F.
pub(crate) fn native_f_dist_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_f_dist_args("f_dist_pdf", &args)?;
    Ok(Value::F64(dist.pdf(x)))
}

/// `f_dist_cdf(x, df1, df2)` -- cumulative distribution function for Fisher-Snedecor F.
pub(crate) fn native_f_dist_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_f_dist_args("f_dist_cdf", &args)?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `f_dist_quantile(p, df1, df2)` -- inverse CDF for Fisher-Snedecor F.
pub(crate) fn native_f_dist_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (p, dist) = parse_f_dist_args("f_dist_quantile", &args)?;
    Ok(Value::F64(dist.inverse_cdf(p)))
}

/// `random_f_dist(n, df1, df2, [seed=None])` -- random sampling from Fisher-Snedecor F.
pub(crate) fn native_random_f_dist(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_f_dist()` requires an integer length as first argument",
        )
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_f_dist()` length must be non-negative, found {n}"),
        ));
    }
    let df1 = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_f_dist()` second argument (df1) must be numeric",
        )
    })?;
    let df2 = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_f_dist()` third argument (df2) must be numeric",
        )
    })?;
    let seed = args.get(3).and_then(|v| v.as_i64());
    let dist = FisherSnedecor::new(df1, df2)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`random_f_dist()`: {e}")))?;
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

// ============================================================================
// Continuous: Chi-Squared (ChiSq)
// ============================================================================

fn parse_chisq_args(func_name: &str, args: &[Value]) -> Result<(f64, ChiSquared), Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` requires a numeric first argument"),
        )
    })?;
    let df = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` second argument (df) must be numeric"),
        )
    })?;
    let dist = ChiSquared::new(df)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`{func_name}()`: {e}")))?;
    Ok((x, dist))
}

/// `chisq_pdf(x, df)` -- probability density function for Chi-squared.
pub(crate) fn native_chisq_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_chisq_args("chisq_pdf", &args)?;
    Ok(Value::F64(dist.pdf(x)))
}

/// `chisq_cdf(x, df)` -- cumulative distribution function for Chi-squared.
pub(crate) fn native_chisq_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_chisq_args("chisq_cdf", &args)?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `chisq_quantile(p, df)` -- inverse CDF for Chi-squared.
pub(crate) fn native_chisq_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (p, dist) = parse_chisq_args("chisq_quantile", &args)?;
    Ok(Value::F64(dist.inverse_cdf(p)))
}

/// `random_chisq(n, df, [seed=None])` -- random sampling from Chi-squared.
pub(crate) fn native_random_chisq(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_chisq()` requires an integer length as first argument",
        )
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_chisq()` length must be non-negative, found {n}"),
        ));
    }
    let df = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_chisq()` second argument (df) must be numeric",
        )
    })?;
    let seed = args.get(2).and_then(|v| v.as_i64());
    let dist = ChiSquared::new(df)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`random_chisq()`: {e}")))?;
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

// ============================================================================
// Continuous: Gamma (quantile extension)
// ============================================================================

/// `gamma_quantile(p, shape, rate)` -- inverse CDF for Gamma(shape, rate).
pub(crate) fn native_gamma_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let p = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`gamma_quantile()` requires a probability first argument (p in [0, 1])",
        )
    })?;
    let shape = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`gamma_quantile()` second argument (shape) must be numeric",
        )
    })?;
    let rate = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`gamma_quantile()` third argument (rate) must be numeric",
        )
    })?;
    let dist = Gamma::new(shape, rate)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`gamma_quantile()`: {e}")))?;
    Ok(Value::F64(dist.inverse_cdf(p)))
}

// ============================================================================
// Continuous: Beta
// ============================================================================

fn parse_beta_args(func_name: &str, args: &[Value]) -> Result<(f64, Beta), Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` requires a numeric first argument"),
        )
    })?;
    let a = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` second argument (shape_a) must be numeric"),
        )
    })?;
    let b = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` third argument (shape_b) must be numeric"),
        )
    })?;
    let dist = Beta::new(a, b)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`{func_name}()`: {e}")))?;
    Ok((x, dist))
}

/// `beta_pdf(x, shape_a, shape_b)` -- probability density function for Beta.
pub(crate) fn native_beta_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_beta_args("beta_pdf", &args)?;
    Ok(Value::F64(dist.pdf(x)))
}

/// `beta_cdf(x, shape_a, shape_b)` -- cumulative distribution function for Beta.
pub(crate) fn native_beta_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_beta_args("beta_cdf", &args)?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `beta_quantile(p, shape_a, shape_b)` -- inverse CDF for Beta.
pub(crate) fn native_beta_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (p, dist) = parse_beta_args("beta_quantile", &args)?;
    Ok(Value::F64(dist.inverse_cdf(p)))
}

/// `random_beta(n, shape_a, shape_b, [seed=None])` -- random sampling from Beta.
pub(crate) fn native_random_beta(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_beta()` requires an integer length as first argument",
        )
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_beta()` length must be non-negative, found {n}"),
        ));
    }
    let a = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_beta()` second argument (shape_a) must be numeric",
        )
    })?;
    let b = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_beta()` third argument (shape_b) must be numeric",
        )
    })?;
    let seed = args.get(3).and_then(|v| v.as_i64());
    let dist = Beta::new(a, b)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`random_beta()`: {e}")))?;
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

// ============================================================================
// Continuous: Uniform
// ============================================================================

fn parse_uniform_args(func_name: &str, args: &[Value]) -> Result<(f64, Uniform), Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` requires a numeric first argument"),
        )
    })?;
    let min = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` second argument (min) must be numeric"),
        )
    })?;
    let max = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` third argument (max) must be numeric"),
        )
    })?;
    let dist = Uniform::new(min, max)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`{func_name}()`: {e}")))?;
    Ok((x, dist))
}

/// `uniform_pdf(x, min, max)` -- probability density function for Uniform.
pub(crate) fn native_uniform_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_uniform_args("uniform_pdf", &args)?;
    Ok(Value::F64(dist.pdf(x)))
}

/// `uniform_cdf(x, min, max)` -- cumulative distribution function for Uniform.
pub(crate) fn native_uniform_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_uniform_args("uniform_cdf", &args)?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `uniform_quantile(p, min, max)` -- inverse CDF for Uniform.
pub(crate) fn native_uniform_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (p, dist) = parse_uniform_args("uniform_quantile", &args)?;
    Ok(Value::F64(dist.inverse_cdf(p)))
}

// ============================================================================
// Continuous: Exponential
// ============================================================================

fn parse_exp_args(func_name: &str, args: &[Value]) -> Result<(f64, Exp), Diagnostic> {
    let x = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` requires a numeric first argument"),
        )
    })?;
    let rate = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            format!("`{func_name}()` second argument (rate) must be numeric"),
        )
    })?;
    let dist = Exp::new(rate)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`{func_name}()`: {e}")))?;
    Ok((x, dist))
}

/// `exp_pdf(x, rate)` -- probability density function for Exponential.
pub(crate) fn native_exp_pdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_exp_args("exp_pdf", &args)?;
    Ok(Value::F64(dist.pdf(x)))
}

/// `exp_cdf(x, rate)` -- cumulative distribution function for Exponential.
pub(crate) fn native_exp_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (x, dist) = parse_exp_args("exp_cdf", &args)?;
    Ok(Value::F64(dist.cdf(x)))
}

/// `exp_quantile(p, rate)` -- inverse CDF for Exponential.
pub(crate) fn native_exp_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (p, dist) = parse_exp_args("exp_quantile", &args)?;
    Ok(Value::F64(dist.inverse_cdf(p)))
}

/// `random_exp(n, rate, [seed=None])` -- random sampling from Exponential.
pub(crate) fn native_random_exp(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_exp()` requires an integer length as first argument",
        )
    })?;
    if n < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_exp()` length must be non-negative, found {n}"),
        ));
    }
    let rate = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_exp()` second argument (rate) must be numeric",
        )
    })?;
    let seed = args.get(2).and_then(|v| v.as_i64());
    let dist = Exp::new(rate)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`random_exp()`: {e}")))?;
    let data = sample_distribution(n as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

// ============================================================================
// Discrete: Binomial
// ============================================================================

/// `binomial_pmf(k, n_trials, p)` -- probability mass function for Binomial.
pub(crate) fn native_binomial_pmf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let k = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_pmf()` requires an integer first argument (k)",
        )
    })?;
    let n = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_pmf()` second argument (n_trials) must be integer",
        )
    })?;
    let p = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_pmf()` third argument (p) must be numeric in [0, 1]",
        )
    })?;
    if k < 0 || n < 0 || k > n {
        return Ok(Value::F64(0.0));
    }
    use statrs::distribution::Binomial;
    let dist = Binomial::new(p, n as u64)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`binomial_pmf()`: {e}")))?;
    Ok(Value::F64(dist.pmf(k as u64)))
}

/// `binomial_cdf(k, n_trials, p)` -- cumulative distribution function for Binomial.
pub(crate) fn native_binomial_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let k = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_cdf()` requires an integer first argument (k)",
        )
    })?;
    let n = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_cdf()` second argument (n_trials) must be integer",
        )
    })?;
    let p = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_cdf()` third argument (p) must be numeric in [0, 1]",
        )
    })?;
    if k < 0 {
        return Ok(Value::F64(0.0));
    }
    if k >= n {
        return Ok(Value::F64(1.0));
    }
    use statrs::distribution::Binomial;
    let dist = Binomial::new(p, n as u64)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`binomial_cdf()`: {e}")))?;
    Ok(Value::F64(dist.cdf(k as u64)))
}

/// `binomial_quantile(prob, n_trials, p)` -- inverse CDF (quantile) for Binomial.
pub(crate) fn native_binomial_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let prob = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_quantile()` requires a probability first argument (prob in [0, 1])",
        )
    })?;
    let n = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_quantile()` second argument (n_trials) must be integer",
        )
    })?;
    let p = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`binomial_quantile()` third argument (p) must be numeric in [0, 1]",
        )
    })?;
    if !(0.0..=1.0).contains(&prob) {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`binomial_quantile()` prob must be in [0, 1], found {prob}"),
        ));
    }
    use statrs::distribution::Binomial;
    let dist = Binomial::new(p, n as u64)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`binomial_quantile()`: {e}")))?;
    Ok(Value::I64(dist.inverse_cdf(prob) as i64))
}

/// `random_binomial(n_samples, n_trials, p, [seed=None])` -- random sampling from Binomial.
pub(crate) fn native_random_binomial(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n_samples = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_binomial()` requires an integer length as first argument",
        )
    })?;
    if n_samples < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_binomial()` length must be non-negative, found {n_samples}"),
        ));
    }
    let n_trials = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_binomial()` second argument (n_trials) must be integer",
        )
    })?;
    let p = args.get(2).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_binomial()` third argument (p) must be numeric in [0, 1]",
        )
    })?;
    let seed = args.get(3).and_then(|v| v.as_i64());
    use statrs::distribution::Binomial;
    let dist = Binomial::new(p, n_trials as u64)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`random_binomial()`: {e}")))?;
    // Sampling discrete distribution
    let data = sample_distribution(n_samples as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}

// ============================================================================
// Discrete: Poisson
// ============================================================================

/// `poisson_pmf(k, lambda)` -- probability mass function for Poisson.
pub(crate) fn native_poisson_pmf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let k = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`poisson_pmf()` requires an integer first argument (k)",
        )
    })?;
    let lambda = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`poisson_pmf()` second argument (lambda) must be numeric",
        )
    })?;
    if k < 0 {
        return Ok(Value::F64(0.0));
    }
    use statrs::distribution::Poisson;
    let dist = Poisson::new(lambda)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`poisson_pmf()`: {e}")))?;
    Ok(Value::F64(dist.pmf(k as u64)))
}

/// `poisson_cdf(k, lambda)` -- cumulative distribution function for Poisson.
pub(crate) fn native_poisson_cdf(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let k = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`poisson_cdf()` requires an integer first argument (k)",
        )
    })?;
    let lambda = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`poisson_cdf()` second argument (lambda) must be numeric",
        )
    })?;
    if k < 0 {
        return Ok(Value::F64(0.0));
    }
    use statrs::distribution::Poisson;
    let dist = Poisson::new(lambda)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`poisson_cdf()`: {e}")))?;
    Ok(Value::F64(dist.cdf(k as u64)))
}

/// `poisson_quantile(prob, lambda)` -- inverse CDF (quantile) for Poisson.
pub(crate) fn native_poisson_quantile(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let prob = args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`poisson_quantile()` requires a probability first argument (prob in [0, 1])",
        )
    })?;
    let lambda = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`poisson_quantile()` second argument (lambda) must be numeric",
        )
    })?;
    if !(0.0..=1.0).contains(&prob) {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`poisson_quantile()` prob must be in [0, 1], found {prob}"),
        ));
    }
    use statrs::distribution::Poisson;
    let dist = Poisson::new(lambda)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`poisson_quantile()`: {e}")))?;
    Ok(Value::I64(dist.inverse_cdf(prob) as i64))
}

/// `random_poisson(n_samples, lambda, [seed=None])` -- random sampling from Poisson.
pub(crate) fn native_random_poisson(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let n_samples = args.first().and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_poisson()` requires an integer length as first argument",
        )
    })?;
    if n_samples < 0 {
        return Err(Diagnostic::compute_error(
            "C0201",
            format!("`random_poisson()` length must be non-negative, found {n_samples}"),
        ));
    }
    let lambda = args.get(1).and_then(|v| v.as_f64()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0201",
            "`random_poisson()` second argument (lambda) must be numeric",
        )
    })?;
    let seed = args.get(2).and_then(|v| v.as_i64());
    use statrs::distribution::Poisson;
    let dist = Poisson::new(lambda)
        .map_err(|e| Diagnostic::compute_error("C0201", format!("`random_poisson()`: {e}")))?;
    let data = sample_distribution(n_samples as usize, &dist, seed);
    Ok(Value::Vector(VectorData::from_f64(data)))
}
