//! Grammar of Graphics & Terminal Visualization Engine for GHL Cockpit Deck.
//!
//! Implements RFC 09:
//! - Scatter plots (`geom_point`)
//! - Regression trend lines (`geom_smooth`)
//! - Histograms (`geom_histogram`) with 8-level vertical blocks
//! - Box-and-whisker plots (`geom_boxplot`) with Tukey fences and outlier markers
//! - Bar charts (`geom_bar`) for categorical frequencies
//! - Full RenderCaps support (Unicode vs ASCII, ANSI colors vs NO_COLOR)

use std::path::Path;
use plotters::prelude::*;
use crate::caps::RenderCaps;
use crate::panel::visual_width;

#[derive(Debug, Clone, PartialEq)]
pub struct AestheticMap {
    pub x: String,
    pub y: Option<String>,
    pub color: Option<String>,
}

impl AestheticMap {
    pub fn new(x: impl Into<String>) -> Self {
        Self {
            x: x.into(),
            y: None,
            color: None,
        }
    }

    pub fn with_y(mut self, y: impl Into<String>) -> Self {
        self.y = Some(y.into());
        self
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }
}

/// A simple linear regression fit (slope + intercept), computed by the
/// caller — e.g. `ghl-runtime`'s statistics engine — and handed to a
/// `Smooth` layer. This crate only renders a trend line; it never fits one,
/// so the same fit always reaches both the terminal and PNG/SVG renderers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearFit {
    pub slope: f64,
    pub intercept: f64,
}

/// A Tukey five-number summary (with 1.5×IQR fences and detected outliers),
/// computed by the caller and handed to a `Boxplot` layer, for the same
/// reason as [`LinearFit`].
#[derive(Debug, Clone, PartialEq)]
pub struct FiveNumberSummary {
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    pub mean: f64,
    pub lower_fence: f64,
    pub upper_fence: f64,
    pub outliers: Vec<f64>,
}

/// Equal-width binning of `PlotSpec::x_data`, shared by both histogram
/// renderers (see `PlotSpec::histogram_bins`).
struct HistogramBins {
    min_x: f64,
    max_x: f64,
    bin_width: f64,
    counts: Vec<u32>,
    max_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GeomKind {
    Point { glyph: Option<char> },
    Line,
    Smooth { fit: Option<LinearFit> },
    Histogram { bins: usize },
    Boxplot { stats: Option<FiveNumberSummary> },
    Bar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeomLayer {
    pub kind: GeomKind,
}

impl GeomLayer {
    pub fn point() -> Self {
        Self {
            kind: GeomKind::Point { glyph: None },
        }
    }

    pub fn line() -> Self {
        Self {
            kind: GeomKind::Line,
        }
    }

    /// A `geom_smooth()` layer with no fit yet; nothing is drawn until a fit
    /// is supplied via [`GeomLayer::smooth_with_fit`].
    pub fn smooth() -> Self {
        Self {
            kind: GeomKind::Smooth { fit: None },
        }
    }

    /// A `geom_smooth()` layer carrying a fit already computed by the caller.
    pub fn smooth_with_fit(fit: LinearFit) -> Self {
        Self {
            kind: GeomKind::Smooth { fit: Some(fit) },
        }
    }

    pub fn histogram(bins: usize) -> Self {
        Self {
            kind: GeomKind::Histogram { bins },
        }
    }

    /// A `geom_boxplot()` layer with no statistics yet; rendering will report
    /// insufficient data until statistics are supplied via
    /// [`GeomLayer::boxplot_with_stats`].
    pub fn boxplot() -> Self {
        Self {
            kind: GeomKind::Boxplot { stats: None },
        }
    }

    /// A `geom_boxplot()` layer carrying a five-number summary already
    /// computed by the caller.
    pub fn boxplot_with_stats(stats: FiveNumberSummary) -> Self {
        Self {
            kind: GeomKind::Boxplot { stats: Some(stats) },
        }
    }

    pub fn bar() -> Self {
        Self {
            kind: GeomKind::Bar,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlotTheme {
    #[default]
    Default,
    Minimal,
    Classic,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlotLabels {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlotSpec {
    pub mapping: Option<AestheticMap>,
    pub layers: Vec<GeomLayer>,
    pub labels: PlotLabels,
    pub x_data: Vec<f64>,
    pub y_data: Vec<f64>,
    pub categories: Vec<String>,
    pub width: usize,
    pub height: usize,
    pub theme: PlotTheme,
}

impl PlotSpec {
    pub fn new() -> Self {
        Self {
            mapping: None,
            layers: Vec::new(),
            labels: PlotLabels::default(),
            x_data: Vec::new(),
            y_data: Vec::new(),
            categories: Vec::new(),
            width: 58,
            height: 12,
            theme: PlotTheme::Default,
        }
    }

    pub fn theme_minimal(mut self) -> Self {
        self.theme = PlotTheme::Minimal;
        self
    }

    pub fn theme_classic(mut self) -> Self {
        self.theme = PlotTheme::Classic;
        self
    }

    pub fn theme_dark(mut self) -> Self {
        self.theme = PlotTheme::Dark;
        self
    }

    pub fn with_mapping(mut self, map: AestheticMap) -> Self {
        self.mapping = Some(map);
        self
    }

    pub fn add_layer(mut self, layer: GeomLayer) -> Self {
        self.layers.push(layer);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.labels.title = Some(title.into());
        self
    }

    pub fn with_labels(mut self, title: Option<String>, x: Option<String>, y: Option<String>) -> Self {
        self.labels.title = title;
        self.labels.x_label = x;
        self.labels.y_label = y;
        self
    }

    pub fn with_xy_data(mut self, x: Vec<f64>, y: Vec<f64>) -> Self {
        self.x_data = x;
        self.y_data = y;
        self
    }

    pub fn with_x_data(mut self, x: Vec<f64>) -> Self {
        self.x_data = x;
        self
    }

    pub fn with_categories(mut self, cats: Vec<String>) -> Self {
        self.categories = cats;
        self
    }

    /// Look up the fit carried by this spec's `Smooth` layer, if any.
    fn smooth_fit(&self) -> Option<LinearFit> {
        self.layers.iter().find_map(|l| match &l.kind {
            GeomKind::Smooth { fit } => *fit,
            _ => None,
        })
    }

    /// Look up the statistics carried by this spec's `Boxplot` layer, if any.
    fn boxplot_stats(&self) -> Option<FiveNumberSummary> {
        self.layers.iter().find_map(|l| match &l.kind {
            GeomKind::Boxplot { stats } => stats.clone(),
            _ => None,
        })
    }

    /// Number of histogram bins requested via the `Histogram` layer, or 8 if none is present.
    fn requested_bins(&self) -> usize {
        self.layers.iter().find_map(|l| match l.kind {
            GeomKind::Histogram { bins } => Some(bins),
            _ => None,
        }).unwrap_or(8)
    }

    /// Bin `self.x_data` into `bins_count` equal-width buckets, shared by both
    /// the PNG/SVG and terminal histogram renderers so they always agree.
    fn histogram_bins(&self, bins_count: usize) -> Option<HistogramBins> {
        if self.x_data.is_empty() {
            return None;
        }

        let min_x = self.x_data.iter().copied().fold(f64::INFINITY, f64::min);
        let max_x = self.x_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let span = if (max_x - min_x).abs() < 1e-9 { 1.0 } else { max_x - min_x };
        let bin_width = span / bins_count as f64;

        let mut counts = vec![0u32; bins_count];
        for &x in &self.x_data {
            let idx = (((x - min_x) / span) * bins_count as f64).floor() as usize;
            let idx = idx.min(bins_count - 1);
            counts[idx] += 1;
        }
        let max_count = *counts.iter().max().unwrap_or(&1);

        Some(HistogramBins { min_x, max_x, bin_width, counts, max_count })
    }

    /// Category → frequency counts for a bar chart: uses `self.categories` if
    /// present, otherwise falls back to `self.x_data` formatted as strings.
    /// Shared by both bar chart renderers so they always agree on which data
    /// they're counting.
    fn bar_counts(&self) -> Option<std::collections::BTreeMap<String, usize>> {
        let cats: Vec<String> = if !self.categories.is_empty() {
            self.categories.clone()
        } else if !self.x_data.is_empty() {
            self.x_data.iter().map(|x| format!("{x}")).collect()
        } else {
            return None;
        };

        let mut counts_map: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for cat in &cats {
            *counts_map.entry(cat.clone()).or_insert(0) += 1;
        }
        Some(counts_map)
    }

    /// Exports the plot to a publication-ready raster (PNG) or vector (SVG) image file via plotters.
    pub fn save_file(&self, path: &str) -> Result<(), String> {
        self.save_file_with_size(path, 800, 600)
    }

    /// Exports the plot with custom pixel dimensions (width x height).
    pub fn save_file_with_size(&self, path: &str, width: u32, height: u32) -> Result<(), String> {
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
            self.draw_chart(&root)?;
            root.present().map_err(|e| format!("Failed to write SVG: {e}"))?;
        } else {
            let root = BitMapBackend::new(path, (width, height)).into_drawing_area();
            self.draw_chart(&root)?;
            root.present().map_err(|e| format!("Failed to write bitmap: {e}"))?;
        }

        Ok(())
    }

    /// Renders the plot directly to an SVG vector string.
    pub fn to_svg(&self, width: u32, height: u32) -> Result<String, String> {
        let mut buf = String::new();
        {
            let root = SVGBackend::with_string(&mut buf, (width, height)).into_drawing_area();
            self.draw_chart(&root)?;
            root.present().map_err(|e| format!("Failed to render SVG: {e}"))?;
        }
        Ok(buf)
    }

    fn draw_chart<DB: DrawingBackend>(&self, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let bg_color = match self.theme {
            PlotTheme::Dark => RGBColor(24, 24, 27),
            _ => WHITE,
        };
        root.fill(&bg_color).map_err(|e| format!("{e}"))?;

        let is_hist = self.layers.iter().any(|l| matches!(l.kind, GeomKind::Histogram { .. }));
        let is_box = self.layers.iter().any(|l| matches!(l.kind, GeomKind::Boxplot { .. }));
        let is_bar = self.layers.iter().any(|l| matches!(l.kind, GeomKind::Bar));

        if is_hist {
            self.draw_plotters_histogram(root)?;
        } else if is_box {
            self.draw_plotters_boxplot(root)?;
        } else if is_bar {
            self.draw_plotters_bar(root)?;
        } else {
            self.draw_plotters_scatter(root)?;
        }

        Ok(())
    }

    fn draw_plotters_scatter<DB: DrawingBackend>(&self, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let n_points = self.x_data.len().min(self.y_data.len());
        if n_points == 0 {
            return Err("Cannot plot empty scatter data".to_string());
        }

        let min_x = self.x_data.iter().copied().fold(f64::INFINITY, f64::min);
        let max_x = self.x_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min_y = self.y_data.iter().copied().fold(f64::INFINITY, f64::min);
        let max_y = self.y_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        let pad_x = if (max_x - min_x).abs() < 1e-6 { 1.0 } else { (max_x - min_x) * 0.08 };
        let pad_y = if (max_y - min_y).abs() < 1e-6 { 1.0 } else { (max_y - min_y) * 0.08 };

        let x_range = (min_x - pad_x)..(max_x + pad_x);
        let y_range = (min_y - pad_y)..(max_y + pad_y);

        let title = self.labels.title.as_deref().unwrap_or("Scatter Plot");
        let x_label = self.labels.x_label.as_deref().unwrap_or("x");
        let y_label = self.labels.y_label.as_deref().unwrap_or("y");

        let mut chart = ChartBuilder::on(root)
            .caption(title, ("sans-serif", 24).into_font().color(&BLACK))
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(x_range, y_range)
            .map_err(|e| format!("Chart build error: {e}"))?;

        chart
            .configure_mesh()
            .x_desc(x_label)
            .y_desc(y_label)
            .axis_desc_style(("sans-serif", 16).into_font().color(&BLACK))
            .label_style(("sans-serif", 12).into_font().color(&BLACK))
            .draw()
            .map_err(|e| format!("Mesh draw error: {e}"))?;

        // Draw scatter points
        let points_data: Vec<(f64, f64)> = self.x_data.iter().copied().zip(self.y_data.iter().copied()).collect();
        chart
            .draw_series(points_data.iter().map(|&(x, y)| {
                Circle::new((x, y), 5, RGBColor(79, 110, 242).filled())
            }))
            .map_err(|e| format!("Points draw error: {e}"))?;

        // Draw the precomputed OLS smooth line, if the `Smooth` layer carries one
        if let Some(fit) = self.smooth_fit() {
            let line_pts = vec![
                (min_x, fit.intercept + fit.slope * min_x),
                (max_x, fit.intercept + fit.slope * max_x),
            ];
            chart
                .draw_series(LineSeries::new(line_pts, &RGBColor(224, 100, 50)).point_size(2))
                .map_err(|e| format!("Line draw error: {e}"))?;
        }

        Ok(())
    }

    fn draw_plotters_histogram<DB: DrawingBackend>(&self, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let bins_count = self.requested_bins().clamp(4, 30);
        let HistogramBins { min_x, max_x, bin_width, counts, max_count } = self
            .histogram_bins(bins_count)
            .ok_or_else(|| "Cannot plot empty histogram data".to_string())?;

        let title = self.labels.title.as_deref().unwrap_or("Histogram");
        let x_label = self.labels.x_label.as_deref().unwrap_or("Value");

        let mut chart = ChartBuilder::on(root)
            .caption(title, ("sans-serif", 24).into_font().color(&BLACK))
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(min_x..(max_x + bin_width * 0.1), 0u32..(max_count + 1))
            .map_err(|e| format!("Chart build error: {e}"))?;

        chart
            .configure_mesh()
            .x_desc(x_label)
            .y_desc("Frequency")
            .axis_desc_style(("sans-serif", 16).into_font().color(&BLACK))
            .label_style(("sans-serif", 12).into_font().color(&BLACK))
            .draw()
            .map_err(|e| format!("Mesh draw error: {e}"))?;

        for (i, &count) in counts.iter().enumerate() {
            let x0 = min_x + i as f64 * bin_width;
            let x1 = x0 + bin_width;
            let style = RGBColor(66, 133, 244).filled();
            chart
                .draw_series(std::iter::once(Rectangle::new([(x0, 0), (x1, count)], style)))
                .map_err(|e| format!("Histogram bar draw error: {e}"))?;
        }

        Ok(())
    }

    fn draw_plotters_boxplot<DB: DrawingBackend>(&self, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let FiveNumberSummary { min: min_v, q1, median, q3, max: max_v, lower_fence, upper_fence, outliers, .. } =
            self.boxplot_stats().ok_or_else(|| "At least 4 observations required for boxplot".to_string())?;

        let pad_y = if (max_v - min_v).abs() < 1e-6 { 1.0 } else { (max_v - min_v) * 0.1 };
        let y_range = (min_v - pad_y)..(max_v + pad_y);

        let title = self.labels.title.as_deref().unwrap_or("Tukey Box-and-Whisker Plot");
        let x_label = self.labels.x_label.as_deref().unwrap_or("Group");
        let y_label = self.labels.y_label.as_deref().unwrap_or("Value");

        let text_color = match self.theme {
            PlotTheme::Dark => RGBColor(240, 240, 240),
            _ => BLACK,
        };

        let mut chart = ChartBuilder::on(root)
            .caption(title, ("sans-serif", 24).into_font().color(&text_color))
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(0.0..2.0, y_range)
            .map_err(|e| format!("Chart build error: {e}"))?;

        chart
            .configure_mesh()
            .x_desc(x_label)
            .y_desc(y_label)
            .axis_desc_style(("sans-serif", 16).into_font().color(&text_color))
            .label_style(("sans-serif", 12).into_font().color(&text_color))
            .draw()
            .map_err(|e| format!("Mesh draw error: {e}"))?;

        let center_x = 1.0;
        let box_half_w = 0.35;
        let whisker_half_w = 0.18;
        let box_color = RGBColor(66, 133, 244);
        let border_color = RGBColor(26, 82, 118);

        // 1. Vertical whisker lines
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x, lower_fence), (center_x, q1)],
            ShapeStyle::from(&border_color).stroke_width(2),
        ))).map_err(|e| format!("Whisker draw error: {e}"))?;

        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x, q3), (center_x, upper_fence)],
            ShapeStyle::from(&border_color).stroke_width(2),
        ))).map_err(|e| format!("Whisker draw error: {e}"))?;

        // 2. Whisker end caps
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x - whisker_half_w, lower_fence), (center_x + whisker_half_w, lower_fence)],
            ShapeStyle::from(&border_color).stroke_width(2),
        ))).map_err(|e| format!("Whisker cap draw error: {e}"))?;

        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x - whisker_half_w, upper_fence), (center_x + whisker_half_w, upper_fence)],
            ShapeStyle::from(&border_color).stroke_width(2),
        ))).map_err(|e| format!("Whisker cap draw error: {e}"))?;

        // 3. IQR Box
        chart.draw_series(std::iter::once(Rectangle::new(
            [(center_x - box_half_w, q1), (center_x + box_half_w, q3)],
            ShapeStyle::from(&box_color.mix(0.6)).filled(),
        ))).map_err(|e| format!("Box draw error: {e}"))?;

        chart.draw_series(std::iter::once(PathElement::new(
            vec![
                (center_x - box_half_w, q1),
                (center_x + box_half_w, q1),
                (center_x + box_half_w, q3),
                (center_x - box_half_w, q3),
                (center_x - box_half_w, q1),
            ],
            ShapeStyle::from(&border_color).stroke_width(2),
        ))).map_err(|e| format!("Box border error: {e}"))?;

        // 4. Median line
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(center_x - box_half_w, median), (center_x + box_half_w, median)],
            ShapeStyle::from(&RED).stroke_width(3),
        ))).map_err(|e| format!("Median line error: {e}"))?;

        // 5. Outliers
        for &o in &outliers {
            chart.draw_series(std::iter::once(Circle::new(
                (center_x, o),
                4,
                ShapeStyle::from(&RED).filled(),
            ))).map_err(|e| format!("Outlier draw error: {e}"))?;
        }

        Ok(())
    }

    fn draw_plotters_bar<DB: DrawingBackend>(&self, root: &DrawingArea<DB, plotters::coord::Shift>) -> Result<(), String> {
        let counts_map = self.bar_counts().ok_or_else(|| "No categories to plot in bar chart".to_string())?;

        let n_cats = counts_map.len();
        let max_count = *counts_map.values().max().unwrap_or(&1);

        let title = self.labels.title.as_deref().unwrap_or("Category Frequencies");
        let x_label = self.labels.x_label.as_deref().unwrap_or("Category");
        let y_label = self.labels.y_label.as_deref().unwrap_or("Count");

        let text_color = match self.theme {
            PlotTheme::Dark => RGBColor(240, 240, 240),
            _ => BLACK,
        };

        let mut chart = ChartBuilder::on(root)
            .caption(title, ("sans-serif", 24).into_font().color(&text_color))
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(0.0..(n_cats as f64 + 1.0), 0u32..(max_count as u32 + 1))
            .map_err(|e| format!("Chart build error: {e}"))?;

        chart
            .configure_mesh()
            .x_desc(x_label)
            .y_desc(y_label)
            .axis_desc_style(("sans-serif", 16).into_font().color(&text_color))
            .label_style(("sans-serif", 12).into_font().color(&text_color))
            .draw()
            .map_err(|e| format!("Mesh draw error: {e}"))?;

        let bar_half_w = 0.35;
        let bar_color = RGBColor(52, 168, 83);

        for (i, (_cat, &count)) in counts_map.iter().enumerate() {
            let cx = (i + 1) as f64;
            let x0 = cx - bar_half_w;
            let x1 = cx + bar_half_w;
            chart
                .draw_series(std::iter::once(Rectangle::new(
                    [(x0, 0), (x1, count as u32)],
                    ShapeStyle::from(&bar_color).filled(),
                )))
                .map_err(|e| format!("Bar draw error: {e}"))?;
        }

        Ok(())
    }

    /// Renders the plot into a complete Cockpit Deck terminal card.
    pub fn render(&self, caps: &RenderCaps) -> String {
        // Determine primary plot type from layers
        let is_hist = self.layers.iter().any(|l| matches!(l.kind, GeomKind::Histogram { .. }));
        let is_box = self.layers.iter().any(|l| matches!(l.kind, GeomKind::Boxplot { .. }));
        let is_bar = self.layers.iter().any(|l| matches!(l.kind, GeomKind::Bar));

        if is_hist {
            self.render_histogram(self.requested_bins(), caps)
        } else if is_box {
            self.render_boxplot(caps)
        } else if is_bar {
            self.render_bar(caps)
        } else {
            // Default: 2D Cartesian (scatter / line / smooth)
            self.render_scatter(caps)
        }
    }

    fn render_scatter(&self, caps: &RenderCaps) -> String {
        let (tl, tr, bl, br, hz, vt, sep_l, sep_r) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│', '├', '┤')
        } else {
            ('+', '+', '+', '+', '-', '|', '+', '+')
        };

        let card_width = 72.min(caps.width);
        let plot_w = card_width.saturating_sub(18).max(20);
        let plot_h = self.height.max(8);

        let n_points = self.x_data.len().min(self.y_data.len());
        if n_points == 0 {
            return format!("{tl}{}{tr}\n{vt} (Empty scatter data) {vt}\n{bl}{}{br}",
                hz.to_string().repeat(card_width - 2),
                hz.to_string().repeat(card_width - 2)
            );
        }

        // Bounding box
        let min_x = self.x_data.iter().copied().fold(f64::INFINITY, f64::min);
        let max_x = self.x_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min_y = self.y_data.iter().copied().fold(f64::INFINITY, f64::min);
        let max_y = self.y_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        let span_x = if (max_x - min_x).abs() < 1e-9 { 1.0 } else { max_x - min_x };
        let span_y = if (max_y - min_y).abs() < 1e-9 { 1.0 } else { max_y - min_y };

        // Grid canvas
        let mut grid = vec![vec![' '; plot_w]; plot_h];

        // Draw the precomputed OLS smooth line, if the `Smooth` layer carries one
        if let Some(fit) = self.smooth_fit() {
            for c in 0..plot_w {
                let cur_x = min_x + (c as f64 / (plot_w - 1) as f64) * span_x;
                let cur_y = fit.intercept + fit.slope * cur_x;
                let norm_y = ((cur_y - min_y) / span_y).clamp(0.0, 1.0);
                let row = ((1.0 - norm_y) * (plot_h - 1) as f64).round() as usize;
                if row < plot_h {
                    grid[row][c] = if caps.unicode_enabled { '·' } else { '.' };
                }
            }
        }

        // Plot points
        let point_glyph = if caps.unicode_enabled { '●' } else { '*' };
        for i in 0..n_points {
            let px = ((self.x_data[i] - min_x) / span_x).clamp(0.0, 1.0);
            let py = ((self.y_data[i] - min_y) / span_y).clamp(0.0, 1.0);

            let c = (px * (plot_w - 1) as f64).round() as usize;
            let r = ((1.0 - py) * (plot_h - 1) as f64).round() as usize;

            if r < plot_h && c < plot_w {
                grid[r][c] = point_glyph;
            }
        }

        // Build output card
        let title = self.labels.title.clone().unwrap_or_else(|| "Scatter Plot".into());
        let badge = if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ READY" } else { "[READY]" };


        let mut out = String::new();
        // Top border
        let header_inner = format!(" {title} ");
        let pad_top = card_width.saturating_sub(visual_width(&header_inner) + visual_width(badge) + 6);
        out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

        // Y label if any
        if let Some(ref yl) = self.labels.y_label {
            let y_header = format!("{yl} ▲");
            let y_pad = card_width.saturating_sub(visual_width(&y_header) + 4);
            out.push_str(&format!("{vt} {}{}{vt}\n", caps.bold(&y_header), " ".repeat(y_pad)));
        }

        // Grid lines with Y axis labels
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

            let mut row_chars = String::with_capacity(plot_w);
            for &ch in &grid[r] {
                if ch == point_glyph {
                    row_chars.push_str(&caps.cyan(&ch.to_string()));
                } else if ch == '·' || ch == '.' {
                    row_chars.push_str(&caps.yellow(&ch.to_string()));
                } else {
                    row_chars.push(ch);
                }
            }

            let inner_content = format!(" {} {tick} {row_chars}", caps.dim(&y_label));
            let vlen = visual_width(&inner_content);
            let pad = " ".repeat(card_width.saturating_sub(vlen + 2));
            out.push_str(&format!("{vt}{inner_content}{pad}{vt}\n"));
        }

        // Bottom X axis
        let x_axis_line = if caps.unicode_enabled {
            format!("         └{}", "─".repeat(plot_w + 1))
        } else {
            format!("         +{}", "-".repeat(plot_w + 1))
        };
        let pad_axis = " ".repeat(card_width.saturating_sub(visual_width(&x_axis_line) + 2));
        out.push_str(&format!("{vt}{}{pad_axis}{vt}\n", caps.dim(&x_axis_line)));

        // X labels
        let x_min_str = format!("{:<7.1}", min_x);
        let x_max_str = format!("{:>7.1}", max_x);
        let x_spaces = plot_w.saturating_sub(14);
        let x_tick_labels = format!("          {x_min_str}{}{x_max_str}", " ".repeat(x_spaces));
        let pad_x = " ".repeat(card_width.saturating_sub(visual_width(&x_tick_labels) + 2));
        out.push_str(&format!("{vt}{}{pad_x}{vt}\n", caps.dim(&x_tick_labels)));

        // X title if any
        if let Some(ref xl) = self.labels.x_label {
            let xl_str = format!("► {xl}");
            let xl_left_pad = (card_width.saturating_sub(visual_width(&xl_str) + 2)) / 2;
            let xl_right_pad = card_width.saturating_sub(visual_width(&xl_str) + 2 + xl_left_pad);
            out.push_str(&format!("{vt}{}{}{}{vt}\n", " ".repeat(xl_left_pad), caps.bold(&xl_str), " ".repeat(xl_right_pad)));
        }

        // Footer telemetry
        let footer_div = format!("{sep_l}{}{sep_r}", hz.to_string().repeat(card_width - 2));
        out.push_str(&caps.dim(&format!("{footer_div}\n")));

        let tele_text = if caps.unicode_enabled {
            format!("/ᐠ˵- ⩊ -˵マ Rendered {} data points with OLS trend line", n_points)
        } else {
            format!("Rendered {} data points with OLS trend line", n_points)
        };

        let pad_tele = " ".repeat(card_width.saturating_sub(visual_width(&tele_text) + 4));
        out.push_str(&format!("{vt} {}{pad_tele} {vt}\n", caps.green(&tele_text)));

        // Bottom border
        out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));

        out
    }

    fn render_histogram(&self, num_bins: usize, caps: &RenderCaps) -> String {
        let (tl, tr, bl, br, hz, vt, sep_l, sep_r) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│', '├', '┤')
        } else {
            ('+', '+', '+', '+', '-', '|', '+', '+')
        };

        let card_width = 72.min(caps.width);
        let bins_count = num_bins.clamp(4, 16);
        let Some(HistogramBins { min_x, max_x, bin_width, counts, max_count }) = self.histogram_bins(bins_count) else {
            return "Empty data for histogram".to_string();
        };
        let n = self.x_data.len();
        let hist_height = 8usize;

        // Unicode 8-level blocks: ' ', ' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'
        let blocks = [' ', ' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

        let mut out = String::new();
        let title = self.labels.title.clone().unwrap_or_else(|| "Frequency Distribution (Histogram)".into());
        let badge = if caps.unicode_enabled { "ฅ(•⩊ •マ NEKO" } else { "[NEKO]" };

        let pad_top = card_width.saturating_sub(visual_width(&title) + visual_width(badge) + 8);
        out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

        for h in (0..hist_height).rev() {
            let row_val = ((h + 1) as f64 / hist_height as f64) * max_count as f64;
            let label = if h == hist_height - 1 || h == hist_height / 2 || h == 0 {
                format!("{:>5.0}", row_val)
            } else {
                "     ".to_string()
            };

            let tick = if caps.unicode_enabled { "┤" } else { "|" };
            let mut bar_line = String::new();

            for &c in &counts {
                let col_height_fraction = (c as f64 / max_count as f64) * hist_height as f64;
                let full_bars = col_height_fraction.floor() as usize;
                let rem = col_height_fraction - full_bars as f64;

                let ch = if h < full_bars {
                    if caps.unicode_enabled { '█' } else { '#' }
                } else if h == full_bars && rem > 0.0 {
                    if caps.unicode_enabled {
                        let block_idx = (rem * 8.0).round() as usize;
                        blocks[block_idx.clamp(1, 8)]
                    } else {
                        ':'
                    }
                } else {
                    ' '
                };

                bar_line.push_str(&format!("  {ch} "));
            }

            let inner = format!(" {} {tick} {}", caps.dim(&label), caps.cyan(&bar_line));
            let vlen = visual_width(&inner);
            let pad = " ".repeat(card_width.saturating_sub(vlen + 2));
            out.push_str(&format!("{vt}{inner}{pad}{vt}\n"));
        }

        // Bottom axis line
        let bot_axis = if caps.unicode_enabled {
            format!("       └{}", "───┬".repeat(bins_count))
        } else {
            format!("       +{}", "---+".repeat(bins_count))
        };
        let pad_ax = " ".repeat(card_width.saturating_sub(visual_width(&bot_axis) + 2));
        out.push_str(&format!("{vt}{}{pad_ax}{vt}\n", caps.dim(&bot_axis)));

        // Range labels
        let min_s = format!("{:<6.1}", min_x);
        let max_s = format!("{:>6.1}", max_x);
        let mid_s = format!("{:^6.1}", min_x + (bin_width * bins_count as f64) / 2.0);
        let span_pad = (bins_count * 4).saturating_sub(18);
        let range_labels = format!("        {min_s}{mid_s}{}{max_s}", " ".repeat(span_pad));
        let pad_r = " ".repeat(card_width.saturating_sub(visual_width(&range_labels) + 2));
        out.push_str(&format!("{vt}{}{pad_r}{vt}\n", caps.dim(&range_labels)));

        // Telemetry
        let div = format!("{sep_l}{}{sep_r}", hz.to_string().repeat(card_width - 2));
        out.push_str(&caps.dim(&format!("{div}\n")));
        let tele = format!("Observations: {} | Bins: {} | Width: {:.2} | Max Count: {}", n, bins_count, bin_width, max_count);
        let pad_t = " ".repeat(card_width.saturating_sub(visual_width(&tele) + 4));
        out.push_str(&format!("{vt} {}{pad_t} {vt}\n", caps.dim(&tele)));

        out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));
        out
    }

    fn render_boxplot(&self, caps: &RenderCaps) -> String {
        let (tl, tr, bl, br, hz, vt, sep_l, sep_r) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│', '├', '┤')
        } else {
            ('+', '+', '+', '+', '-', '|', '+', '+')
        };

        let card_width = 72.min(caps.width);
        let Some(FiveNumberSummary { min: min_v, q1, median, q3, max: max_v, mean, lower_fence, upper_fence, outliers }) =
            self.boxplot_stats()
        else {
            return "At least 4 observations required for boxplot".to_string();
        };
        let iqr = q3 - q1;

        // 1D horizontal bar mapping
        let bar_width = card_width.saturating_sub(20).max(24);
        let span = if (max_v - min_v).abs() < 1e-9 { 1.0 } else { max_v - min_v };

        let map_col = |v: f64| -> usize {
            let frac = ((v - min_v) / span).clamp(0.0, 1.0);
            (frac * (bar_width - 1) as f64).round() as usize
        };

        let col_lf = map_col(lower_fence);
        let col_q1 = map_col(q1);
        let col_med = map_col(median);
        let col_q3 = map_col(q3);
        let col_uf = map_col(upper_fence);

        let mut bar = vec![' '; bar_width];
        // Lower whisker
        for c in col_lf..col_q1 {
            bar[c] = if caps.unicode_enabled { '─' } else { '-' };
        }
        bar[col_lf] = if caps.unicode_enabled { '├' } else { '|' };

        // Box
        for c in col_q1..=col_q3 {
            bar[c] = ' ';
        }
        bar[col_q1] = '[';
        bar[col_med] = if caps.unicode_enabled { '│' } else { '|' };
        bar[col_q3] = ']';

        // Upper whisker
        for c in (col_q3 + 1)..=col_uf {
            bar[c] = if caps.unicode_enabled { '─' } else { '-' };
        }
        bar[col_uf] = if caps.unicode_enabled { '┤' } else { '|' };

        // Outliers
        for &o in &outliers {
            let co = map_col(o);
            if co < bar_width {
                bar[co] = '*';
            }
        }

        let bar_str: String = bar.into_iter().collect();

        let mut out = String::new();
        let title = self.labels.title.clone().unwrap_or_else(|| "Tukey Box-and-Whisker Plot".into());
        let badge = if caps.unicode_enabled { "/ᐠ˵- ⩊ -˵マ ✧ QUANTILES" } else { "[QUANTILES]" };


        let pad_top = card_width.saturating_sub(visual_width(&title) + visual_width(badge) + 8);
        out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

        let box_line = format!("    {}", caps.cyan(&bar_str));
        let pad_box = " ".repeat(card_width.saturating_sub(visual_width(&box_line) + 2));
        out.push_str(&format!("{vt}{box_line}{pad_box}{vt}\n"));

        // Axis underneath box
        let min_s = format!("{:<7.1}", min_v);
        let max_s = format!("{:>7.1}", max_v);
        let med_s = format!("{:^7.1}", median);
        let axis_pad = bar_width.saturating_sub(21);
        let axis_labels = format!("    {min_s}{med_s}{}{max_s}", " ".repeat(axis_pad));
        let pad_ax = " ".repeat(card_width.saturating_sub(visual_width(&axis_labels) + 2));
        out.push_str(&format!("{vt}{}{pad_ax}{vt}\n", caps.dim(&axis_labels)));

        // Statistical summary table
        let div = format!("{sep_l}{}{sep_r}", hz.to_string().repeat(card_width - 2));
        out.push_str(&caps.dim(&format!("{div}\n")));

        let s1 = format!("Min: {:.2} | Q1: {:.2} | Median: {:.2} | Mean: {:.2}",
            min_v, q1, median, mean
        );
        let pad_s1 = " ".repeat(card_width.saturating_sub(visual_width(&s1) + 4));
        out.push_str(&format!("{vt} {}{pad_s1} {vt}\n", caps.bold(&s1)));

        let s2 = format!("Q3: {:.2} | Max: {:.2} | IQR: {:.2} | Outliers: {}",
            q3, max_v, iqr, outliers.len()
        );
        let pad_s2 = " ".repeat(card_width.saturating_sub(visual_width(&s2) + 4));
        out.push_str(&format!("{vt} {}{pad_s2} {vt}\n", caps.dim(&s2)));

        out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));
        out
    }

    fn render_bar(&self, caps: &RenderCaps) -> String {
        let (tl, tr, bl, br, hz, vt, sep_l, sep_r) = if caps.unicode_enabled {
            ('╭', '╮', '╰', '╯', '─', '│', '├', '┤')
        } else {
            ('+', '+', '+', '+', '-', '|', '+', '+')
        };

        let card_width = 72.min(caps.width);
        let Some(counts_map) = self.bar_counts() else {
            return "No categories to display in bar chart".to_string();
        };

        let max_cat_len = counts_map.keys().map(|k| visual_width(k)).max().unwrap_or(8).min(16);
        let max_count = *counts_map.values().max().unwrap_or(&1).max(&1);
        let max_bar_len = card_width.saturating_sub(max_cat_len + 16).max(10);

        let mut out = String::new();
        let title = self.labels.title.clone().unwrap_or_else(|| "Category Frequencies (Bar Chart)".into());
        let badge = if caps.unicode_enabled { "ฅ(•⩊ •マ STATS" } else { "[STATS]" };

        let pad_top = card_width.saturating_sub(visual_width(&title) + visual_width(badge) + 8);
        out.push_str(&caps.dim(&format!("{tl}{hz} {title} {}{hz} {badge} {hz}{tr}\n", hz.to_string().repeat(pad_top))));

        for (cat, count) in &counts_map {
            let bar_len = ((count * max_bar_len) / max_count).max(1);
            let bar_ch = if caps.unicode_enabled { "█" } else { "#" };
            let bar_str = bar_ch.repeat(bar_len);

            let pad_cat = " ".repeat(max_cat_len.saturating_sub(visual_width(cat)));
            let row_line = format!(" {pad_cat}{} {} {} ({count})", caps.bold(cat), caps.dim("│"), caps.cyan(&bar_str));
            let pad_row = " ".repeat(card_width.saturating_sub(visual_width(&row_line) + 2));
            out.push_str(&format!("{vt}{row_line}{pad_row}{vt}\n"));
        }

        let div = format!("{sep_l}{}{sep_r}", hz.to_string().repeat(card_width - 2));
        out.push_str(&caps.dim(&format!("{div}\n")));

        let total_observations: usize = counts_map.values().sum();
        let tele = format!("Total Categories: {} | Total Observations: {}", counts_map.len(), total_observations);
        let pad_t = " ".repeat(card_width.saturating_sub(visual_width(&tele) + 4));
        out.push_str(&format!("{vt} {}{pad_t} {vt}\n", caps.dim(&tele)));

        out.push_str(&caps.dim(&format!("{bl}{}{br}\n", hz.to_string().repeat(card_width - 2))));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scatter_plot_render() {
        let caps = RenderCaps::rich_terminal(72);
        let plot = PlotSpec::new()
            .with_title("Test Scatter")
            .with_xy_data(vec![1.0, 2.0, 3.0, 4.0], vec![10.0, 20.0, 30.0, 40.0])
            .add_layer(GeomLayer::point())
            .add_layer(GeomLayer::smooth_with_fit(LinearFit { slope: 10.0, intercept: 0.0 }));

        let rendered = plot.render(&caps);
        assert!(rendered.contains("Rendered 4 data points"));
    }


    #[test]
    fn test_histogram_render() {
        let caps = RenderCaps::rich_terminal(72);
        let plot = PlotSpec::new()
            .with_title("Test Histogram")
            .with_x_data(vec![1.0, 1.2, 1.3, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0])
            .add_layer(GeomLayer::histogram(6));

        let rendered = plot.render(&caps);
        assert!(rendered.contains("Test Histogram"));
        assert!(rendered.contains("Observations: 9"));
    }

    #[test]
    fn test_boxplot_render() {
        let caps = RenderCaps::rich_terminal(72);
        let stats = FiveNumberSummary {
            min: 10.0, q1: 20.0, median: 30.0, q3: 40.0, max: 100.0, mean: 34.375,
            lower_fence: 10.0, upper_fence: 70.0, outliers: vec![100.0],
        };
        let plot = PlotSpec::new()
            .with_title("Test Boxplot")
            .with_x_data(vec![10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 100.0])
            .add_layer(GeomLayer::boxplot_with_stats(stats));

        let rendered = plot.render(&caps);
        assert!(rendered.contains("Test Boxplot"));
        assert!(rendered.contains("Median:"));
        assert!(rendered.contains("Outliers: 1"));
    }

    #[test]
    fn test_bar_chart_render() {
        let caps = RenderCaps::rich_terminal(72);
        let plot = PlotSpec::new()
            .with_title("Test Bar")
            .with_categories(vec!["A".into(), "B".into(), "A".into(), "C".into(), "A".into()])
            .add_layer(GeomLayer::bar());

        let rendered = plot.render(&caps);
        assert!(rendered.contains("Test Bar"));
        assert!(rendered.contains("Total Categories: 3"));
    }

    /// Regression test: the terminal bar chart used to only ever read
    /// `categories`, while the PNG/SVG bar chart already fell back to
    /// `x_data` formatted as strings when `categories` was empty. Both now
    /// share `PlotSpec::bar_counts`, so this must succeed instead of
    /// returning "No categories to display in bar chart".
    #[test]
    fn test_bar_chart_render_falls_back_to_x_data() {
        let caps = RenderCaps::rich_terminal(72);
        let plot = PlotSpec::new()
            .with_title("Numeric Bar")
            .with_x_data(vec![1.0, 2.0, 1.0, 3.0, 1.0])
            .add_layer(GeomLayer::bar());

        let rendered = plot.render(&caps);
        assert!(rendered.contains("Numeric Bar"));
        assert!(rendered.contains("Total Categories: 3"));
        assert!(rendered.contains("Total Observations: 5"));
    }

    #[test]
    fn test_plotters_export_png_and_svg() {
        let plot = PlotSpec::new()
            .with_title("Plotters Verification")
            .with_xy_data(vec![1.0, 2.0, 3.0, 4.0], vec![10.0, 20.0, 30.0, 40.0])
            .add_layer(GeomLayer::point())
            .add_layer(GeomLayer::smooth());

        let temp_dir = std::env::temp_dir();
        let png_path = temp_dir.join("ghl_test_plot.png");
        let svg_path = temp_dir.join("ghl_test_plot.svg");

        assert!(plot.save_file(png_path.to_str().unwrap()).is_ok());
        assert!(png_path.exists());
        let _ = std::fs::remove_file(png_path);

        assert!(plot.save_file(svg_path.to_str().unwrap()).is_ok());
        assert!(svg_path.exists());
        let _ = std::fs::remove_file(svg_path);
    }
}
