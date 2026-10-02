//! Terminal ASCII/Unicode visualization engine for GHL Cockpit Deck.
//!
//! Provides fast inline telemetry, multi-series ANSI color coding,
//! 8-level block histograms (` ▂▃▄▅▆▇█`), and Tukey box-and-whisker diagrams.

use ghl_diagnostics::caps::RenderCaps;
use ghl_diagnostics::panel::visual_width;
use crate::palette::get_okabe_ito_color;
use crate::spec::{GeomKind, HistogramBins, PlotSpec};

pub struct TerminalRenderer;

impl TerminalRenderer {
    pub fn render(spec: &PlotSpec, caps: &RenderCaps) -> String {
        let is_hist = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Histogram { .. }));
        let is_box = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Boxplot { .. }));
        let is_bar = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Bar));

        if is_hist {
            Self::render_histogram(spec, spec.requested_bins(), caps)
        } else if is_box {
            Self::render_boxplot(spec, caps)
        } else if is_bar {
            Self::render_bar(spec, caps)
        } else {
            Self::render_cartesian(spec, caps)
        }
    }

    fn render_cartesian(spec: &PlotSpec, caps: &RenderCaps) -> String {
        let (tl, tr, bl, br, hz, vt) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│')
        } else {
            ('+', '+', '+', '+', '-', '|')
        };

        let card_width = 72.min(caps.width);
        let plot_w = card_width.saturating_sub(18).max(20);
        let plot_h = spec.height.max(8);

        let series_list = spec.all_series();
        if series_list.is_empty() || series_list.iter().all(|s| s.x_values.is_empty() && s.y_values.is_empty()) {
            return format!("{tl}{}{tr}\n{vt} (Empty cartesian data) {vt}\n{bl}{}{br}",
                hz.to_string().repeat(card_width - 2),
                hz.to_string().repeat(card_width - 2)
            );
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for s in &series_list {
            for &x in &s.x_values {
                if x < min_x { min_x = x; }
                if x > max_x { max_x = x; }
            }
            for &y in &s.y_values {
                if y < min_y { min_y = y; }
                if y > max_y { max_y = y; }
            }
        }

        let span_x = if (max_x - min_x).abs() < 1e-9 { 1.0 } else { max_x - min_x };
        let span_y = if (max_y - min_y).abs() < 1e-9 { 1.0 } else { max_y - min_y };

        // Grid canvas of characters and ANSI color prefixes
        let mut grid_chars = vec![vec![' '; plot_w]; plot_h];
        let mut grid_colors: Vec<Vec<Option<&'static str>>> = vec![vec![None; plot_w]; plot_h];

        // Draw smooth trend line if any
        if let Some(fit) = spec.smooth_fit() {
            for c in 0..plot_w {
                let cur_x = min_x + (c as f64 / (plot_w - 1) as f64) * span_x;
                let cur_y = fit.intercept + fit.slope * cur_x;
                let norm_y = ((cur_y - min_y) / span_y).clamp(0.0, 1.0);
                let row = ((1.0 - norm_y) * (plot_h - 1) as f64).round() as usize;
                if row < plot_h {
                    grid_chars[row][c] = if caps.unicode_enabled { '·' } else { '.' };
                    grid_colors[row][c] = Some("\x1b[33m"); // Yellow for trend
                }
            }
        }

        // Draw points from series
        let point_glyph = if caps.unicode_enabled { '●' } else { '*' };
        for (i, s) in series_list.iter().enumerate() {
            let color_palette = get_okabe_ito_color(i);
            let n_pts = s.x_values.len().min(s.y_values.len());

            for pt_idx in 0..n_pts {
                let px = ((s.x_values[pt_idx] - min_x) / span_x).clamp(0.0, 1.0);
                let py = ((s.y_values[pt_idx] - min_y) / span_y).clamp(0.0, 1.0);

                let c = (px * (plot_w - 1) as f64).round() as usize;
                let r = ((1.0 - py) * (plot_h - 1) as f64).round() as usize;

                if r < plot_h && c < plot_w {
                    grid_chars[r][c] = point_glyph;
                    grid_colors[r][c] = Some(color_palette.ansi_code);
                }
            }
        }

        // Build output card
        let title = spec.labels.title.clone().unwrap_or_else(|| "Cartesian Plot".into());
        let badge = if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ READY" } else { "[READY]" };

        let mut out = String::new();
        let header_inner = format!(" {title} ");
        let pad_top = card_width.saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6);
        out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

        if let Some(ref yl) = spec.labels.y_label {
            let y_header = format!("{yl} ▲");
            let y_pad = card_width.saturating_sub(visual_width(&y_header) + 4);
            out.push_str(&format!("{vt} {}{}{vt}\n", caps.bold(&y_header), " ".repeat(y_pad)));
        }

        for r in 0..plot_h {
            let y_val = max_y - (r as f64 / (plot_h - 1) as f64) * span_y;
            let y_label = if r == 0 || r == plot_h / 2 || r == plot_h - 1 {
                format!("{:>7.1}", y_val)
            } else {
                "       ".to_string()
            };

            let tick = if caps.unicode_enabled {
                if r == plot_h - 1 { "┼" } else { "┤" }
            } else {
                if r == plot_h - 1 { "+" } else { "|" }
            };

            let mut row_chars = String::new();
            for c in 0..plot_w {
                let ch = grid_chars[r][c];
                if caps.color_enabled {
                    if let Some(color_code) = grid_colors[r][c] {
                        row_chars.push_str(color_code);
                        row_chars.push(ch);
                        row_chars.push_str("\x1b[0m");
                    } else {
                        row_chars.push(ch);
                    }
                } else {
                    row_chars.push(ch);
                }
            }

            let line_content = format!("{} {} {}", caps.dim(&y_label), caps.dim(tick), row_chars);
            let row_pad = card_width.saturating_sub(visual_width(&line_content) + 4);
            out.push_str(&format!("{vt} {}{}{vt}\n", line_content, " ".repeat(row_pad)));
        }

        // X axis line
        let axis_corner = if caps.unicode_enabled { "└" } else { "+" };
        let axis_line = hz.to_string().repeat(plot_w);
        let x_axis_str = format!("        {} {}", axis_corner, axis_line);
        let x_axis_pad = card_width.saturating_sub(visual_width(&x_axis_str) + 4);
        out.push_str(&format!("{vt} {}{}{vt}\n", caps.dim(&x_axis_str), " ".repeat(x_axis_pad)));

        // X tick labels
        let x_labels_str = format!("         {:<8.1}{:>width$.1}", min_x, max_x, width = plot_w.saturating_sub(8));
        let x_labels_pad = card_width.saturating_sub(visual_width(&x_labels_str) + 4);
        out.push_str(&format!("{vt} {}{}{vt}\n", caps.dim(&x_labels_str), " ".repeat(x_labels_pad)));

        if let Some(ref xl) = spec.labels.x_label {
            let x_center = format!("{xl} ▶");
            let x_pad = card_width.saturating_sub(visual_width(&x_center) + 4);
            out.push_str(&format!("{vt} {}{}{vt}\n", caps.bold(&x_center), " ".repeat(x_pad)));
        }

        // Legend if multiple series
        let grouped_series: Vec<_> = series_list.iter().filter(|s| s.group_name.is_some()).collect();
        if !grouped_series.is_empty() {
            let mut legend_str = String::from("Legend: ");
            for (i, s) in series_list.iter().enumerate() {
                if let Some(ref name) = s.group_name {
                    let color = get_okabe_ito_color(i);
                    if caps.color_enabled {
                        legend_str.push_str(&format!("{}■\x1b[0m {}  ", color.ansi_code, name));
                    } else {
                        legend_str.push_str(&format!("[*] {}  ", name));
                    }
                }
            }
            let leg_pad = card_width.saturating_sub(visual_width(&legend_str) + 4);
            out.push_str(&format!("{vt} {}{}{vt}\n", caps.dim(&legend_str), " ".repeat(leg_pad)));
        }

        out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));
        out
    }

    fn render_histogram(spec: &PlotSpec, bins_count: usize, caps: &RenderCaps) -> String {
        let (tl, tr, bl, br, hz, vt) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│')
        } else {
            ('+', '+', '+', '+', '-', '|')
        };

        let card_width = 72.min(caps.width);
        let HistogramBins { min_x, max_x, bin_width, counts, max_count } = match spec.histogram_bins(bins_count.clamp(4, 24)) {
            Some(b) => b,
            None => {
                return format!("{tl}{}{tr}\n{vt} (Empty histogram data) {vt}\n{bl}{}{br}",
                    hz.to_string().repeat(card_width - 2),
                    hz.to_string().repeat(card_width - 2)
                );
            }
        };

        let title = spec.labels.title.clone().unwrap_or_else(|| "Histogram".into());
        let badge = if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ READY" } else { "[READY]" };

        let mut out = String::new();
        let header_inner = format!(" {title} ");
        let pad_top = card_width.saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6);
        out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

        let bar_levels = [' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let hist_h = 6;

        for level in (1..=hist_h).rev() {
            let mut row = String::new();
            for &c in &counts {
                let frac = c as f64 / max_count.max(1) as f64;
                let height_units = (frac * (hist_h as f64 * 8.0)).round() as usize;

                let char_in_row = if height_units >= level * 8 {
                    if caps.unicode_enabled { '█' } else { '#' }
                } else if height_units > (level - 1) * 8 {
                    let rem = height_units - (level - 1) * 8;
                    if caps.unicode_enabled { bar_levels[rem.clamp(0, 7)] } else { '#' }
                } else {
                    ' '
                };
                row.push(char_in_row);
                row.push(' ');
            }
            let styled_row = caps.cyan(&row);
            let row_pad = card_width.saturating_sub(visual_width(&row) + 6);
            out.push_str(&format!("{vt}   {}{}{vt}\n", styled_row, " ".repeat(row_pad)));
        }

        // Domain summary
        let summary = format!("[{:.1} .. {:.1}]  Bins: {}  BinWidth: {:.2}", min_x, max_x, counts.len(), bin_width);
        let sum_pad = card_width.saturating_sub(visual_width(&summary) + 4);
        out.push_str(&format!("{vt} {}{}{vt}\n", caps.dim(&summary), " ".repeat(sum_pad)));

        out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));
        out
    }

    fn render_boxplot(spec: &PlotSpec, caps: &RenderCaps) -> String {
        let (tl, tr, bl, br, hz, vt) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│')
        } else {
            ('+', '+', '+', '+', '-', '|')
        };

        let card_width = 72.min(caps.width);
        let multi_stats = spec.boxplot_multi_stats();
        if !multi_stats.is_empty() {
            let title = spec.labels.title.clone().unwrap_or_else(|| "Comparative Boxplot".into());
            let badge = if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ READY" } else { "[READY]" };

            let mut out = String::new();
            let header_inner = format!(" {title} ");
            let pad_top = card_width.saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6);
            out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

            let mut global_min = f64::INFINITY;
            let mut global_max = f64::NEG_INFINITY;
            for (_, st) in &multi_stats {
                if st.min < global_min { global_min = st.min; }
                if st.max > global_max { global_max = st.max; }
            }
            let span = (global_max - global_min).abs().max(1e-9);
            let cat_label_w = multi_stats.iter().map(|(c, _)| visual_width(c)).max().unwrap_or(8).min(16);
            let plot_w = card_width.saturating_sub(cat_label_w + 18).max(15);

            for (idx, (cat, st)) in multi_stats.iter().enumerate() {
                let map_val = |v: f64| -> usize {
                    let frac = ((v - global_min) / span).clamp(0.0, 1.0);
                    (frac * (plot_w - 1) as f64).round() as usize
                };

                let lf = map_val(st.lower_fence);
                let q1 = map_val(st.q1);
                let med = map_val(st.median);
                let q3 = map_val(st.q3);
                let uf = map_val(st.upper_fence);

                let mut line = vec![' '; plot_w];
                for i in lf..q1 { line[i] = if caps.unicode_enabled { '─' } else { '-' }; }
                for i in q1..=q3 { line[i] = if caps.unicode_enabled { '█' } else { '=' }; }
                for i in (q3 + 1)..=uf { line[i] = if caps.unicode_enabled { '─' } else { '-' }; }

                line[lf] = if caps.unicode_enabled { '├' } else { '|' };
                line[uf] = if caps.unicode_enabled { '┤' } else { '|' };
                line[med] = if caps.unicode_enabled { '┃' } else { '|' };

                let diagram: String = line.into_iter().collect();
                let color_info = get_okabe_ito_color(idx);
                let colored_diag = if caps.color_enabled {
                    format!("{}{}\x1b[0m", color_info.ansi_code, diagram)
                } else {
                    diagram.clone()
                };

                let line_str = format!(" {:<width$} {} Med: {:>6.1}", cat, colored_diag, st.median, width = cat_label_w);
                let line_raw = format!(" {:<width$} {} Med: {:>6.1}", cat, diagram, st.median, width = cat_label_w);
                let line_pad = card_width.saturating_sub(visual_width(&line_raw) + 4);
                out.push_str(&format!("{vt}{}{}{vt}\n", line_str, " ".repeat(line_pad)));
            }

            let scale_summary = format!(" Domain: [{:.1} .. {:.1}]  Categories: {}", global_min, global_max, multi_stats.len());
            let sc_pad = card_width.saturating_sub(visual_width(&scale_summary) + 4);
            out.push_str(&format!("{vt}{}{}{vt}\n", caps.dim(&scale_summary), " ".repeat(sc_pad)));

            out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));
            return out;
        }

        let stats = match spec.boxplot_stats() {
            Some(s) => s,
            None => {
                return format!("{tl}{}{tr}\n{vt} (No boxplot statistics) {vt}\n{bl}{}{br}",
                    hz.to_string().repeat(card_width - 2),
                    hz.to_string().repeat(card_width - 2)
                );
            }
        };

        let title = spec.labels.title.clone().unwrap_or_else(|| "Boxplot".into());
        let badge = if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ READY" } else { "[READY]" };

        let mut out = String::new();
        let header_inner = format!(" {title} ");
        let pad_top = card_width.saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6);
        out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

        let plot_w = card_width.saturating_sub(16).max(20);
        let span = (stats.max - stats.min).abs().max(1e-9);

        let map_val = |v: f64| -> usize {
            let frac = ((v - stats.min) / span).clamp(0.0, 1.0);
            (frac * (plot_w - 1) as f64).round() as usize
        };

        let lf = map_val(stats.lower_fence);
        let q1 = map_val(stats.q1);
        let med = map_val(stats.median);
        let q3 = map_val(stats.q3);
        let uf = map_val(stats.upper_fence);

        let mut line = vec![' '; plot_w];
        for i in lf..q1 { line[i] = if caps.unicode_enabled { '─' } else { '-' }; }
        for i in q1..=q3 { line[i] = if caps.unicode_enabled { '█' } else { '=' }; }
        for i in (q3 + 1)..=uf { line[i] = if caps.unicode_enabled { '─' } else { '-' }; }

        line[lf] = if caps.unicode_enabled { '├' } else { '|' };
        line[uf] = if caps.unicode_enabled { '┤' } else { '|' };
        line[med] = if caps.unicode_enabled { '┃' } else { '|' };

        let diagram: String = line.into_iter().collect();
        let diag_pad = card_width.saturating_sub(visual_width(&diagram) + 6);
        out.push_str(&format!("{vt}   {}{}{vt}\n", caps.green(&diagram), " ".repeat(diag_pad)));

        let stats_info = format!("Min: {:.1}  Q1: {:.1}  Med: {:.1}  Q3: {:.1}  Max: {:.1}", stats.min, stats.q1, stats.median, stats.q3, stats.max);
        let info_pad = card_width.saturating_sub(visual_width(&stats_info) + 4);
        out.push_str(&format!("{vt} {}{}{vt}\n", caps.dim(&stats_info), " ".repeat(info_pad)));

        out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));
        out
    }

    fn render_bar(spec: &PlotSpec, caps: &RenderCaps) -> String {
        let (tl, tr, bl, br, hz, vt) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│')
        } else {
            ('+', '+', '+', '+', '-', '|')
        };

        let card_width = 72.min(caps.width);
        let counts_map = match spec.bar_counts() {
            Some(c) => c,
            None => {
                return format!("{tl}{}{tr}\n{vt} (Empty bar data) {vt}\n{bl}{}{br}",
                    hz.to_string().repeat(card_width - 2),
                    hz.to_string().repeat(card_width - 2)
                );
            }
        };

        let title = spec.labels.title.clone().unwrap_or_else(|| "Bar Chart".into());
        let badge = if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ READY" } else { "[READY]" };

        let mut out = String::new();
        let header_inner = format!(" {title} ");
        let pad_top = card_width.saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6);
        out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

        let max_count = counts_map.values().copied().max().unwrap_or(1);
        let bar_max_w = card_width.saturating_sub(28).max(10);

        for (cat, &cnt) in &counts_map {
            let frac = cnt as f64 / max_count as f64;
            let bar_len = (frac * bar_max_w as f64).round() as usize;
            let bar_str = if caps.unicode_enabled { "█".repeat(bar_len) } else { "#".repeat(bar_len) };
            let line = format!("{:<12} {} ({})", cat, caps.cyan(&bar_str), cnt);
            let row_pad = card_width.saturating_sub(visual_width(&format!("{:<12} {} ({})", cat, bar_str, cnt)) + 4);
            out.push_str(&format!("{vt} {}{}{vt}\n", line, " ".repeat(row_pad)));
        }

        out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));
        out
    }
}
