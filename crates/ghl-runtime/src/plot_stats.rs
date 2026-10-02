//! Statistics feeding `ghl-diagnostics`'s plotting geoms (RFC 09).
//!
//! `ghl-diagnostics` only renders plots — it never fits a model or computes a
//! summary statistic itself, so a trend line or boxplot always looks the same
//! whether it's drawn to a terminal or exported to PNG/SVG. This module is
//! where that computation actually happens, next to (but deliberately
//! separate from) [`crate::neko`]'s full formula-based `fit_ols`: a
//! `geom_smooth()` trend line is a quick bivariate fit, not a fitted model
//! with standard errors, so it doesn't need `Blueprint`/`DataFrame` machinery.
//! Anything reusing the same statistic in the future (e.g. a GLMM or IRT
//! diagnostic plot) should call these functions rather than reimplementing
//! them again at the call site.

use ghl_plot::{FiveNumberSummary, LinearFit};

/// Ordinary least squares fit of `y = slope * x + intercept` over paired
/// `(x, y)` observations. Returns `None` when there are fewer than 2 points
/// or `x` has no variance (a vertical/degenerate fit).
pub fn simple_linear_fit(x: &[f64], y: &[f64]) -> Option<LinearFit> {
    let n = x.len().min(y.len());
    if n < 2 {
        return None;
    }

    let mean_x = x[..n].iter().sum::<f64>() / n as f64;
    let mean_y = y[..n].iter().sum::<f64>() / n as f64;

    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..n {
        let dx = x[i] - mean_x;
        num += dx * (y[i] - mean_y);
        den += dx * dx;
    }

    if den.abs() <= 1e-9 {
        return None;
    }

    let slope = num / den;
    let intercept = mean_y - slope * mean_x;
    Some(LinearFit { slope, intercept })
}

/// Tukey five-number summary (min/Q1/median/Q3/max) plus 1.5×IQR fences and
/// the observations that fall outside them. Returns `None` when there are
/// fewer than 4 observations, mirroring the minimum `geom_boxplot()` needs to
/// produce a meaningful box.
pub fn five_number_summary(data: &[f64]) -> Option<FiveNumberSummary> {
    let n = data.len();
    if n < 4 {
        return None;
    }

    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min = sorted[0];
    let max = sorted[n - 1];
    let q1 = sorted[n / 4];
    let median = sorted[n / 2];
    let q3 = sorted[(3 * n) / 4];
    let iqr = q3 - q1;
    let lower_fence = (q1 - 1.5 * iqr).max(min);
    let upper_fence = (q3 + 1.5 * iqr).min(max);
    let mean = sorted.iter().sum::<f64>() / n as f64;

    let outliers: Vec<f64> = sorted
        .iter()
        .copied()
        .filter(|&v| v < lower_fence || v > upper_fence)
        .collect();

    Some(FiveNumberSummary {
        min,
        q1,
        median,
        q3,
        max,
        mean,
        lower_fence,
        upper_fence,
        outliers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_linear_fit_exact_line() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y = vec![10.0, 20.0, 30.0, 40.0];
        let fit = simple_linear_fit(&x, &y).expect("fit must succeed");
        assert!((fit.slope - 10.0).abs() < 1e-9);
        assert!((fit.intercept - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_simple_linear_fit_insufficient_or_degenerate() {
        assert!(simple_linear_fit(&[1.0], &[1.0]).is_none());
        assert!(simple_linear_fit(&[5.0, 5.0, 5.0], &[1.0, 2.0, 3.0]).is_none());
    }

    #[test]
    fn test_five_number_summary_with_outlier() {
        let data = vec![10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 100.0];
        let stats = five_number_summary(&data).expect("summary must succeed");
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.q1, 20.0);
        assert_eq!(stats.median, 30.0);
        assert_eq!(stats.q3, 40.0);
        assert_eq!(stats.max, 100.0);
        assert!((stats.mean - 34.375).abs() < 1e-9);
        assert_eq!(stats.lower_fence, 10.0);
        assert_eq!(stats.upper_fence, 70.0);
        assert_eq!(stats.outliers, vec![100.0]);
    }

    #[test]
    fn test_five_number_summary_too_few_points() {
        assert!(five_number_summary(&[1.0, 2.0, 3.0]).is_none());
    }
}
