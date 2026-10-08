//! Terminal ASCII/Unicode visualization engine for GHL Cockpit Deck.
//!
//! Provides fast inline telemetry, multi-series ANSI color coding,
//! 8-level block histograms (` ▂▃▄▅▆▇█`), and Tukey box-and-whisker diagrams.

use crate::palette::get_okabe_ito_color;
use crate::spec::{GeomKind, HistogramBins, PlotSpec};
use ghl_diagnostics::caps::RenderCaps;
use ghl_diagnostics::panel::visual_width;

pub struct TerminalRenderer;

impl TerminalRenderer {
    pub fn render(spec: &PlotSpec, caps: &RenderCaps) -> String {
        if let Some(ref comp) = spec.composite {
            return match comp.as_ref() {
                crate::spec::CompositePlot::Horizontal(left, right) => {
                    let l_str = Self::render(left, caps);
                    let r_str = Self::render(right, caps);
                    format!("{l_str}\n\n{r_str}")
                }
                crate::spec::CompositePlot::Vertical(top, bottom) => {
                    let t_str = Self::render(top, caps);
                    let b_str = Self::render(bottom, caps);
                    format!("{t_str}\n\n{b_str}")
                }
            };
        }

        if spec.facet.is_some() {
            let (panels, _, _) = spec.partition_facets();
            let mut out = String::new();
            if let Some(ref title) = spec.labels.title {
                out.push_str(&format!("=== {} ===\n\n", title));
            }
            for (idx, panel) in panels.into_iter().enumerate() {
                if idx > 0 {
                    out.push_str("\n\n");
                }
                let mut panel_spec = panel.spec;
                panel_spec.labels.title = Some(format!("[ {} ]", panel.label));
                out.push_str(&Self::render_single(&panel_spec, caps));
            }
            return out;
        }

        Self::render_single(spec, caps)
    }

    fn render_single(spec: &PlotSpec, caps: &RenderCaps) -> String {
        let is_hist = spec
            .layers
            .iter()
            .any(|l| matches!(l.kind, GeomKind::Histogram { .. }));
        let is_box = spec
            .layers
            .iter()
            .any(|l| matches!(l.kind, GeomKind::Boxplot { .. }));
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
        let (tl, bl, hz, vt) = if caps.unicode_enabled {
            ('╭', '╰', '─', '│')
        } else {
            ('+', '+', '-', '|')
        };

        let card_width = 72.min(caps.width);
        let plot_w = card_width.saturating_sub(18).max(20);
        let plot_h = spec.height.max(8);

        let series_list = spec.all_series();
        let has_local_data = spec.layers.iter().any(|l| {
            l.data
                .as_ref()
                .map(|d| !d.x_values.is_empty() || !d.series.is_empty())
                .unwrap_or(false)
        });
        if (series_list.is_empty()
            || series_list
                .iter()
                .all(|s| s.x_values.is_empty() && s.y_values.is_empty()))
            && !has_local_data
        {
            return format!(
                "{tl}{}\n{vt} (Empty cartesian data)\n{bl}{}\n",
                hz.to_string().repeat(24),
                hz.to_string().repeat(24)
            );
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for s in &series_list {
            for &x in &s.x_values {
                if x < min_x {
                    min_x = x;
                }
                if x > max_x {
                    max_x = x;
                }
            }
            for &y in &s.y_values {
                if y < min_y {
                    min_y = y;
                }
                if y > max_y {
                    max_y = y;
                }
            }
        }

        for layer in &spec.layers {
            if let Some(ref d) = layer.data {
                for &x in &d.x_values {
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                }
                for &y in &d.y_values {
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
                for s in &d.series {
                    for &x in &s.x_values {
                        if x < min_x {
                            min_x = x;
                        }
                        if x > max_x {
                            max_x = x;
                        }
                    }
                    for &y in &s.y_values {
                        if y < min_y {
                            min_y = y;
                        }
                        if y > max_y {
                            max_y = y;
                        }
                    }
                }
            }
        }

        if let Some((lx, hx)) = spec.x_limits {
            min_x = lx;
            max_x = hx;
        }
        if let Some((ly, hy)) = spec.y_limits {
            min_y = ly;
            max_y = hy;
        }

        let span_x = if (max_x - min_x).abs() < 1e-9 {
            1.0
        } else {
            max_x - min_x
        };
        let span_y = if (max_y - min_y).abs() < 1e-9 {
            1.0
        } else {
            max_y - min_y
        };

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
        for (i, s) in series_list.iter().enumerate() {
            let color_palette = get_okabe_ito_color(i);
            let n_pts = s.x_values.len().min(s.y_values.len());

            for pt_idx in 0..n_pts {
                let px = ((s.x_values[pt_idx] - min_x) / span_x).clamp(0.0, 1.0);
                let py = ((s.y_values[pt_idx] - min_y) / span_y).clamp(0.0, 1.0);

                let c = (px * (plot_w - 1) as f64).round() as usize;
                let r = ((1.0 - py) * (plot_h - 1) as f64).round() as usize;

                if r < plot_h && c < plot_w {
                    let point_glyph = if caps.unicode_enabled {
                        if pt_idx < s.shape_values.len() {
                            spec.map_shape(&s.shape_values[pt_idx]).to_unicode_char()
                        } else if let Some(ref gn) = s.group_name {
                            spec.map_shape(gn).to_unicode_char()
                        } else if let Some(sh_val) = spec.shape_data.get(pt_idx) {
                            spec.map_shape(sh_val).to_unicode_char()
                        } else {
                            '●'
                        }
                    } else {
                        '*'
                    };
                    grid_chars[r][c] = point_glyph;
                    grid_colors[r][c] = Some(color_palette.ansi_code);
                }
            }
        }

        // Draw layer-local data if any
        let mut local_palette_offset = series_list.len();
        for layer in &spec.layers {
            if let Some(ref d) = layer.data {
                let local_series = d.all_series();
                for (s_idx, s) in local_series.iter().enumerate() {
                    let color_palette = get_okabe_ito_color(local_palette_offset + s_idx);
                    let n_pts = s.x_values.len().min(s.y_values.len());

                    match &layer.kind {
                        GeomKind::Point { .. } => {
                            for pt_idx in 0..n_pts {
                                let px = ((s.x_values[pt_idx] - min_x) / span_x).clamp(0.0, 1.0);
                                let py = ((s.y_values[pt_idx] - min_y) / span_y).clamp(0.0, 1.0);
                                let c = (px * (plot_w - 1) as f64).round() as usize;
                                let r = ((1.0 - py) * (plot_h - 1) as f64).round() as usize;
                                if r < plot_h && c < plot_w {
                                    grid_chars[r][c] =
                                        if caps.unicode_enabled { '●' } else { '*' };
                                    grid_colors[r][c] = Some(color_palette.ansi_code);
                                }
                            }
                        }
                        GeomKind::Line { .. } => {
                            for pt_idx in 0..n_pts {
                                let px = ((s.x_values[pt_idx] - min_x) / span_x).clamp(0.0, 1.0);
                                let py = ((s.y_values[pt_idx] - min_y) / span_y).clamp(0.0, 1.0);
                                let c = (px * (plot_w - 1) as f64).round() as usize;
                                let r = ((1.0 - py) * (plot_h - 1) as f64).round() as usize;
                                if r < plot_h && c < plot_w {
                                    grid_chars[r][c] =
                                        if caps.unicode_enabled { '─' } else { '-' };
                                    grid_colors[r][c] = Some(color_palette.ansi_code);
                                }
                            }
                        }
                        GeomKind::Smooth { fit, .. } => {
                            let fit_val = fit.or_else(|| {
                                crate::spec::LinearFit::compute(&s.x_values, &s.y_values)
                            });
                            if let Some(f) = fit_val {
                                for c in 0..plot_w {
                                    let cur_x = min_x + (c as f64 / (plot_w - 1) as f64) * span_x;
                                    let cur_y = f.intercept + f.slope * cur_x;
                                    let norm_y = ((cur_y - min_y) / span_y).clamp(0.0, 1.0);
                                    let row =
                                        ((1.0 - norm_y) * (plot_h - 1) as f64).round() as usize;
                                    if row < plot_h {
                                        grid_chars[row][c] =
                                            if caps.unicode_enabled { '·' } else { '.' };
                                        grid_colors[row][c] = Some(color_palette.ansi_code);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                local_palette_offset += local_series.len().max(1);
            }
        }

        // Build output card: Left-Rail Modern Layout
        let title = spec
            .labels
            .title
            .clone()
            .unwrap_or_else(|| "Cartesian Plot".into());
        let badge = if caps.unicode_enabled {
            "/ᐠ˵- ⩊ -˵マ ✧ READY"
        } else {
            "[READY]"
        };

        let mut out = String::new();
        let header_inner = format!(" {title} ");
        let pad_top = card_width
            .saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6)
            .max(4);
        out.push_str(&caps.dim(&format!(
            "{tl}{hz} {title} {}{hz} {badge}\n",
            hz.to_string().repeat(pad_top)
        )));

        if let Some(ref yl) = spec.labels.y_label {
            let y_header = format!("{yl} ▲");
            out.push_str(&format!("{vt} {}\n", caps.bold(&y_header)));
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
            out.push_str(&format!("{vt} {}\n", line_content));
        }

        // X axis line
        let axis_corner = if caps.unicode_enabled { "└" } else { "+" };
        let axis_line = hz.to_string().repeat(plot_w);
        let x_axis_str = format!("        {} {}", axis_corner, axis_line);
        out.push_str(&format!("{vt} {}\n", caps.dim(&x_axis_str)));

        // X tick labels
        let x_labels_str = format!(
            "         {:<8.1}{:>width$.1}",
            min_x,
            max_x,
            width = plot_w.saturating_sub(8)
        );
        out.push_str(&format!("{vt} {}\n", caps.dim(&x_labels_str)));

        if let Some(ref xl) = spec.labels.x_label {
            let x_center = format!("{xl} ▶");
            out.push_str(&format!("{vt} {}\n", caps.bold(&x_center)));
        }

        // Legend if multiple series
        let grouped_series: Vec<_> = series_list
            .iter()
            .filter(|s| s.group_name.is_some())
            .collect();
        if !grouped_series.is_empty() {
            let mut legend_str = String::from("Legend: ");
            for (i, s) in series_list.iter().enumerate() {
                if let Some(ref name) = s.group_name {
                    let color = get_okabe_ito_color(i);
                    let sym = if caps.unicode_enabled {
                        spec.map_shape(name).to_unicode_char()
                    } else {
                        '*'
                    };
                    if caps.color_enabled {
                        legend_str.push_str(&format!("{}{sym}\x1b[0m {}  ", color.ansi_code, name));
                    } else {
                        legend_str.push_str(&format!("[{sym}] {}  ", name));
                    }
                }
            }
            out.push_str(&format!("{vt} {}\n", caps.dim(&legend_str)));
        }

        let bot_len = (plot_w + 14).min(card_width).max(30);
        out.push_str(&caps.dim(&format!("{bl}{}\n", hz.to_string().repeat(bot_len))));
        out
    }

    fn render_histogram(spec: &PlotSpec, bins_count: usize, caps: &RenderCaps) -> String {
        let (tl, bl, hz, vt) = if caps.unicode_enabled {
            ('╭', '╰', '─', '│')
        } else {
            ('+', '+', '-', '|')
        };

        let card_width = 72.min(caps.width);
        let HistogramBins {
            min_x,
            max_x,
            bin_width,
            counts,
            max_count,
        } = match spec.histogram_bins(bins_count.clamp(4, 24)) {
            Some(b) => b,
            None => {
                return format!(
                    "{tl}{}\n{vt} (Empty histogram data)\n{bl}{}\n",
                    hz.to_string().repeat(24),
                    hz.to_string().repeat(24)
                );
            }
        };

        let title = spec
            .labels
            .title
            .clone()
            .unwrap_or_else(|| "Histogram".into());
        let badge = if caps.unicode_enabled {
            "/ᐠ˵- ⩊ -˵マ ✧ READY"
        } else {
            "[READY]"
        };

        let mut out = String::new();
        let header_inner = format!(" {title} ");
        let pad_top = card_width
            .saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6)
            .max(4);
        out.push_str(&caps.dim(&format!(
            "{tl}{hz} {title} {}{hz} {badge}\n",
            hz.to_string().repeat(pad_top)
        )));

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
                    if caps.unicode_enabled {
                        bar_levels[rem.clamp(0, 7)]
                    } else {
                        '#'
                    }
                } else {
                    ' '
                };
                row.push(char_in_row);
                row.push(' ');
            }
            let styled_row = caps.cyan(&row);
            out.push_str(&format!("{vt}   {}\n", styled_row));
        }

        // Domain summary
        let summary = format!(
            "[{:.1} .. {:.1}]  Bins: {}  BinWidth: {:.2}",
            min_x,
            max_x,
            counts.len(),
            bin_width
        );
        out.push_str(&format!("{vt} {}\n", caps.dim(&summary)));

        let bot_len = (counts.len() * 2 + 8).min(card_width).max(30);
        out.push_str(&caps.dim(&format!("{bl}{}\n", hz.to_string().repeat(bot_len))));
        out
    }

    fn render_boxplot(spec: &PlotSpec, caps: &RenderCaps) -> String {
        let (tl, bl, hz, vt) = if caps.unicode_enabled {
            ('╭', '╰', '─', '│')
        } else {
            ('+', '+', '-', '|')
        };

        let card_width = 72.min(caps.width);
        let multi_stats = spec.boxplot_multi_stats();
        if !multi_stats.is_empty() {
            let title = spec
                .labels
                .title
                .clone()
                .unwrap_or_else(|| "Comparative Boxplot".into());
            let badge = if caps.unicode_enabled {
                "/ᐠ˵- ⩊ -˵マ ✧ READY"
            } else {
                "[READY]"
            };

            let mut out = String::new();
            let header_inner = format!(" {title} ");
            let pad_top = card_width
                .saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6)
                .max(4);
            out.push_str(&caps.dim(&format!(
                "{tl}{hz} {title} {}{hz} {badge}\n",
                hz.to_string().repeat(pad_top)
            )));

            let mut global_min = f64::INFINITY;
            let mut global_max = f64::NEG_INFINITY;
            for (_, st) in &multi_stats {
                if st.min < global_min {
                    global_min = st.min;
                }
                if st.max > global_max {
                    global_max = st.max;
                }
            }
            let span = (global_max - global_min).abs().max(1e-9);
            let cat_label_w = multi_stats
                .iter()
                .map(|(c, _)| visual_width(c))
                .max()
                .unwrap_or(8)
                .min(16);
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
                for i in lf..q1 {
                    line[i] = if caps.unicode_enabled { '─' } else { '-' };
                }
                for i in q1..=q3 {
                    line[i] = if caps.unicode_enabled { '█' } else { '=' };
                }
                for i in (q3 + 1)..=uf {
                    line[i] = if caps.unicode_enabled { '─' } else { '-' };
                }

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

                let line_str = format!(
                    " {:<width$} {} Med: {:>6.1}",
                    cat,
                    colored_diag,
                    st.median,
                    width = cat_label_w
                );
                out.push_str(&format!("{vt}{}\n", line_str));
            }

            let scale_summary = format!(
                " Domain: [{:.1} .. {:.1}]  Categories: {}",
                global_min,
                global_max,
                multi_stats.len()
            );
            out.push_str(&format!("{vt}{}\n", caps.dim(&scale_summary)));

            let bot_len = (plot_w + cat_label_w + 20).min(card_width).max(30);
            out.push_str(&caps.dim(&format!("{bl}{}\n", hz.to_string().repeat(bot_len))));
            return out;
        }

        let stats = match spec.boxplot_stats() {
            Some(s) => s,
            None => {
                return format!(
                    "{tl}{}\n{vt} (No boxplot statistics)\n{bl}{}\n",
                    hz.to_string().repeat(24),
                    hz.to_string().repeat(24)
                );
            }
        };

        let title = spec
            .labels
            .title
            .clone()
            .unwrap_or_else(|| "Boxplot".into());
        let badge = if caps.unicode_enabled {
            "/ᐠ˵- ⩊ -˵マ ✧ READY"
        } else {
            "[READY]"
        };

        let mut out = String::new();
        let header_inner = format!(" {title} ");
        let pad_top = card_width
            .saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6)
            .max(4);
        out.push_str(&caps.dim(&format!(
            "{tl}{hz} {title} {}{hz} {badge}\n",
            hz.to_string().repeat(pad_top)
        )));

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
        for i in lf..q1 {
            line[i] = if caps.unicode_enabled { '─' } else { '-' };
        }
        for i in q1..=q3 {
            line[i] = if caps.unicode_enabled { '█' } else { '=' };
        }
        for i in (q3 + 1)..=uf {
            line[i] = if caps.unicode_enabled { '─' } else { '-' };
        }

        line[lf] = if caps.unicode_enabled { '├' } else { '|' };
        line[uf] = if caps.unicode_enabled { '┤' } else { '|' };
        line[med] = if caps.unicode_enabled { '┃' } else { '|' };

        let diagram: String = line.into_iter().collect();
        out.push_str(&format!("{vt}   {}\n", caps.green(&diagram)));

        let stats_info = format!(
            "Min: {:.1}  Q1: {:.1}  Med: {:.1}  Q3: {:.1}  Max: {:.1}",
            stats.min, stats.q1, stats.median, stats.q3, stats.max
        );
        out.push_str(&format!("{vt} {}\n", caps.dim(&stats_info)));

        let bot_len = (plot_w + 14).min(card_width).max(30);
        out.push_str(&caps.dim(&format!("{bl}{}\n", hz.to_string().repeat(bot_len))));
        out
    }

    fn render_bar(spec: &PlotSpec, caps: &RenderCaps) -> String {
        let (tl, bl, hz, vt) = if caps.unicode_enabled {
            ('╭', '╰', '─', '│')
        } else {
            ('+', '+', '-', '|')
        };

        let card_width = 72.min(caps.width);
        let counts_map = match spec.bar_counts() {
            Some(c) => c,
            None => {
                return format!(
                    "{tl}{}\n{vt} (Empty bar data)\n{bl}{}\n",
                    hz.to_string().repeat(24),
                    hz.to_string().repeat(24)
                );
            }
        };

        let title = spec
            .labels
            .title
            .clone()
            .unwrap_or_else(|| "Bar Chart".into());
        let badge = if caps.unicode_enabled {
            "/ᐠ˵- ⩊ -˵マ ✧ READY"
        } else {
            "[READY]"
        };

        let mut out = String::new();
        let header_inner = format!(" {title} ");
        let pad_top = card_width
            .saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6)
            .max(4);
        out.push_str(&caps.dim(&format!(
            "{tl}{hz} {title} {}{hz} {badge}\n",
            hz.to_string().repeat(pad_top)
        )));

        let max_count = counts_map.values().copied().max().unwrap_or(1);
        let bar_max_w = card_width.saturating_sub(28).max(10);

        for (cat, &cnt) in &counts_map {
            let frac = cnt as f64 / max_count as f64;
            let bar_len = (frac * bar_max_w as f64).round() as usize;
            let bar_str = if caps.unicode_enabled {
                "█".repeat(bar_len)
            } else {
                "#".repeat(bar_len)
            };
            let line = format!("{:<12} {} ({})", cat, caps.cyan(&bar_str), cnt);
            out.push_str(&format!("{vt} {}\n", line));
        }

        let bot_len = (bar_max_w + 24).min(card_width).max(30);
        out.push_str(&caps.dim(&format!("{bl}{}\n", hz.to_string().repeat(bot_len))));
        out
    }
}
