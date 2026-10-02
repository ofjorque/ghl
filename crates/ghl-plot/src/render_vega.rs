//! Interactive Vega-Lite v5 specification JSON generator.
//!
//! Produces publication-grade, interactive visualizations compatible with
//! Positron, Jupyter notebooks, VSCode data viewers, and web browsers.

use serde_json::{json, Value};
use crate::palette::OKABE_ITO;
use crate::spec::{GeomKind, PlotSpec, PlotTheme, ScaleTransform};

pub struct VegaRenderer;

impl VegaRenderer {
    /// Generate a valid Vega-Lite v5 JSON string from a `PlotSpec`.
    pub fn to_vega_json(spec: &PlotSpec) -> Result<String, String> {
        let val = Self::to_vega_value(spec)?;
        serde_json::to_string_pretty(&val).map_err(|e| format!("Vega JSON serialization error: {e}"))
    }

    /// Generate a `serde_json::Value` tree representing the Vega-Lite v5 spec.
    pub fn to_vega_value(spec: &PlotSpec) -> Result<Value, String> {
        let is_hist = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Histogram { .. }));
        let is_box = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Boxplot { .. }));
        let is_bar = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Bar));

        let title = spec.labels.title.as_deref().unwrap_or("Plot");
        let x_title = spec.labels.x_label.as_deref().unwrap_or("x");
        let y_title = spec.labels.y_label.as_deref().unwrap_or("y");

        let palette_range: Vec<&str> = OKABE_ITO.iter().map(|c| c.hex).collect();

        // Theme configuration
        let mut config = json!({
            "view": { "stroke": null }
        });
        if spec.theme == PlotTheme::Dark {
            config["background"] = json!("#18181b");
            config["axis"] = json!({
                "gridColor": "#33333a",
                "domainColor": "#55555f",
                "labelColor": "#e4e4e7",
                "titleColor": "#f4f4f5"
            });
            config["title"] = json!({
                "color": "#f4f4f5"
            });
            config["legend"] = json!({
                "labelColor": "#e4e4e7",
                "titleColor": "#f4f4f5"
            });
        }

        if is_hist {
            let data_values: Vec<Value> = spec.x_data.iter().map(|&x| json!({ "x": x })).collect();
            let bins_count = spec.requested_bins().clamp(4, 40);

            return Ok(json!({
                "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
                "title": title,
                "width": 600,
                "height": 400,
                "data": { "values": data_values },
                "mark": { "type": "bar", "tooltip": true, "color": "#4F6EF2" },
                "encoding": {
                    "x": {
                        "field": "x",
                        "bin": { "maxbins": bins_count },
                        "type": "quantitative",
                        "title": x_title
                    },
                    "y": {
                        "aggregate": "count",
                        "type": "quantitative",
                        "title": "Count"
                    }
                },
                "config": config
            }));
        }

        if is_bar {
            let counts_map = spec.bar_counts().unwrap_or_default();
            let data_values: Vec<Value> = counts_map.into_iter().map(|(cat, count)| {
                json!({ "category": cat, "count": count })
            }).collect();

            return Ok(json!({
                "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
                "title": title,
                "width": 600,
                "height": 400,
                "data": { "values": data_values },
                "mark": { "type": "bar", "tooltip": true },
                "encoding": {
                    "x": { "field": "category", "type": "nominal", "title": x_title },
                    "y": { "field": "count", "type": "quantitative", "title": "Count" },
                    "color": {
                        "field": "category",
                        "type": "nominal",
                        "scale": { "range": palette_range },
                        "legend": null
                    }
                },
                "config": config
            }));
        }

        if is_box {
            let has_categories = !spec.categories.is_empty() && !spec.y_data.is_empty();
            let data_values: Vec<Value> = if has_categories {
                let n = spec.categories.len().min(spec.y_data.len());
                (0..n).map(|i| json!({ "category": spec.categories[i], "value": spec.y_data[i] })).collect()
            } else {
                spec.y_data.iter().map(|&y| json!({ "value": y })).collect()
            };

            let mut encoding = json!({
                "y": { "field": "value", "type": "quantitative", "title": y_title }
            });
            if has_categories {
                encoding["x"] = json!({ "field": "category", "type": "nominal", "title": x_title });
                encoding["color"] = json!({
                    "field": "category",
                    "type": "nominal",
                    "scale": { "range": palette_range },
                    "legend": null
                });
            }

            return Ok(json!({
                "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
                "title": title,
                "width": if has_categories { 550 } else { 400 },
                "height": 400,
                "data": { "values": data_values },
                "mark": { "type": "boxplot", "extent": 1.5, "color": "#56B4E9" },
                "encoding": encoding,
                "config": config
            }));
        }

        // Multi-layer 2D Cartesian (Points, Lines, Smooth)
        let series_list = spec.all_series();
        let mut data_values = Vec::new();
        let has_groups = series_list.iter().any(|s| s.group_name.is_some());

        for s in &series_list {
            let n = s.x_values.len().min(s.y_values.len());
            for i in 0..n {
                let mut row = json!({
                    "x": s.x_values[i],
                    "y": s.y_values[i]
                });
                if let Some(ref g) = s.group_name {
                    row["group"] = json!(g);
                }
                data_values.push(row);
            }
        }

        let mut x_scale = json!({});
        if spec.x_scale == ScaleTransform::Log10 {
            x_scale["type"] = json!("log");
        } else if spec.x_scale == ScaleTransform::Sqrt {
            x_scale["type"] = json!("sqrt");
        }

        let mut y_scale = json!({});
        if spec.y_scale == ScaleTransform::Log10 {
            y_scale["type"] = json!("log");
        } else if spec.y_scale == ScaleTransform::Sqrt {
            y_scale["type"] = json!("sqrt");
        }

        let mut base_encoding = json!({
            "x": {
                "field": "x",
                "type": "quantitative",
                "title": x_title,
                "scale": x_scale
            },
            "y": {
                "field": "y",
                "type": "quantitative",
                "title": y_title,
                "scale": y_scale
            }
        });

        if has_groups {
            base_encoding["color"] = json!({
                "field": "group",
                "type": "nominal",
                "title": spec.labels.color_label.as_deref().unwrap_or("Group"),
                "scale": { "range": palette_range }
            });
        }

        let mut layers = Vec::new();

        let has_points = spec.layers.is_empty() || spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Point { .. }));
        let has_lines = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Line { .. }));
        let has_smooth = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Smooth { .. }));

        if has_points {
            layers.push(json!({
                "mark": { "type": "circle", "size": 60, "tooltip": true },
                "encoding": base_encoding
            }));
        }

        if has_lines {
            layers.push(json!({
                "mark": { "type": "line", "strokeWidth": 2 },
                "encoding": base_encoding
            }));
        }

        if has_smooth {
            // Vega regression transform
            let smooth_layer = json!({
                "mark": { "type": "line", "color": "#D55E00", "strokeWidth": 3 },
                "transform": [
                    { "regression": "y", "on": "x" }
                ],
                "encoding": {
                    "x": { "field": "x", "type": "quantitative" },
                    "y": { "field": "y", "type": "quantitative" }
                }
            });
            layers.push(smooth_layer);
        }

        Ok(json!({
            "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
            "title": title,
            "width": 600,
            "height": 400,
            "data": { "values": data_values },
            "layer": layers,
            "config": config
        }))
    }
}
