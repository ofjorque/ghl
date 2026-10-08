//! Grammar of Graphics Specification types for GHL.
//!
//! Grounded directly in GHL native types (F64, I64, Factor, String, Bool, Date)
//! as specified in RFC 16.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScaleTransform {
    #[default]
    Linear,
    Log10,
    Sqrt,
    DiscreteBand,
    Temporal,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AestheticMap {
    pub x: String,
    pub y: Option<String>,
    pub color: Option<String>,
    pub size: Option<String>,
    pub shape: Option<String>,
    pub facet_col: Option<String>,
    pub facet_row: Option<String>,
}

impl AestheticMap {
    pub fn new(x: impl Into<String>) -> Self {
        Self {
            x: x.into(),
            y: None,
            color: None,
            size: None,
            shape: None,
            facet_col: None,
            facet_row: None,
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

    pub fn with_size(mut self, size: impl Into<String>) -> Self {
        self.size = Some(size.into());
        self
    }

    pub fn with_shape(mut self, shape: impl Into<String>) -> Self {
        self.shape = Some(shape.into());
        self
    }

    pub fn with_facet_col(mut self, facet_col: impl Into<String>) -> Self {
        self.facet_col = Some(facet_col.into());
        self
    }

    pub fn with_facet_row(mut self, facet_row: impl Into<String>) -> Self {
        self.facet_row = Some(facet_row.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FacetScales {
    #[default]
    Fixed,
    FreeX,
    FreeY,
    Free,
}

impl FacetScales {
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "free" => FacetScales::Free,
            "free_x" | "freex" => FacetScales::FreeX,
            "free_y" | "freey" => FacetScales::FreeY,
            _ => FacetScales::Fixed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FacetLayout {
    Wrap {
        variable: String,
        ncol: Option<usize>,
        nrow: Option<usize>,
        scales: FacetScales,
    },
    Grid {
        row_var: Option<String>,
        col_var: Option<String>,
        scales: FacetScales,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacetSpec {
    pub layout: FacetLayout,
}

impl FacetSpec {
    pub fn wrap(
        variable: impl Into<String>,
        ncol: Option<usize>,
        nrow: Option<usize>,
        scales: FacetScales,
    ) -> Self {
        Self {
            layout: FacetLayout::Wrap {
                variable: variable.into(),
                ncol,
                nrow,
                scales,
            },
        }
    }

    pub fn grid(row_var: Option<String>, col_var: Option<String>, scales: FacetScales) -> Self {
        Self {
            layout: FacetLayout::Grid {
                row_var,
                col_var,
                scales,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacetPanel {
    pub label: String,
    pub row_label: Option<String>,
    pub col_label: Option<String>,
    pub row_idx: usize,
    pub col_idx: usize,
    pub spec: PlotSpec,
}

/// A simple linear regression fit (slope + intercept).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinearFit {
    pub slope: f64,
    pub intercept: f64,
}

impl LinearFit {
    pub fn compute(xs: &[f64], ys: &[f64]) -> Option<Self> {
        let n = xs.len().min(ys.len());
        if n < 2 {
            return None;
        }
        let mean_x = xs.iter().take(n).sum::<f64>() / n as f64;
        let mean_y = ys.iter().take(n).sum::<f64>() / n as f64;

        let mut num = 0.0;
        let mut den = 0.0;
        for i in 0..n {
            let dx = xs[i] - mean_x;
            let dy = ys[i] - mean_y;
            num += dx * dy;
            den += dx * dx;
        }

        if den.abs() < 1e-12 {
            return None;
        }

        let slope = num / den;
        let intercept = mean_y - slope * mean_x;
        Some(Self { slope, intercept })
    }
}

/// A Tukey five-number summary with 1.5*IQR fences and detected outliers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GeomKind {
    Point {
        size: Option<f64>,
        glyph: Option<char>,
    },
    Line {
        width: Option<u32>,
    },
    Smooth {
        fit: Option<LinearFit>,
        se: bool,
    },
    Histogram {
        bins: usize,
    },
    Boxplot {
        stats: Option<FiveNumberSummary>,
        multi_stats: Vec<(String, FiveNumberSummary)>,
    },
    Bar,
    Area {
        alpha: Option<f64>,
    },
    Rug,
}

/// Local dataset attached to an individual geom layer, overriding or supplementing the global plot dataset.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LayerData {
    pub x_values: Vec<f64>,
    pub y_values: Vec<f64>,
    pub categories: Vec<String>,
    pub size_values: Vec<f64>,
    pub shape_values: Vec<String>,
    pub series: Vec<DataSeries>,
    pub columns_cache: BTreeMap<String, Vec<String>>,
}

impl LayerData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_xy(mut self, x: Vec<f64>, y: Vec<f64>) -> Self {
        self.x_values = x;
        self.y_values = y;
        self
    }

    pub fn with_series(mut self, series: Vec<DataSeries>) -> Self {
        self.series = series;
        self
    }

    pub fn with_column_cache(mut self, name: impl Into<String>, data: Vec<String>) -> Self {
        self.columns_cache.insert(name.into(), data);
        self
    }

    pub fn resolve_mapping(&mut self, mapping: &AestheticMap) {
        if self.x_values.is_empty() {
            if let Some(col) = self.columns_cache.get(&mapping.x) {
                self.x_values = col.iter().filter_map(|s| s.parse::<f64>().ok()).collect();
                if self.x_values.is_empty() {
                    self.categories = col.clone();
                }
            }
        }
        if self.y_values.is_empty() {
            if let Some(ref y_name) = mapping.y {
                if let Some(col) = self.columns_cache.get(y_name) {
                    self.y_values = col.iter().filter_map(|s| s.parse::<f64>().ok()).collect();
                }
            }
        }
    }

    pub fn all_series(&self) -> Vec<DataSeries> {
        if !self.series.is_empty() {
            self.series.clone()
        } else if !self.x_values.is_empty() {
            vec![DataSeries {
                group_name: None,
                color_hex: None,
                x_values: self.x_values.clone(),
                y_values: self.y_values.clone(),
                categories: self.categories.clone(),
                size_values: self.size_values.clone(),
                shape_values: self.shape_values.clone(),
            }]
        } else {
            Vec::new()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeomLayer {
    pub kind: GeomKind,
    pub mapping: Option<AestheticMap>,
    pub data: Option<LayerData>,
}

impl GeomLayer {
    pub fn point() -> Self {
        Self {
            kind: GeomKind::Point {
                size: None,
                glyph: None,
            },
            mapping: None,
            data: None,
        }
    }

    pub fn point_with_glyph(glyph: char) -> Self {
        Self {
            kind: GeomKind::Point {
                size: None,
                glyph: Some(glyph),
            },
            mapping: None,
            data: None,
        }
    }

    pub fn line() -> Self {
        Self {
            kind: GeomKind::Line { width: None },
            mapping: None,
            data: None,
        }
    }

    pub fn smooth() -> Self {
        Self {
            kind: GeomKind::Smooth {
                fit: None,
                se: false,
            },
            mapping: None,
            data: None,
        }
    }

    pub fn smooth_with_fit(fit: LinearFit) -> Self {
        Self {
            kind: GeomKind::Smooth {
                fit: Some(fit),
                se: false,
            },
            mapping: None,
            data: None,
        }
    }

    pub fn histogram(bins: usize) -> Self {
        Self {
            kind: GeomKind::Histogram { bins },
            mapping: None,
            data: None,
        }
    }

    pub fn boxplot() -> Self {
        Self {
            kind: GeomKind::Boxplot {
                stats: None,
                multi_stats: Vec::new(),
            },
            mapping: None,
            data: None,
        }
    }

    pub fn boxplot_with_stats(stats: FiveNumberSummary) -> Self {
        Self {
            kind: GeomKind::Boxplot {
                stats: Some(stats),
                multi_stats: Vec::new(),
            },
            mapping: None,
            data: None,
        }
    }

    pub fn boxplot_with_multi_stats(multi_stats: Vec<(String, FiveNumberSummary)>) -> Self {
        Self {
            kind: GeomKind::Boxplot {
                stats: None,
                multi_stats,
            },
            mapping: None,
            data: None,
        }
    }

    pub fn bar() -> Self {
        Self {
            kind: GeomKind::Bar,
            mapping: None,
            data: None,
        }
    }

    pub fn area() -> Self {
        Self {
            kind: GeomKind::Area { alpha: Some(0.3) },
            mapping: None,
            data: None,
        }
    }

    pub fn rug() -> Self {
        Self {
            kind: GeomKind::Rug,
            mapping: None,
            data: None,
        }
    }

    pub fn with_mapping(mut self, mapping: AestheticMap) -> Self {
        self.mapping = Some(mapping);
        self
    }

    pub fn with_data(mut self, data: LayerData) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_xy_data(mut self, x: Vec<f64>, y: Vec<f64>) -> Self {
        self.data = Some(LayerData::default().with_xy(x, y));
        self
    }

    /// Returns the effective X and Y data for this layer, falling back to the plot's global data.
    pub fn effective_xy<'a>(&'a self, plot: &'a PlotSpec) -> (&'a [f64], &'a [f64]) {
        if let Some(ref d) = self.data {
            if !d.x_values.is_empty() || !d.y_values.is_empty() {
                return (&d.x_values, &d.y_values);
            }
        }
        (&plot.x_data, &plot.y_data)
    }

    /// Returns the effective series list for this layer, falling back to the plot's global series.
    pub fn effective_series<'a>(&'a self, plot: &'a PlotSpec) -> &'a [DataSeries] {
        if let Some(ref d) = self.data {
            if !d.series.is_empty() {
                return &d.series;
            }
        }
        &plot.series
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlotTheme {
    #[default]
    Default,
    Minimal,
    Classic,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ThemeModifier {
    pub theme: Option<PlotTheme>,
    pub font_family: Option<String>,
}

impl ThemeModifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_theme(mut self, theme: PlotTheme) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn with_font(mut self, font: impl Into<String>) -> Self {
        self.font_family = Some(font.into());
        self
    }

    pub fn apply(&self, plot: &mut PlotSpec) {
        if let Some(t) = self.theme {
            plot.theme = t;
        }
        if let Some(ref f) = self.font_family {
            plot.font_family = Some(f.clone());
        }
    }
}

/// Scale modifier applied to a `PlotSpec` via `+` or pipeline functions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScaleModifier {
    Size { range: (f64, f64) },
    XLog10,
    YLog10,
    XSqrt,
    YSqrt,
}

impl ScaleModifier {
    pub fn apply(&self, plot: &mut PlotSpec) {
        match self {
            ScaleModifier::Size { range } => {
                plot.size_range = *range;
            }
            ScaleModifier::XLog10 => {
                plot.x_scale = ScaleTransform::Log10;
            }
            ScaleModifier::YLog10 => {
                plot.y_scale = ScaleTransform::Log10;
            }
            ScaleModifier::XSqrt => {
                plot.x_scale = ScaleTransform::Sqrt;
            }
            ScaleModifier::YSqrt => {
                plot.y_scale = ScaleTransform::Sqrt;
            }
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum MarkerShape {
    #[default]
    Circle,
    Triangle,
    Square,
    Cross,
    Diamond,
    InvertedTriangle,
}

impl MarkerShape {
    pub fn from_index(idx: usize) -> Self {
        match idx % 6 {
            0 => MarkerShape::Circle,
            1 => MarkerShape::Triangle,
            2 => MarkerShape::Square,
            3 => MarkerShape::Cross,
            4 => MarkerShape::Diamond,
            _ => MarkerShape::InvertedTriangle,
        }
    }

    pub fn to_vega_shape(self) -> &'static str {
        match self {
            MarkerShape::Circle => "circle",
            MarkerShape::Triangle => "triangle",
            MarkerShape::Square => "square",
            MarkerShape::Cross => "cross",
            MarkerShape::Diamond => "diamond",
            MarkerShape::InvertedTriangle => "triangle-down",
        }
    }

    pub fn to_unicode_char(self) -> char {
        match self {
            MarkerShape::Circle => '●',
            MarkerShape::Triangle => '▲',
            MarkerShape::Square => '■',
            MarkerShape::Cross => '+',
            MarkerShape::Diamond => '◆',
            MarkerShape::InvertedTriangle => '▼',
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlotLabels {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub color_label: Option<String>,
    pub size_label: Option<String>,
    pub shape_label: Option<String>,
    pub caption: Option<String>,
}

/// A grouped series of data points, used when aesthetic mapping includes `color`, `shape`, or `size`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DataSeries {
    pub group_name: Option<String>,
    pub color_hex: Option<String>,
    pub x_values: Vec<f64>,
    pub y_values: Vec<f64>,
    pub categories: Vec<String>,
    pub size_values: Vec<f64>,
    pub shape_values: Vec<String>,
}

/// Equal-width binning for histograms.
#[derive(Debug, Clone, PartialEq)]
pub struct HistogramBins {
    pub min_x: f64,
    pub max_x: f64,
    pub bin_width: f64,
    pub counts: Vec<u32>,
    pub max_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompositePlot {
    Horizontal(Box<PlotSpec>, Box<PlotSpec>),
    Vertical(Box<PlotSpec>, Box<PlotSpec>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlotSpec {
    pub mapping: Option<AestheticMap>,
    pub layers: Vec<GeomLayer>,
    pub labels: PlotLabels,
    pub x_data: Vec<f64>,
    pub y_data: Vec<f64>,
    pub categories: Vec<String>,
    pub size_data: Vec<f64>,
    pub shape_data: Vec<String>,
    pub size_range: (f64, f64),
    pub series: Vec<DataSeries>,
    pub x_scale: ScaleTransform,
    pub y_scale: ScaleTransform,
    pub x_limits: Option<(f64, f64)>,
    pub y_limits: Option<(f64, f64)>,
    pub facet: Option<FacetSpec>,
    pub facet_data: Vec<String>,
    pub facet_row_data: Vec<String>,
    pub columns_cache: BTreeMap<String, Vec<String>>,
    pub composite: Option<Box<CompositePlot>>,
    pub font_family: Option<String>,
    pub width: usize,
    pub height: usize,
    pub theme: PlotTheme,
}

impl Default for PlotSpec {
    fn default() -> Self {
        Self::new()
    }
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
            size_data: Vec::new(),
            shape_data: Vec::new(),
            size_range: (3.0, 10.0),
            series: Vec::new(),
            x_scale: ScaleTransform::Linear,
            y_scale: ScaleTransform::Linear,
            x_limits: None,
            y_limits: None,
            facet: None,
            facet_data: Vec::new(),
            facet_row_data: Vec::new(),
            columns_cache: BTreeMap::new(),
            composite: None,
            font_family: None,
            width: 58,
            height: 12,
            theme: PlotTheme::Default,
        }
    }

    /// Compose two plots horizontally side-by-side (`p1 | p2`).
    pub fn beside(self, other: PlotSpec) -> Self {
        let mut parent = PlotSpec::new();
        parent.composite = Some(Box::new(CompositePlot::Horizontal(
            Box::new(self),
            Box::new(other),
        )));
        parent
    }

    /// Compose two plots vertically stacked (`p1 / p2`).
    pub fn stack(self, other: PlotSpec) -> Self {
        let mut parent = PlotSpec::new();
        parent.composite = Some(Box::new(CompositePlot::Vertical(
            Box::new(self),
            Box::new(other),
        )));
        parent
    }

    /// Set font family for titles, axis text, and labels.
    pub fn with_font(mut self, font: impl Into<String>) -> Self {
        self.font_family = Some(font.into());
        self
    }

    pub fn with_size_data(mut self, size_data: Vec<f64>) -> Self {
        self.size_data = size_data;
        self
    }

    pub fn with_shape_data(mut self, shape_data: Vec<String>) -> Self {
        self.shape_data = shape_data;
        self
    }

    pub fn with_size_range(mut self, min: f64, max: f64) -> Self {
        self.size_range = (min, max);
        self
    }

    /// Scale a continuous raw value into a pixel radius according to `self.size_range`.
    pub fn scale_size(&self, val: f64) -> f64 {
        let (min_r, max_r) = self.size_range;
        if self.size_data.is_empty() {
            return (min_r + max_r) / 2.0;
        }
        let mut min_val = f64::INFINITY;
        let mut max_val = f64::NEG_INFINITY;
        for &v in &self.size_data {
            if v < min_val {
                min_val = v;
            }
            if v > max_val {
                max_val = v;
            }
        }
        if (max_val - min_val).abs() < 1e-9 {
            return (min_r + max_r) / 2.0;
        }
        let norm = ((val - min_val) / (max_val - min_val)).clamp(0.0, 1.0);
        min_r + norm * (max_r - min_r)
    }

    /// Return deterministic unique shape levels.
    pub fn shape_levels(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        let mut levels = Vec::new();
        for s in &self.shape_data {
            if seen.insert(s.clone()) {
                levels.push(s.clone());
            }
        }
        levels
    }

    /// Map a discrete category to a `MarkerShape`.
    pub fn map_shape(&self, val: &str) -> MarkerShape {
        let levels = self.shape_levels();
        if let Some(pos) = levels.iter().position(|l| l == val) {
            MarkerShape::from_index(pos)
        } else {
            MarkerShape::Circle
        }
    }

    pub fn with_facet(mut self, facet: FacetSpec) -> Self {
        self.facet = Some(facet);
        self
    }

    pub fn with_facet_data(mut self, data: Vec<String>) -> Self {
        self.facet_data = data;
        self
    }

    pub fn with_facet_row_data(mut self, data: Vec<String>) -> Self {
        self.facet_row_data = data;
        self
    }

    pub fn with_column_cache(mut self, name: impl Into<String>, data: Vec<String>) -> Self {
        self.columns_cache.insert(name.into(), data);
        self
    }

    pub fn with_limits(
        mut self,
        x_limits: Option<(f64, f64)>,
        y_limits: Option<(f64, f64)>,
    ) -> Self {
        self.x_limits = x_limits;
        self.y_limits = y_limits;
        self
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

    pub fn with_labels(
        mut self,
        title: Option<String>,
        x: Option<String>,
        y: Option<String>,
    ) -> Self {
        self.labels.title = title;
        self.labels.x_label = x;
        self.labels.y_label = y;
        self
    }

    pub fn with_full_labels(mut self, labels: PlotLabels) -> Self {
        self.labels = labels;
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

    pub fn with_series(mut self, series: Vec<DataSeries>) -> Self {
        self.series = series;
        self
    }

    pub fn scale_x_log10(mut self) -> Self {
        self.x_scale = ScaleTransform::Log10;
        self
    }

    pub fn scale_y_log10(mut self) -> Self {
        self.y_scale = ScaleTransform::Log10;
        self
    }

    pub fn scale_x_sqrt(mut self) -> Self {
        self.x_scale = ScaleTransform::Sqrt;
        self
    }

    pub fn scale_y_sqrt(mut self) -> Self {
        self.y_scale = ScaleTransform::Sqrt;
        self
    }

    /// Retrieve all data series. If `self.series` is defined, returns it;
    /// otherwise constructs a single synthetic series from `x_data` and `y_data`.
    pub fn all_series(&self) -> Vec<DataSeries> {
        if !self.series.is_empty() {
            self.series.clone()
        } else if !self.x_data.is_empty() {
            vec![DataSeries {
                group_name: None,
                color_hex: None,
                x_values: self.x_data.clone(),
                y_values: self.y_data.clone(),
                categories: self.categories.clone(),
                size_values: self.size_data.clone(),
                shape_values: self.shape_data.clone(),
            }]
        } else {
            Vec::new()
        }
    }

    /// Look up the linear regression fit carried by this spec's `Smooth` layer, if any.
    pub fn smooth_fit(&self) -> Option<LinearFit> {
        self.layers.iter().find_map(|l| match &l.kind {
            GeomKind::Smooth { fit, .. } => *fit,
            _ => None,
        })
    }

    /// Look up the statistics carried by this spec's `Boxplot` layer, if any.
    pub fn boxplot_stats(&self) -> Option<FiveNumberSummary> {
        self.layers.iter().find_map(|l| match &l.kind {
            GeomKind::Boxplot { stats, .. } => stats.clone(),
            _ => None,
        })
    }

    /// Look up multi-category boxplot statistics, if any.
    pub fn boxplot_multi_stats(&self) -> Vec<(String, FiveNumberSummary)> {
        self.layers
            .iter()
            .find_map(|l| match &l.kind {
                GeomKind::Boxplot { multi_stats, .. } if !multi_stats.is_empty() => {
                    Some(multi_stats.clone())
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Number of histogram bins requested via the `Histogram` layer, or 8 by default.
    pub fn requested_bins(&self) -> usize {
        self.layers
            .iter()
            .find_map(|l| match l.kind {
                GeomKind::Histogram { bins } => Some(bins),
                _ => None,
            })
            .unwrap_or(8)
    }

    /// Bin `self.x_data` into `bins_count` equal-width buckets.
    pub fn histogram_bins(&self, bins_count: usize) -> Option<HistogramBins> {
        if self.x_data.is_empty() {
            return None;
        }

        let min_x = self.x_data.iter().copied().fold(f64::INFINITY, f64::min);
        let max_x = self
            .x_data
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let span = if (max_x - min_x).abs() < 1e-9 {
            1.0
        } else {
            max_x - min_x
        };
        let bin_width = span / bins_count as f64;

        let mut counts = vec![0u32; bins_count];
        for &x in &self.x_data {
            let idx = (((x - min_x) / span) * bins_count as f64).floor() as usize;
            let idx = idx.min(bins_count - 1);
            counts[idx] += 1;
        }
        let max_count = *counts.iter().max().unwrap_or(&1);

        Some(HistogramBins {
            min_x,
            max_x,
            bin_width,
            counts,
            max_count,
        })
    }

    /// Category -> frequency counts for a bar chart.
    pub fn bar_counts(&self) -> Option<BTreeMap<String, usize>> {
        let cats: Vec<String> = if !self.categories.is_empty() {
            self.categories.clone()
        } else if !self.x_data.is_empty() {
            self.x_data.iter().map(|x| format!("{x}")).collect()
        } else {
            return None;
        };

        let mut counts_map: BTreeMap<String, usize> = BTreeMap::new();
        for cat in &cats {
            *counts_map.entry(cat.clone()).or_insert(0) += 1;
        }
        Some(counts_map)
    }

    /// Partition this plot specification into faceted sub-panels according to `self.facet`.
    /// Returns the list of panels together with the grid geometry `(total_rows, total_cols)`.
    pub fn partition_facets(&self) -> (Vec<FacetPanel>, usize, usize) {
        let facet = match &self.facet {
            Some(f) => f,
            None => {
                return (
                    vec![FacetPanel {
                        label: String::new(),
                        row_label: None,
                        col_label: None,
                        row_idx: 0,
                        col_idx: 0,
                        spec: self.clone(),
                    }],
                    1,
                    1,
                );
            }
        };

        // Compute global data limits for fixed scale sharing across all panels
        let global_min_x = self.x_data.iter().copied().fold(f64::INFINITY, f64::min);
        let global_max_x = self
            .x_data
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let global_min_y = self.y_data.iter().copied().fold(f64::INFINITY, f64::min);
        let global_max_y = self
            .y_data
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);

        let global_x_limits = if global_min_x.is_finite() && global_max_x.is_finite() {
            Some((global_min_x, global_max_x))
        } else {
            None
        };
        let global_y_limits = if global_min_y.is_finite() && global_max_y.is_finite() {
            Some((global_min_y, global_max_y))
        } else {
            None
        };

        let n_data = self
            .x_data
            .len()
            .max(self.y_data.len())
            .max(self.categories.len());

        match &facet.layout {
            FacetLayout::Wrap {
                variable,
                ncol,
                nrow,
                scales,
            } => {
                let facet_vals: &[String] = if !self.facet_data.is_empty() {
                    &self.facet_data
                } else if let Some(vals) = self.columns_cache.get(variable) {
                    vals.as_slice()
                } else {
                    &[]
                };

                let mut unique_levels: Vec<String> = Vec::new();
                for val in facet_vals {
                    if !unique_levels.contains(val) {
                        unique_levels.push(val.clone());
                    }
                }

                if unique_levels.is_empty() {
                    return (
                        vec![FacetPanel {
                            label: String::new(),
                            row_label: None,
                            col_label: None,
                            row_idx: 0,
                            col_idx: 0,
                            spec: self.clone(),
                        }],
                        1,
                        1,
                    );
                }

                let total_panels = unique_levels.len();
                let cols = match ncol {
                    Some(c) if *c > 0 => *c,
                    _ => match nrow {
                        Some(r) if *r > 0 => total_panels.div_ceil(*r),
                        _ => (total_panels as f64).sqrt().ceil() as usize,
                    },
                }
                .max(1);
                let rows = total_panels.div_ceil(cols);

                let mut panels = Vec::new();
                for (panel_idx, level) in unique_levels.into_iter().enumerate() {
                    let r = panel_idx / cols;
                    let c = panel_idx % cols;

                    let indices: Vec<usize> = (0..n_data)
                        .filter(|&i| facet_vals.get(i) == Some(&level))
                        .collect();

                    let sub_x: Vec<f64> = indices
                        .iter()
                        .filter_map(|&i| self.x_data.get(i).copied())
                        .collect();
                    let sub_y: Vec<f64> = indices
                        .iter()
                        .filter_map(|&i| self.y_data.get(i).copied())
                        .collect();
                    let sub_cats: Vec<String> = indices
                        .iter()
                        .filter_map(|&i| self.categories.get(i).cloned())
                        .collect();
                    let sub_sizes: Vec<f64> = indices
                        .iter()
                        .filter_map(|&i| self.size_data.get(i).copied())
                        .collect();
                    let sub_shapes: Vec<String> = indices
                        .iter()
                        .filter_map(|&i| self.shape_data.get(i).cloned())
                        .collect();

                    // Reconstruct series if color aesthetic is present
                    let mut sub_series = Vec::new();
                    if let Some(color_col_name) =
                        self.mapping.as_ref().and_then(|m| m.color.as_ref())
                    {
                        if let Some(color_vals) = self.columns_cache.get(color_col_name) {
                            let mut groups: BTreeMap<
                                String,
                                (Vec<f64>, Vec<f64>, Vec<f64>, Vec<String>),
                            > = BTreeMap::new();
                            for &i in &indices {
                                if let (Some(&x), Some(&y), Some(g_key)) =
                                    (self.x_data.get(i), self.y_data.get(i), color_vals.get(i))
                                {
                                    let entry = groups.entry(g_key.clone()).or_insert_with(|| {
                                        (Vec::new(), Vec::new(), Vec::new(), Vec::new())
                                    });
                                    entry.0.push(x);
                                    entry.1.push(y);
                                    if let Some(&sz) = self.size_data.get(i) {
                                        entry.2.push(sz);
                                    }
                                    if let Some(sh) = self.shape_data.get(i) {
                                        entry.3.push(sh.clone());
                                    }
                                }
                            }
                            for (g_name, (g_xs, g_ys, g_sizes, g_shapes)) in groups {
                                sub_series.push(DataSeries {
                                    group_name: Some(g_name),
                                    color_hex: None,
                                    x_values: g_xs,
                                    y_values: g_ys,
                                    categories: Vec::new(),
                                    size_values: g_sizes,
                                    shape_values: g_shapes,
                                });
                            }
                        }
                    }

                    let mut sub_spec = self.clone();
                    sub_spec.facet = None;
                    sub_spec.x_data = sub_x;
                    sub_spec.y_data = sub_y;
                    sub_spec.categories = sub_cats;
                    sub_spec.size_data = sub_sizes;
                    sub_spec.shape_data = sub_shapes;
                    if !sub_series.is_empty() {
                        sub_spec.series = sub_series;
                    }

                    match scales {
                        FacetScales::Fixed => {
                            sub_spec.x_limits = global_x_limits;
                            sub_spec.y_limits = global_y_limits;
                        }
                        FacetScales::FreeX => {
                            sub_spec.x_limits = None;
                            sub_spec.y_limits = global_y_limits;
                        }
                        FacetScales::FreeY => {
                            sub_spec.x_limits = global_x_limits;
                            sub_spec.y_limits = None;
                        }
                        FacetScales::Free => {
                            sub_spec.x_limits = None;
                            sub_spec.y_limits = None;
                        }
                    }

                    sub_spec.recompute_layer_statistics();

                    panels.push(FacetPanel {
                        label: level.clone(),
                        row_label: None,
                        col_label: Some(level),
                        row_idx: r,
                        col_idx: c,
                        spec: sub_spec,
                    });
                }

                (panels, rows, cols)
            }
            FacetLayout::Grid {
                row_var,
                col_var,
                scales,
            } => {
                let row_vals_source: &[String] = if !self.facet_row_data.is_empty() {
                    &self.facet_row_data
                } else if let Some(var) = row_var {
                    self.columns_cache
                        .get(var)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[])
                } else {
                    &[]
                };

                let col_vals_source: &[String] = if !self.facet_data.is_empty() {
                    &self.facet_data
                } else if let Some(var) = col_var {
                    self.columns_cache
                        .get(var)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[])
                } else {
                    &[]
                };

                let mut unique_rows: Vec<String> = Vec::new();
                if row_var.is_some() {
                    for v in row_vals_source {
                        if !unique_rows.contains(v) {
                            unique_rows.push(v.clone());
                        }
                    }
                }
                if unique_rows.is_empty() {
                    unique_rows.push(String::new());
                }

                let mut unique_cols: Vec<String> = Vec::new();
                if col_var.is_some() {
                    for v in col_vals_source {
                        if !unique_cols.contains(v) {
                            unique_cols.push(v.clone());
                        }
                    }
                }
                if unique_cols.is_empty() {
                    unique_cols.push(String::new());
                }

                let total_rows = unique_rows.len();
                let total_cols = unique_cols.len();

                let mut panels = Vec::new();
                for (r_idx, r_val) in unique_rows.iter().enumerate() {
                    for (c_idx, c_val) in unique_cols.iter().enumerate() {
                        let mut indices = Vec::new();
                        for i in 0..n_data {
                            let match_row = if row_var.is_some() {
                                row_vals_source.get(i) == Some(r_val)
                            } else {
                                true
                            };
                            let match_col = if col_var.is_some() {
                                col_vals_source.get(i) == Some(c_val)
                            } else {
                                true
                            };
                            if match_row && match_col {
                                indices.push(i);
                            }
                        }

                        let sub_x: Vec<f64> = indices
                            .iter()
                            .filter_map(|&i| self.x_data.get(i).copied())
                            .collect();
                        let sub_y: Vec<f64> = indices
                            .iter()
                            .filter_map(|&i| self.y_data.get(i).copied())
                            .collect();
                        let sub_cats: Vec<String> = indices
                            .iter()
                            .filter_map(|&i| self.categories.get(i).cloned())
                            .collect();
                        let sub_sizes: Vec<f64> = indices
                            .iter()
                            .filter_map(|&i| self.size_data.get(i).copied())
                            .collect();
                        let sub_shapes: Vec<String> = indices
                            .iter()
                            .filter_map(|&i| self.shape_data.get(i).cloned())
                            .collect();

                        let mut sub_series = Vec::new();
                        if let Some(color_col_name) =
                            self.mapping.as_ref().and_then(|m| m.color.as_ref())
                        {
                            if let Some(color_vals) = self.columns_cache.get(color_col_name) {
                                let mut groups: BTreeMap<
                                    String,
                                    (Vec<f64>, Vec<f64>, Vec<f64>, Vec<String>),
                                > = BTreeMap::new();
                                for &i in &indices {
                                    if let (Some(&x), Some(&y), Some(g_key)) =
                                        (self.x_data.get(i), self.y_data.get(i), color_vals.get(i))
                                    {
                                        let entry =
                                            groups.entry(g_key.clone()).or_insert_with(|| {
                                                (Vec::new(), Vec::new(), Vec::new(), Vec::new())
                                            });
                                        entry.0.push(x);
                                        entry.1.push(y);
                                        if let Some(&sz) = self.size_data.get(i) {
                                            entry.2.push(sz);
                                        }
                                        if let Some(sh) = self.shape_data.get(i) {
                                            entry.3.push(sh.clone());
                                        }
                                    }
                                }
                                for (g_name, (g_xs, g_ys, g_szs, g_shs)) in groups {
                                    sub_series.push(DataSeries {
                                        group_name: Some(g_name),
                                        color_hex: None,
                                        x_values: g_xs,
                                        y_values: g_ys,
                                        categories: Vec::new(),
                                        size_values: g_szs,
                                        shape_values: g_shs,
                                    });
                                }
                            }
                        }

                        let mut sub_spec = self.clone();
                        sub_spec.facet = None;
                        sub_spec.x_data = sub_x;
                        sub_spec.y_data = sub_y;
                        sub_spec.categories = sub_cats;
                        sub_spec.size_data = sub_sizes;
                        sub_spec.shape_data = sub_shapes;
                        if !sub_series.is_empty() {
                            sub_spec.series = sub_series;
                        }

                        match scales {
                            FacetScales::Fixed => {
                                sub_spec.x_limits = global_x_limits;
                                sub_spec.y_limits = global_y_limits;
                            }
                            FacetScales::FreeX => {
                                sub_spec.x_limits = None;
                                sub_spec.y_limits = global_y_limits;
                            }
                            FacetScales::FreeY => {
                                sub_spec.x_limits = global_x_limits;
                                sub_spec.y_limits = None;
                            }
                            FacetScales::Free => {
                                sub_spec.x_limits = None;
                                sub_spec.y_limits = None;
                            }
                        }

                        sub_spec.recompute_layer_statistics();

                        let label = match (row_var.is_some(), col_var.is_some()) {
                            (true, true) => format!("{r_val} | {c_val}"),
                            (true, false) => r_val.clone(),
                            (false, true) => c_val.clone(),
                            (false, false) => String::new(),
                        };

                        panels.push(FacetPanel {
                            label,
                            row_label: if row_var.is_some() {
                                Some(r_val.clone())
                            } else {
                                None
                            },
                            col_label: if col_var.is_some() {
                                Some(c_val.clone())
                            } else {
                                None
                            },
                            row_idx: r_idx,
                            col_idx: c_idx,
                            spec: sub_spec,
                        });
                    }
                }

                (panels, total_rows, total_cols)
            }
        }
    }

    /// Recompute statistical layers (OLS regression, Tukey boxplots) on local data.
    pub fn recompute_layer_statistics(&mut self) {
        for layer in &mut self.layers {
            match &mut layer.kind {
                GeomKind::Smooth { fit, .. } => {
                    *fit = compute_linear_fit(&self.x_data, &self.y_data);
                }
                GeomKind::Boxplot { stats, multi_stats } => {
                    if !self.categories.is_empty() && !self.y_data.is_empty() {
                        let n = self.categories.len().min(self.y_data.len());
                        let mut grouped: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                        for i in 0..n {
                            grouped
                                .entry(self.categories[i].clone())
                                .or_default()
                                .push(self.y_data[i]);
                        }
                        let mut new_multi = Vec::new();
                        for (cat, vals) in grouped {
                            if let Some(st) = compute_five_number_summary(&vals) {
                                new_multi.push((cat, st));
                            }
                        }
                        *multi_stats = new_multi;
                        *stats = None;
                    } else if !self.x_data.is_empty() {
                        *stats = compute_five_number_summary(&self.x_data);
                        multi_stats.clear();
                    }
                }
                _ => {}
            }
        }
    }
}

/// Compute ordinary least squares linear regression slope and intercept.
pub fn compute_linear_fit(xs: &[f64], ys: &[f64]) -> Option<LinearFit> {
    let n = xs.len().min(ys.len());
    if n < 2 {
        return None;
    }
    let valid: Vec<(f64, f64)> = xs[..n]
        .iter()
        .copied()
        .zip(ys[..n].iter().copied())
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();

    let n = valid.len() as f64;
    if n < 2.0 {
        return None;
    }

    let sum_x: f64 = valid.iter().map(|p| p.0).sum();
    let sum_y: f64 = valid.iter().map(|p| p.1).sum();
    let mean_x = sum_x / n;
    let mean_y = sum_y / n;

    let mut ss_xy = 0.0;
    let mut ss_xx = 0.0;
    for (x, y) in &valid {
        ss_xy += (x - mean_x) * (y - mean_y);
        ss_xx += (x - mean_x) * (x - mean_x);
    }

    if ss_xx.abs() < 1e-12 {
        return None;
    }

    let slope = ss_xy / ss_xx;
    let intercept = mean_y - slope * mean_x;
    Some(LinearFit { slope, intercept })
}

/// Compute Tukey five-number summary with 1.5*IQR fences and outliers.
pub fn compute_five_number_summary(vals: &[f64]) -> Option<FiveNumberSummary> {
    let mut clean: Vec<f64> = vals.iter().copied().filter(|v| v.is_finite()).collect();
    if clean.is_empty() {
        return None;
    }
    clean.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = clean.len();
    let min = clean[0];
    let max = clean[n - 1];
    let mean = clean.iter().sum::<f64>() / n as f64;

    let median = if n % 2 == 1 {
        clean[n / 2]
    } else {
        (clean[n / 2 - 1] + clean[n / 2]) / 2.0
    };

    let q1 = if n.is_multiple_of(4) {
        (clean[n / 4 - 1] + clean[n / 4]) / 2.0
    } else {
        clean[n / 4]
    };

    let q3 = if (3 * n).is_multiple_of(4) {
        (clean[3 * n / 4 - 1] + clean[3 * n / 4]) / 2.0
    } else {
        clean[3 * n / 4]
    };

    let iqr = q3 - q1;
    let lower_fence = q1 - 1.5 * iqr;
    let upper_fence = q3 + 1.5 * iqr;
    let outliers: Vec<f64> = clean
        .iter()
        .copied()
        .filter(|&x| x < lower_fence || x > upper_fence)
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
