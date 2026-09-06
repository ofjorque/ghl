//! Sparkline micro-distribution renderer for GHL Cockpit Deck.
//!
//! Renders numeric sequences into compact inline distribution graphs with Unicode
//! block heights (` ▂▃▄▅▆▇█`) or ASCII fallbacks (`.:-=+*#@`).

use crate::caps::RenderCaps;

const UNICODE_TICKS: &[char] = &[' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const ASCII_TICKS: &[char] = &['.', ':', '-', '=', '+', '*', '#', '@'];

#[derive(Debug, Clone)]
pub struct Sparkline;

impl Sparkline {
    /// Render a slice of numbers as a sparkline string.
    ///
    /// If `max_len` is provided and smaller than `values.len()`, the series is downsampled
    /// via bucket averaging.
    pub fn render(values: &[f64], max_len: Option<usize>, caps: &RenderCaps) -> String {
        if values.is_empty() {
            return String::new();
        }

        // Filter out non-finite numbers for min/max computation
        let finite_vals: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
        if finite_vals.is_empty() {
            return if caps.unicode_enabled { "┄".to_string() } else { "-".to_string() };
        }

        let sampled = match max_len {
            Some(limit) if limit > 0 && limit < values.len() => Self::downsample(values, limit),
            _ => values.to_vec(),
        };

        let min = finite_vals.iter().copied().fold(f64::INFINITY, f64::min);
        let max = finite_vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        let ticks = if caps.unicode_enabled {
            UNICODE_TICKS
        } else {
            ASCII_TICKS
        };

        let num_ticks = ticks.len();
        let range = max - min;

        let mut out = String::with_capacity(sampled.len());
        for &v in &sampled {
            if !v.is_finite() {
                out.push('?');
                continue;
            }
            let idx = if range <= 1e-12 {
                num_ticks / 2
            } else {
                let frac = ((v - min) / range).clamp(0.0, 1.0);
                let scaled = (frac * (num_ticks as f64 - 1.0)).round() as usize;
                scaled.min(num_ticks - 1)
            };
            out.push(ticks[idx]);
        }

        out
    }

    /// Downsample values into `target_len` buckets using mean.
    fn downsample(values: &[f64], target_len: usize) -> Vec<f64> {
        let chunk_size = values.len() as f64 / target_len as f64;
        let mut result = Vec::with_capacity(target_len);

        for i in 0..target_len {
            let start = (i as f64 * chunk_size).floor() as usize;
            let end = (((i + 1) as f64 * chunk_size).ceil() as usize).min(values.len());

            if start >= end {
                if let Some(&last) = result.last() {
                    result.push(last);
                }
                continue;
            }

            let slice = &values[start..end];
            let sum: f64 = slice.iter().copied().filter(|v| v.is_finite()).sum();
            let count = slice.iter().filter(|v| v.is_finite()).count();
            if count > 0 {
                result.push(sum / count as f64);
            } else {
                result.push(0.0);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparkline_unicode() {
        let caps = RenderCaps::rich_terminal(80);
        let data = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let spark = Sparkline::render(&data, None, &caps);
        assert_eq!(spark, " ▂▃▄▅▆▇█");
    }

    #[test]
    fn test_sparkline_ascii() {
        let caps = RenderCaps::ascii_plain(80);
        let data = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let spark = Sparkline::render(&data, None, &caps);
        assert_eq!(spark, ".:-=+*#@");
    }

    #[test]
    fn test_sparkline_downsample() {
        let caps = RenderCaps::rich_terminal(80);
        let data: Vec<f64> = (0..100).map(|x| x as f64).collect();
        let spark = Sparkline::render(&data, Some(10), &caps);
        assert_eq!(spark.chars().count(), 10);
    }
}
