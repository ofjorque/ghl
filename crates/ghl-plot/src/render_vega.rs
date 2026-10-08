//! Interactive Vega-Lite v5 specification JSON generator.
//!
//! Produces publication-grade, interactive visualizations compatible with
//! Positron, Jupyter notebooks, VSCode data viewers, and web browsers.

use crate::palette::OKABE_ITO;
use crate::spec::{GeomKind, PlotSpec, PlotTheme, ScaleTransform};
use serde_json::{Value, json};

pub struct VegaRenderer;

impl VegaRenderer {
    /// Generate a valid Vega-Lite v5 JSON string from a `PlotSpec`.
    pub fn to_vega_json(spec: &PlotSpec) -> Result<String, String> {
        let val = Self::to_vega_value(spec)?;
        serde_json::to_string_pretty(&val)
            .map_err(|e| format!("Vega JSON serialization error: {e}"))
    }

    /// Generate a `serde_json::Value` tree representing the Vega-Lite v5 spec.
    pub fn to_vega_value(spec: &PlotSpec) -> Result<Value, String> {
        if let Some(ref comp) = spec.composite {
            match comp.as_ref() {
                crate::spec::CompositePlot::Horizontal(left, right) => {
                    let mut l = (**left).clone();
                    let mut r = (**right).clone();
                    if l.font_family.is_none() {
                        l.font_family = spec.font_family.clone();
                    }
                    if r.font_family.is_none() {
                        r.font_family = spec.font_family.clone();
                    }
                    let mut l_val = Self::to_vega_value(&l)?;
                    let mut r_val = Self::to_vega_value(&r)?;
                    if let Some(obj) = l_val.as_object_mut() {
                        obj.remove("$schema");
                    }
                    if let Some(obj) = r_val.as_object_mut() {
                        obj.remove("$schema");
                    }
                    let mut out = json!({
                        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
                        "hconcat": [l_val, r_val]
                    });
                    if let Some(ref font) = spec.font_family {
                        out["config"] = json!({ "font": font });
                    }
                    return Ok(out);
                }
                crate::spec::CompositePlot::Vertical(top, bottom) => {
                    let mut t = (**top).clone();
                    let mut b = (**bottom).clone();
                    if t.font_family.is_none() {
                        t.font_family = spec.font_family.clone();
                    }
                    if b.font_family.is_none() {
                        b.font_family = spec.font_family.clone();
                    }
                    let mut t_val = Self::to_vega_value(&t)?;
                    let mut b_val = Self::to_vega_value(&b)?;
                    if let Some(obj) = t_val.as_object_mut() {
                        obj.remove("$schema");
                    }
                    if let Some(obj) = b_val.as_object_mut() {
                        obj.remove("$schema");
                    }
                    let mut out = json!({
                        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
                        "vconcat": [t_val, b_val]
                    });
                    if let Some(ref font) = spec.font_family {
                        out["config"] = json!({ "font": font });
                    }
                    return Ok(out);
                }
            }
        }

        let is_hist = spec
            .layers
            .iter()
            .any(|l| matches!(l.kind, GeomKind::Histogram { .. }));
        let is_box = spec
            .layers
            .iter()
            .any(|l| matches!(l.kind, GeomKind::Boxplot { .. }));
        let is_bar = spec.layers.iter().any(|l| matches!(l.kind, GeomKind::Bar));

        let title = spec.labels.title.as_deref().unwrap_or("Plot");
        let x_title = spec.labels.x_label.as_deref().unwrap_or("x");
        let y_title = spec.labels.y_label.as_deref().unwrap_or("y");

        let palette_range: Vec<&str> = OKABE_ITO.iter().map(|c| c.hex).collect();

        // Theme configuration
        let mut config = json!({
            "view": { "stroke": null }
        });
        if let Some(ref font) = spec.font_family {
            config["font"] = json!(font);
        }
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
            let data_values: Vec<Value> = counts_map
                .into_iter()
                .map(|(cat, count)| json!({ "category": cat, "count": count }))
                .collect();

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
                (0..n)
                    .map(|i| json!({ "category": spec.categories[i], "value": spec.y_data[i] }))
                    .collect()
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
        let has_size =
            series_list.iter().any(|s| !s.size_values.is_empty()) || !spec.size_data.is_empty();
        let has_shape =
            series_list.iter().any(|s| !s.shape_values.is_empty()) || !spec.shape_data.is_empty();

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
                if let Some(&sz) = s.size_values.get(i) {
                    row["size"] = json!(sz);
                } else if let Some(&sz) = spec.size_data.get(i) {
                    row["size"] = json!(sz);
                }
                if let Some(sh) = s.shape_values.get(i) {
                    row["shape"] = json!(sh);
                } else if let Some(ref gn) = s.group_name {
                    row["shape"] = json!(gn);
                } else if let Some(sh) = spec.shape_data.get(i) {
                    row["shape"] = json!(sh);
                }
                for (col_name, col_vals) in &spec.columns_cache {
                    if let Some(val) = col_vals.get(i) {
                        row[col_name] = json!(val);
                    }
                }
                if let Some(f_val) = spec.facet_data.get(i) {
                    if let Some(ref facet_spec) = spec.facet {
                        match &facet_spec.layout {
                            crate::spec::FacetLayout::Wrap { variable, .. } => {
                                row[variable] = json!(f_val);
                            }
                            crate::spec::FacetLayout::Grid {
                                col_var: Some(c), ..
                            } => {
                                row[c] = json!(f_val);
                            }
                            _ => {}
                        }
                    }
                }
                if let Some(r_val) = spec.facet_row_data.get(i) {
                    if let Some(ref facet_spec) = spec.facet {
                        if let crate::spec::FacetLayout::Grid {
                            row_var: Some(r), ..
                        } = &facet_spec.layout
                        {
                            row[r] = json!(r_val);
                        }
                    }
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

        if has_size {
            let min_area =
                (spec.size_range.0 * spec.size_range.0 * std::f64::consts::PI).round() as u64;
            let max_area =
                (spec.size_range.1 * spec.size_range.1 * std::f64::consts::PI).round() as u64;
            base_encoding["size"] = json!({
                "field": "size",
                "type": "quantitative",
                "title": spec.labels.size_label.as_deref().unwrap_or("Size"),
                "scale": { "range": [min_area, max_area] }
            });
        }

        if has_shape {
            base_encoding["shape"] = json!({
                "field": "shape",
                "type": "nominal",
                "title": spec.labels.shape_label.as_deref().unwrap_or("Shape")
            });
        }

        let mut layers = Vec::new();

        let global_has_points = spec.layers.is_empty()
            || spec
                .layers
                .iter()
                .any(|l| l.data.is_none() && matches!(l.kind, GeomKind::Point { .. }));
        let global_has_lines = spec
            .layers
            .iter()
            .any(|l| l.data.is_none() && matches!(l.kind, GeomKind::Line { .. }));
        let global_has_smooth = spec
            .layers
            .iter()
            .any(|l| l.data.is_none() && matches!(l.kind, GeomKind::Smooth { .. }));

        if global_has_points && !data_values.is_empty() {
            let mut point_mark = json!({
                "type": "point",
                "filled": true,
                "tooltip": true
            });
            if !has_size {
                point_mark["size"] = json!(60);
            }
            layers.push(json!({
                "mark": point_mark,
                "encoding": base_encoding
            }));
        }

        if global_has_lines && !data_values.is_empty() {
            let mut line_encoding = base_encoding.clone();
            if let Some(obj) = line_encoding.as_object_mut() {
                obj.remove("shape");
                obj.remove("size");
            }
            layers.push(json!({
                "mark": { "type": "line", "strokeWidth": 2 },
                "encoding": line_encoding
            }));
        }

        if global_has_smooth && !data_values.is_empty() {
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

        // Add layers with local data
        let mut local_layer_idx = 0;
        for layer in &spec.layers {
            if let Some(ref d) = layer.data {
                let mut local_values = Vec::new();
                let n = d.x_values.len().min(d.y_values.len());
                for i in 0..n {
                    let mut row = json!({
                        "x": d.x_values[i],
                        "y": d.y_values[i]
                    });
                    for (col_name, col_vals) in &d.columns_cache {
                        if let Some(val) = col_vals.get(i) {
                            row[col_name] = json!(val);
                        }
                    }
                    local_values.push(row);
                }

                let color = OKABE_ITO[(local_layer_idx + 1) % OKABE_ITO.len()].hex;
                local_layer_idx += 1;

                match &layer.kind {
                    GeomKind::Point { .. } => {
                        layers.push(json!({
                            "data": { "values": local_values },
                            "mark": { "type": "point", "filled": true, "color": color, "size": 60, "tooltip": true },
                            "encoding": {
                                "x": { "field": "x", "type": "quantitative" },
                                "y": { "field": "y", "type": "quantitative" }
                            }
                        }));
                    }
                    GeomKind::Line { .. } => {
                        layers.push(json!({
                            "data": { "values": local_values },
                            "mark": { "type": "line", "color": color, "strokeWidth": 2 },
                            "encoding": {
                                "x": { "field": "x", "type": "quantitative" },
                                "y": { "field": "y", "type": "quantitative" }
                            }
                        }));
                    }
                    GeomKind::Area { .. } => {
                        layers.push(json!({
                            "data": { "values": local_values },
                            "mark": { "type": "area", "color": color, "opacity": 0.3 },
                            "encoding": {
                                "x": { "field": "x", "type": "quantitative" },
                                "y": { "field": "y", "type": "quantitative" }
                            }
                        }));
                    }
                    GeomKind::Smooth { .. } => {
                        layers.push(json!({
                            "data": { "values": local_values },
                            "mark": { "type": "line", "color": color, "strokeWidth": 3 },
                            "transform": [
                                { "regression": "y", "on": "x" }
                            ],
                            "encoding": {
                                "x": { "field": "x", "type": "quantitative" },
                                "y": { "field": "y", "type": "quantitative" }
                            }
                        }));
                    }
                    _ => {}
                }
            }
        }

        if let Some(ref facet_spec) = spec.facet {
            let mut facet_json = json!({
                "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
                "title": title,
                "data": { "values": data_values },
                "spec": {
                    "width": 240,
                    "height": 180,
                    "layer": layers,
                },
                "config": config
            });

            match &facet_spec.layout {
                crate::spec::FacetLayout::Wrap {
                    variable,
                    ncol,
                    scales,
                    ..
                } => {
                    let mut facet_enc = json!({
                        "field": variable,
                        "type": "nominal"
                    });
                    if let Some(c) = ncol {
                        facet_enc["columns"] = json!(c);
                    }
                    facet_json["facet"] = facet_enc;

                    match scales {
                        crate::spec::FacetScales::Free => {
                            facet_json["resolve"] =
                                json!({ "scale": { "x": "independent", "y": "independent" } });
                        }
                        crate::spec::FacetScales::FreeX => {
                            facet_json["resolve"] = json!({ "scale": { "x": "independent" } });
                        }
                        crate::spec::FacetScales::FreeY => {
                            facet_json["resolve"] = json!({ "scale": { "y": "independent" } });
                        }
                        crate::spec::FacetScales::Fixed => {}
                    }
                }
                crate::spec::FacetLayout::Grid {
                    row_var,
                    col_var,
                    scales,
                } => {
                    let mut facet_enc = json!({});
                    if let Some(r) = row_var {
                        facet_enc["row"] = json!({ "field": r, "type": "nominal" });
                    }
                    if let Some(c) = col_var {
                        facet_enc["column"] = json!({ "field": c, "type": "nominal" });
                    }
                    facet_json["facet"] = facet_enc;

                    match scales {
                        crate::spec::FacetScales::Free => {
                            facet_json["resolve"] =
                                json!({ "scale": { "x": "independent", "y": "independent" } });
                        }
                        crate::spec::FacetScales::FreeX => {
                            facet_json["resolve"] = json!({ "scale": { "x": "independent" } });
                        }
                        crate::spec::FacetScales::FreeY => {
                            facet_json["resolve"] = json!({ "scale": { "y": "independent" } });
                        }
                        crate::spec::FacetScales::Fixed => {}
                    }
                }
            }
            Ok(facet_json)
        } else {
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
}
