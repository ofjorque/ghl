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

/// A simple linear regression fit (slope + intercept).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinearFit {
    pub slope: f64,
    pub intercept: f64,
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
    Point { size: Option<f64>, glyph: Option<char> },
    Line { width: Option<u32> },
    Smooth { fit: Option<LinearFit>, se: bool },
    Histogram { bins: usize },
    Boxplot { stats: Option<FiveNumberSummary> },
    Bar,
    Area { alpha: Option<f64> },
    Rug,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeomLayer {
    pub kind: GeomKind,
    pub mapping: Option<AestheticMap>,
}

impl GeomLayer {
    pub fn point() -> Self {
        Self {
            kind: GeomKind::Point { size: None, glyph: None },
            mapping: None,
        }
    }

    pub fn point_with_glyph(glyph: char) -> Self {
        Self {
            kind: GeomKind::Point { size: None, glyph: Some(glyph) },
            mapping: None,
        }
    }

    pub fn line() -> Self {
        Self {
            kind: GeomKind::Line { width: None },
            mapping: None,
        }
    }

    pub fn smooth() -> Self {
        Self {
            kind: GeomKind::Smooth { fit: None, se: false },
            mapping: None,
        }
    }

    pub fn smooth_with_fit(fit: LinearFit) -> Self {
        Self {
            kind: GeomKind::Smooth { fit: Some(fit), se: false },
            mapping: None,
        }
    }

    pub fn histogram(bins: usize) -> Self {
        Self {
            kind: GeomKind::Histogram { bins },
            mapping: None,
        }
    }

    pub fn boxplot() -> Self {
        Self {
            kind: GeomKind::Boxplot { stats: None },
            mapping: None,
        }
    }

    pub fn boxplot_with_stats(stats: FiveNumberSummary) -> Self {
        Self {
            kind: GeomKind::Boxplot { stats: Some(stats) },
            mapping: None,
        }
    }

    pub fn bar() -> Self {
        Self {
            kind: GeomKind::Bar,
            mapping: None,
        }
    }

    pub fn area() -> Self {
        Self {
            kind: GeomKind::Area { alpha: Some(0.3) },
            mapping: None,
        }
    }

    pub fn rug() -> Self {
        Self {
            kind: GeomKind::Rug,
            mapping: None,
        }
    }

    pub fn with_mapping(mut self, mapping: AestheticMap) -> Self {
        self.mapping = Some(mapping);
        self
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlotLabels {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub color_label: Option<String>,
    pub caption: Option<String>,
}

/// A grouped series of data points, used when aesthetic mapping includes `color`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DataSeries {
    pub group_name: Option<String>,
    pub color_hex: Option<String>,
    pub x_values: Vec<f64>,
    pub y_values: Vec<f64>,
    pub categories: Vec<String>,
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
pub struct PlotSpec {
    pub mapping: Option<AestheticMap>,
    pub layers: Vec<GeomLayer>,
    pub labels: PlotLabels,
    pub x_data: Vec<f64>,
    pub y_data: Vec<f64>,
    pub categories: Vec<String>,
    pub series: Vec<DataSeries>,
    pub x_scale: ScaleTransform,
    pub y_scale: ScaleTransform,
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
            series: Vec::new(),
            x_scale: ScaleTransform::Linear,
            y_scale: ScaleTransform::Linear,
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
            GeomKind::Boxplot { stats } => stats.clone(),
            _ => None,
        })
    }

    /// Number of histogram bins requested via the `Histogram` layer, or 8 by default.
    pub fn requested_bins(&self) -> usize {
        self.layers.iter().find_map(|l| match l.kind {
            GeomKind::Histogram { bins } => Some(bins),
            _ => None,
        }).unwrap_or(8)
    }

    /// Bin `self.x_data` into `bins_count` equal-width buckets.
    pub fn histogram_bins(&self, bins_count: usize) -> Option<HistogramBins> {
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
}
