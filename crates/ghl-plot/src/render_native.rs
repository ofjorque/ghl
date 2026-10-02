//! Native publication-grade raster and vector rendering backend using Plotters.
//!
//! Features:
//! - Multi-layer simultaneous rendering (e.g. Points + Line + OLS Smooth trendline)
//! - Multi-series color grouping with the Okabe-Ito colorblind-safe palette
//! - Automatic legend generation for grouped aesthetics
//! - High-DPI raster PNG export and crisp vector SVG export
//! - Multiple themes (DarkEngine, Minimal, Classic)

use std::path::Path;
use plotters::prelude::*;
use crate::palette::get_okabe_ito_color;
use crate::spec::{GeomKind, HistogramBins, PlotSpec, PlotTheme};

pub struct NativeRenderer;

impl NativeRenderer {
    /// Save plot to PNG or SVG file with default 800x600 dimensions.
    pub fn save_file(spec: &PlotSpec, path: &str) -> Result<(), String> {
        Self::save_file_with_size(spec, path, 800, 600)
    }

    /// Save plot with custom pixel dimensions.
    pub fn save_file_with_size(spec: &PlotSpec, path: &str, width: u32, height: u32) -> Result<(), String> {
        let path_ref = Path::new(path);
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create directories: {e}"))?;
            }
        }

        let ext = path_ref
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "png".to_string());

        if ext == "svg" {
            let root = SVGBackend::new(path, (width, height)).into_drawing_area();
            Self::draw_chart(spec, &root)?;
            root.present().map_err(|e| format!("Failed to write SVG: {e}"))?;
        } else {
            let root = BitMapBackend::new(path, (width, height)).into_drawing_area();
            Self::draw_chart(spec, &root)?;
            root.present().map_err(|e| format!("Failed to write bitmap: {e}"))?;
        }

        Ok(())
    }

    /// Render plot directly to an SVG XML string.
    pub fn to_svg(spec: &PlotSpec, width: u32, height: u32) -> Result<String, String> {
        let mut buf = String::new();
        {
            let root = SVGBackend::with_string(&mut buf, (width, height)).into_drawing_area();
            Self::draw_chart(spec, &root)?;
            root.present().map_err(|e| format!("Failed to render SVG: {e}"))?;
        }
        Ok(buf)
    }

    pub fn draw_chart<DB: DrawingBackend>(spec: &PlotSpec, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let bg_color = match spec.theme {
            PlotTheme::Dark => RGBColor(24, 24, 27),
            _ => WHITE,
        };
        root.fill(&bg_color).map_err(|e| format!("{e}"))?;

        if spec.facet.is_some() {
            return Self::draw_facets(spec, root);
        }

        Self::draw_single_panel(spec, root)
    }

    fn draw_single_panel<DB: DrawingBackend>(spec: &PlotSpec, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let is_hist = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Histogram { .. }));
        let is_box = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Boxplot { .. }));
        let is_bar = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Bar));

        if is_hist {
            Self::draw_histogram(spec, root)?;
        } else if is_box {
            Self::draw_boxplot(spec, root)?;
        } else if is_bar {
            Self::draw_bar(spec, root)?;
        } else {
            Self::draw_cartesian_layers(spec, root)?;
        }

        Ok(())
    }

    fn draw_facets<DB: DrawingBackend>(spec: &PlotSpec, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let (panels, rows, cols) = spec.partition_facets();
        if panels.is_empty() {
            return Ok(());
        }

        let is_dark = spec.theme == PlotTheme::Dark;
        let strip_bg = if is_dark {
            RGBColor(38, 38, 44)
        } else {
            RGBColor(235, 236, 240)
        };
        let strip_border = if is_dark {
            RGBColor(60, 60, 70)
        } else {
            RGBColor(200, 202, 210)
        };
        let strip_text_color = if is_dark {
            RGBColor(240, 240, 245)
        } else {
            RGBColor(30, 30, 35)
        };

        // Top super-title if present
        let grid_area = if let Some(ref title) = spec.labels.title {
            let (top, bottom) = root.split_vertically(38);
            let title_style = ("sans-serif", 20).into_font().color(&strip_text_color);
            top.draw(&Text::new(title.clone(), (15, 10), title_style))
                .map_err(|e| format!("{e}"))?;
            bottom
        } else {
            root.clone()
        };

        let sub_areas = grid_area.split_evenly((rows, cols));

        for panel in panels {
            let area_idx = panel.row_idx * cols + panel.col_idx;
            if area_idx >= sub_areas.len() {
                continue;
            }
            let cell_area = &sub_areas[area_idx];

            let (strip_area, chart_area) = cell_area.split_vertically(26);

            strip_area.fill(&strip_bg).map_err(|e| format!("{e}"))?;
            let w = strip_area.dim_in_pixel().0 as i32;
            let h = strip_area.dim_in_pixel().1 as i32;
            strip_area.draw(&plotters::element::Rectangle::new(
                [(0, 0), (w - 1, h - 1)],
                strip_border.stroke_width(1),
            )).map_err(|e| format!("{e}"))?;

            let strip_text = &panel.label;
            let strip_font = ("sans-serif", 13).into_font().color(&strip_text_color);
            let char_width = 7;
            let text_pixel_len = strip_text.len() as i32 * char_width;
            let draw_x = ((w - text_pixel_len) / 2).max(6);
            strip_area.draw(&Text::new(strip_text.clone(), (draw_x, 6), strip_font))
                .map_err(|e| format!("{e}"))?;

            let mut panel_spec = panel.spec;
            panel_spec.labels.title = None;
            let _ = Self::draw_single_panel(&panel_spec, &chart_area);
        }

        Ok(())
    }

    /// Renders multi-layer 2D Cartesian graphics (Points, Lines, Smooth, Rug)
    /// with multi-series colors and automatic legend.
    fn draw_cartesian_layers<DB: DrawingBackend>(spec: &PlotSpec, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let series_list = spec.all_series();
        if series_list.is_empty() || series_list.iter().all(|s| s.x_values.is_empty() && s.y_values.is_empty()) {
            return Err("Cannot plot empty Cartesian data".to_string());
        }

        let (bg_color, text_color, grid_color) = match spec.theme {
            PlotTheme::Dark => (RGBColor(24, 24, 27), RGBColor(240, 240, 245), RGBColor(50, 50, 60)),
            PlotTheme::Minimal => (WHITE, RGBColor(35, 35, 40), RGBColor(230, 230, 235)),
            _ => (WHITE, BLACK, RGBColor(210, 210, 215)),
        };

        // Determine min and max domain across all series
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

        if let Some((lx, hx)) = spec.x_limits {
            min_x = lx;
            max_x = hx;
        }
        if let Some((ly, hy)) = spec.y_limits {
            min_y = ly;
            max_y = hy;
        }

        if min_x.is_infinite() || min_y.is_infinite() {
            return Err("No numeric data points to render".to_string());
        }

        let pad_x = if (max_x - min_x).abs() < 1e-6 { 1.0 } else { (max_x - min_x) * 0.08 };
        let pad_y = if (max_y - min_y).abs() < 1e-6 { 1.0 } else { (max_y - min_y) * 0.08 };

        let x_range = (min_x - pad_x)..(max_x + pad_x);
        let y_range = (min_y - pad_y)..(max_y + pad_y);

        let title_opt = spec.labels.title.as_deref();
        let x_label = spec.labels.x_label.as_deref().unwrap_or("x");
        let y_label = spec.labels.y_label.as_deref().unwrap_or("y");

        let mut chart_builder = ChartBuilder::on(root);
        if let Some(t) = title_opt {
            chart_builder.caption(t, ("sans-serif", 22).into_font().color(&text_color)).margin(18);
        } else {
            chart_builder.margin(10);
        }

        let mut chart = chart_builder
            .x_label_area_size(38)
            .y_label_area_size(48)
            .build_cartesian_2d(x_range, y_range)
            .map_err(|e| format!("Chart build error: {e}"))?;

        chart
            .configure_mesh()
            .x_desc(x_label)
            .y_desc(y_label)
            .axis_desc_style(("sans-serif", 15).into_font().color(&text_color))
            .label_style(("sans-serif", 12).into_font().color(&text_color))
            .light_line_style(ShapeStyle::from(&grid_color).stroke_width(1))
            .bold_line_style(ShapeStyle::from(&grid_color).stroke_width(1))
            .draw()
            .map_err(|e| format!("Mesh draw error: {e}"))?;

        let has_points = spec.layers.is_empty() || spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Point { .. }));
        let has_lines = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Line { .. }));
        let has_smooth = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Smooth { .. }));

        // Render each series with palette color
        for (i, s) in series_list.iter().enumerate() {
            let color_palette = get_okabe_ito_color(i);
            let series_color = color_palette.to_plotters_color();

            let points: Vec<(f64, f64)> = s.x_values.iter().copied().zip(s.y_values.iter().copied()).collect();

            // 1. Draw points layer if requested
            if has_points && !points.is_empty() {
                let name = s.group_name.clone().unwrap_or_else(|| format!("Series {}", i + 1));
                let color_for_legend = series_color;
                
                let series = chart.draw_series(points.iter().map(|&(x, y)| {
                    Circle::new((x, y), 5, ShapeStyle::from(&series_color).filled())
                })).map_err(|e| format!("Points draw error: {e}"))?;

                if s.group_name.is_some() {
                    series.label(name).legend(move |(x, y)| Circle::new((x + 10, y), 4, ShapeStyle::from(&color_for_legend).filled()));
                }
            }

            // 2. Draw line layer if requested
            if has_lines && points.len() >= 2 {
                let mut sorted_points = points.clone();
                sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

                let name = s.group_name.clone().unwrap_or_else(|| format!("Series {}", i + 1));
                let color_for_legend = series_color;

                let series = chart.draw_series(
                    LineSeries::new(sorted_points, ShapeStyle::from(&series_color).stroke_width(2))
                ).map_err(|e| format!("Line draw error: {e}"))?;

                if s.group_name.is_some() && !has_points {
                    series.label(name).legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], ShapeStyle::from(&color_for_legend).stroke_width(2)));
                }
            }
        }

        // 3. Draw smooth regression line if requested
        if has_smooth {
            if let Some(fit) = spec.smooth_fit() {
                let smooth_color = RGBColor(220, 50, 47); // Vivid Coral / Solarized Red
                let line_pts = vec![
                    (min_x, fit.intercept + fit.slope * min_x),
                    (max_x, fit.intercept + fit.slope * max_x),
                ];
                chart
                    .draw_series(LineSeries::new(line_pts, ShapeStyle::from(&smooth_color).stroke_width(3)))
                    .map_err(|e| format!("Smooth line draw error: {e}"))?
                    .label(format!("Fit: y = {:.2} + {:.2}x", fit.intercept, fit.slope))
                    .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], ShapeStyle::from(&smooth_color).stroke_width(3)));
            }
        }

        // Render legend if grouped aesthetics were mapped
        let has_groups = series_list.iter().any(|s| s.group_name.is_some()) || has_smooth;
        if has_groups {
            chart
                .configure_series_labels()
                .background_style(ShapeStyle::from(&bg_color).filled())
                .border_style(ShapeStyle::from(&grid_color).stroke_width(1))
                .label_font(("sans-serif", 12).into_font().color(&text_color))
                .position(SeriesLabelPosition::UpperRight)
                .draw()
                .map_err(|e| format!("Legend draw error: {e}"))?;
        }

        Ok(())
    }

    fn draw_histogram<DB: DrawingBackend>(spec: &PlotSpec, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let bins_count = spec.requested_bins().clamp(4, 30);
        let HistogramBins { min_x, max_x, bin_width, counts, max_count } = spec
            .histogram_bins(bins_count)
            .ok_or_else(|| "Cannot plot empty histogram data".to_string())?;

        let (text_color, grid_color) = match spec.theme {
            PlotTheme::Dark => (RGBColor(240, 240, 245), RGBColor(50, 50, 60)),
            _ => (BLACK, RGBColor(210, 210, 215)),
        };

        let title = spec.labels.title.as_deref().unwrap_or("Histogram");
        let x_label = spec.labels.x_label.as_deref().unwrap_or("x");
        let y_label = spec.labels.y_label.as_deref().unwrap_or("Count");

        let mut chart = ChartBuilder::on(root)
            .caption(title, ("sans-serif", 22).into_font().color(&text_color))
            .margin(20)
            .x_label_area_size(42)
            .y_label_area_size(52)
            .build_cartesian_2d(min_x..(max_x + bin_width * 0.05), 0u32..(max_count + 1))
            .map_err(|e| format!("Chart build error: {e}"))?;

        chart
            .configure_mesh()
            .x_desc(x_label)
            .y_desc(y_label)
            .axis_desc_style(("sans-serif", 15).into_font().color(&text_color))
            .label_style(("sans-serif", 12).into_font().color(&text_color))
            .light_line_style(ShapeStyle::from(&grid_color).stroke_width(1))
            .draw()
            .map_err(|e| format!("Mesh draw error: {e}"))?;

        let bar_fill = RGBColor(79, 110, 242);
        let bar_stroke = RGBColor(30, 60, 180);

        for (i, &count) in counts.iter().enumerate() {
            let x0 = min_x + i as f64 * bin_width;
            let x1 = x0 + bin_width;
            chart
                .draw_series(std::iter::once(Rectangle::new(
                    [(x0, 0), (x1, count)],
                    ShapeStyle::from(&bar_fill).filled(),
                )))
                .map_err(|e| format!("Bar draw error: {e}"))?;

            chart
                .draw_series(std::iter::once(PathElement::new(
                    vec![(x0, 0), (x0, count), (x1, count), (x1, 0)],
                    ShapeStyle::from(&bar_stroke).stroke_width(1),
                )))
                .map_err(|e| format!("Bar border draw error: {e}"))?;
        }

        Ok(())
    }

    fn draw_boxplot<DB: DrawingBackend>(spec: &PlotSpec, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let multi_stats = spec.boxplot_multi_stats();
        if !multi_stats.is_empty() {
            let (bg_color, text_color, grid_color) = match spec.theme {
                PlotTheme::Dark => (RGBColor(24, 24, 27), RGBColor(240, 240, 245), RGBColor(50, 50, 60)),
                _ => (WHITE, BLACK, RGBColor(210, 210, 215)),
            };

            let title = spec.labels.title.as_deref().unwrap_or("Comparative Boxplot");
            let x_label = spec.labels.x_label.as_deref().unwrap_or("Category");
            let y_label = spec.labels.y_label.as_deref().unwrap_or("Value");

            let mut global_min = f64::INFINITY;
            let mut global_max = f64::NEG_INFINITY;
            for (_, st) in &multi_stats {
                if st.min < global_min { global_min = st.min; }
                if st.max > global_max { global_max = st.max; }
            }
            let y_span = (global_max - global_min).abs().max(1.0);
            let y_pad = y_span * 0.1;
            let y_min = global_min - y_pad;
            let y_max = global_max + y_pad;

            let k = multi_stats.len();
            let mut chart = ChartBuilder::on(root)
                .caption(title, ("sans-serif", 22).into_font().color(&text_color))
                .margin(20)
                .x_label_area_size(42)
                .y_label_area_size(52)
                .build_cartesian_2d(0.0..(k as f64 + 1.0), y_min..y_max)
                .map_err(|e| format!("Chart build error: {e}"))?;

            chart
                .configure_mesh()
                .x_desc(x_label)
                .y_desc(y_label)
                .axis_desc_style(("sans-serif", 15).into_font().color(&text_color))
                .label_style(("sans-serif", 12).into_font().color(&text_color))
                .light_line_style(ShapeStyle::from(&grid_color).stroke_width(1))
                .draw()
                .map_err(|e| format!("Mesh draw error: {e}"))?;

            for (idx, (cat_name, stats)) in multi_stats.iter().enumerate() {
                let center_x = (idx + 1) as f64;
                let half_w = 0.28;
                let whisker_w = 0.14;
                let color_info = get_okabe_ito_color(idx);
                let box_color = color_info.to_plotters_color();
                let stroke_color = match spec.theme {
                    PlotTheme::Dark => RGBColor(220, 220, 220),
                    _ => RGBColor(30, 30, 30),
                };

                // Whiskers
                chart.draw_series(std::iter::once(PathElement::new(
                    vec![(center_x, stats.q1), (center_x, stats.lower_fence)],
                    ShapeStyle::from(&stroke_color).stroke_width(2),
                ))).map_err(|e| format!("{e}"))?;
                chart.draw_series(std::iter::once(PathElement::new(
                    vec![(center_x - whisker_w, stats.lower_fence), (center_x + whisker_w, stats.lower_fence)],
                    ShapeStyle::from(&stroke_color).stroke_width(2),
                ))).map_err(|e| format!("{e}"))?;

                chart.draw_series(std::iter::once(PathElement::new(
                    vec![(center_x, stats.q3), (center_x, stats.upper_fence)],
                    ShapeStyle::from(&stroke_color).stroke_width(2),
                ))).map_err(|e| format!("{e}"))?;
                chart.draw_series(std::iter::once(PathElement::new(
                    vec![(center_x - whisker_w, stats.upper_fence), (center_x + whisker_w, stats.upper_fence)],
                    ShapeStyle::from(&stroke_color).stroke_width(2),
                ))).map_err(|e| format!("{e}"))?;

                // Box
                let series = chart.draw_series(std::iter::once(Rectangle::new(
                    [(center_x - half_w, stats.q1), (center_x + half_w, stats.q3)],
                    ShapeStyle::from(&box_color).filled(),
                ))).map_err(|e| format!("{e}"))?;

                let leg_color = box_color;
                let cat_label = cat_name.clone();
                series.label(cat_label).legend(move |(x, y)| Rectangle::new([(x, y - 5), (x + 12, y + 5)], ShapeStyle::from(&leg_color).filled()));

                chart.draw_series(std::iter::once(PathElement::new(
                    vec![
                        (center_x - half_w, stats.q1),
                        (center_x + half_w, stats.q1),
                        (center_x + half_w, stats.q3),
                        (center_x - half_w, stats.q3),
                        (center_x - half_w, stats.q1),
                    ],
                    ShapeStyle::from(&stroke_color).stroke_width(2),
                ))).map_err(|e| format!("{e}"))?;

                // Median line
                chart.draw_series(std::iter::once(PathElement::new(
                    vec![(center_x - half_w, stats.median), (center_x + half_w, stats.median)],
                    ShapeStyle::from(&RGBColor(213, 94, 0)).stroke_width(3),
                ))).map_err(|e| format!("{e}"))?;

                // Outliers
                for &val in &stats.outliers {
                    chart.draw_series(std::iter::once(Circle::new(
                        (center_x, val),
                        4,
                        ShapeStyle::from(&RGBColor(213, 94, 0)).filled(),
                    ))).map_err(|e| format!("{e}"))?;
                }
            }

            // Legend
            chart
                .configure_series_labels()
                .background_style(ShapeStyle::from(&bg_color).filled())
                .border_style(ShapeStyle::from(&grid_color).stroke_width(1))
                .label_font(("sans-serif", 12).into_font().color(&text_color))
                .position(SeriesLabelPosition::UpperRight)
                .draw()
                .map_err(|e| format!("Legend draw error: {e}"))?;

            return Ok(());
        }

        let stats = spec.boxplot_stats().ok_or_else(|| "Cannot plot boxplot without summary statistics".to_string())?;

        let (text_color, grid_color) = match spec.theme {
            PlotTheme::Dark => (RGBColor(240, 240, 245), RGBColor(50, 50, 60)),
            _ => (BLACK, RGBColor(210, 210, 215)),
        };

        let title = spec.labels.title.as_deref().unwrap_or("Boxplot");
        let y_label = spec.labels.y_label.as_deref().unwrap_or("Value");

        let y_span = (stats.max - stats.min).abs().max(1.0);
        let y_pad = y_span * 0.1;
        let y_min = stats.min - y_pad;
        let y_max = stats.max + y_pad;

        let mut chart = ChartBuilder::on(root)
            .caption(title, ("sans-serif", 22).into_font().color(&text_color))
            .margin(20)
            .x_label_area_size(42)
            .y_label_area_size(52)
            .build_cartesian_2d(0.0..2.0, y_min..y_max)
            .map_err(|e| format!("Chart build error: {e}"))?;

        chart
            .configure_mesh()
            .y_desc(y_label)
            .axis_desc_style(("sans-serif", 15).into_font().color(&text_color))
            .label_style(("sans-serif", 12).into_font().color(&text_color))
            .light_line_style(ShapeStyle::from(&grid_color).stroke_width(1))
            .draw()
            .map_err(|e| format!("Mesh draw error: {e}"))?;

        let center_x = 1.0;
        let half_w = 0.25;
        let whisker_w = 0.12;

        let box_color = RGBColor(86, 180, 233);
        let stroke_color = RGBColor(20, 40, 80);

        // Lower whisker
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x, stats.q1), (center_x, stats.lower_fence)],
            ShapeStyle::from(&stroke_color).stroke_width(2),
        ))).map_err(|e| format!("{e}"))?;
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x - whisker_w, stats.lower_fence), (center_x + whisker_w, stats.lower_fence)],
            ShapeStyle::from(&stroke_color).stroke_width(2),
        ))).map_err(|e| format!("{e}"))?;

        // Upper whisker
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x, stats.q3), (center_x, stats.upper_fence)],
            ShapeStyle::from(&stroke_color).stroke_width(2),
        ))).map_err(|e| format!("{e}"))?;
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x - whisker_w, stats.upper_fence), (center_x + whisker_w, stats.upper_fence)],
            ShapeStyle::from(&stroke_color).stroke_width(2),
        ))).map_err(|e| format!("{e}"))?;

        // IQR box
        chart.draw_series(std::iter::once(Rectangle::new(
            [(center_x - half_w, stats.q1), (center_x + half_w, stats.q3)],
            ShapeStyle::from(&box_color).filled(),
        ))).map_err(|e| format!("{e}"))?;
        chart.draw_series(std::iter::once(PathElement::new(
            vec![
                (center_x - half_w, stats.q1),
                (center_x + half_w, stats.q1),
                (center_x + half_w, stats.q3),
                (center_x - half_w, stats.q3),
                (center_x - half_w, stats.q1),
            ],
            ShapeStyle::from(&stroke_color).stroke_width(2),
        ))).map_err(|e| format!("{e}"))?;

        // Median line
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x - half_w, stats.median), (center_x + half_w, stats.median)],
            ShapeStyle::from(&RGBColor(213, 94, 0)).stroke_width(3),
        ))).map_err(|e| format!("{e}"))?;

        // Outliers
        for &val in &stats.outliers {
            chart.draw_series(std::iter::once(Circle::new(
                (center_x, val),
                4,
                ShapeStyle::from(&RGBColor(213, 94, 0)).filled(),
            ))).map_err(|e| format!("{e}"))?;
        }

        Ok(())
    }

    fn draw_bar<DB: DrawingBackend>(spec: &PlotSpec, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let counts_map = spec.bar_counts().ok_or_else(|| "Cannot plot bar chart without categories or data".to_string())?;

        let (text_color, grid_color) = match spec.theme {
            PlotTheme::Dark => (RGBColor(240, 240, 245), RGBColor(50, 50, 60)),
            _ => (BLACK, RGBColor(210, 210, 215)),
        };

        let title = spec.labels.title.as_deref().unwrap_or("Bar Chart");
        let x_label = spec.labels.x_label.as_deref().unwrap_or("Category");
        let y_label = spec.labels.y_label.as_deref().unwrap_or("Count");

        let n_cats = counts_map.len();
        let max_count = counts_map.values().copied().max().unwrap_or(1);

        let mut chart = ChartBuilder::on(root)
            .caption(title, ("sans-serif", 22).into_font().color(&text_color))
            .margin(20)
            .x_label_area_size(42)
            .y_label_area_size(52)
            .build_cartesian_2d(0.0..(n_cats as f64 + 1.0), 0u32..(max_count as u32 + 1))
            .map_err(|e| format!("Chart build error: {e}"))?;

        chart
            .configure_mesh()
            .x_desc(x_label)
            .y_desc(y_label)
            .axis_desc_style(("sans-serif", 15).into_font().color(&text_color))
            .label_style(("sans-serif", 12).into_font().color(&text_color))
            .light_line_style(ShapeStyle::from(&grid_color).stroke_width(1))
            .draw()
            .map_err(|e| format!("Mesh draw error: {e}"))?;

        let bar_half_w = 0.35;
        for (i, (_cat, &count)) in counts_map.iter().enumerate() {
            let color = get_okabe_ito_color(i).to_plotters_color();
            let cx = (i + 1) as f64;
            let x0 = cx - bar_half_w;
            let x1 = cx + bar_half_w;
            chart
                .draw_series(std::iter::once(Rectangle::new(
                    [(x0, 0), (x1, count as u32)],
                    ShapeStyle::from(&color).filled(),
                )))
                .map_err(|e| format!("Bar draw error: {e}"))?;
        }

        Ok(())
    }
}
