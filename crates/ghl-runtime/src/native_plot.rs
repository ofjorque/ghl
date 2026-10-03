//! Grammar of Graphics (`std::plot`, RFC 16) native functions.
//!
//! Grounded in GHL native types (F64, I64, Factor, String, Bool, Date).
//! Provides multi-layer composition, color grouping with Okabe-Ito palettes,
//! interactive Vega-Lite JSON export, and Positron Plots pane integration.

use std::collections::BTreeMap;
use ghl_diagnostics::{Diagnostic, RenderCaps};
use ghl_plot::{AestheticMap, DataSeries, GeomLayer, PlotSpec};
use ghl_types::ContrastScheme;
use polars_core::prelude::DataFrame;
use crate::na_reasons::NaReasonTable;
use crate::value::Value;

// =========================================================================
// Grammar of Graphics (std::plot - RFC 16) Native Functions
// =========================================================================

fn extract_raw_string(v: &Value) -> String {
    match v {
        Value::String(s) | Value::ColRef(s) => s.clone(),
        Value::I64(n) => n.to_string(),
        Value::F64(x) => x.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Factor { levels, indices, .. } => {
            indices.first().and_then(|&i| levels.get(i)).cloned().unwrap_or_default()
        }
        Value::NA(None) => "NA".to_string(),
        Value::NA(Some(r)) => format!("NA:{}", r),
        other => {
            let s = other.render_styled(&RenderCaps::ascii_plain(80));
            ghl_diagnostics::panel::strip_ansi(&s).trim_matches('"').to_string()
        }
    }
}

pub(crate) fn native_aes(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut x: Option<String> = None;
    let mut y: Option<String> = None;
    let mut color: Option<String> = None;
    let mut size: Option<String> = None;
    let mut shape: Option<String> = None;
    let mut facet_col: Option<String> = None;
    let mut facet_row: Option<String> = None;

    let mut pos_args = Vec::new();
    for arg in args {
        match arg {
            Value::NamedArg(name, val) => match name.as_str() {
                "x" => x = Some(extract_raw_string(&val)),
                "y" => y = Some(extract_raw_string(&val)),
                "color" | "colour" => color = Some(extract_raw_string(&val)),
                "size" => size = Some(extract_raw_string(&val)),
                "shape" => shape = Some(extract_raw_string(&val)),
                "facet_col" | "facet" => facet_col = Some(extract_raw_string(&val)),
                "facet_row" => facet_row = Some(extract_raw_string(&val)),
                _ => {}
            },
            other => pos_args.push(other),
        }
    }

    let mut pos_idx = 0;
    if x.is_none() && pos_idx < pos_args.len() {
        x = Some(extract_raw_string(&pos_args[pos_idx]));
        pos_idx += 1;
    }
    if y.is_none() && pos_idx < pos_args.len() {
        y = Some(extract_raw_string(&pos_args[pos_idx]));
        pos_idx += 1;
    }
    if color.is_none() && pos_idx < pos_args.len() {
        color = Some(extract_raw_string(&pos_args[pos_idx]));
    }

    let x_str = x.ok_or_else(|| {
        Diagnostic::compute_error("C0301", "`aes()` requires at least an `x` aesthetic (e.g. `aes(x = displ)` or `aes(displ, hwy)`)")
    })?;

    Ok(Value::Aesthetic(AestheticMap {
        x: x_str,
        y,
        color,
        size,
        shape,
        facet_col,
        facet_row,
    }))
}

pub(crate) fn native_plot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Ok(Value::Plot(Box::new(PlotSpec::new())));
    }

    let first = &args[0];
    match first {
        Value::DataFrame { frame, na_reasons } => {
            let mut plot_spec = PlotSpec::new();
            let col_values = |name: &str| -> Vec<Value> {
                crate::polars_bridge::pull_column_as_values(frame, na_reasons, name)
                    .unwrap_or_default()
            };
            let col_f64 = |name: &str| -> Vec<f64> {
                col_values(name).iter().filter_map(|v| v.as_f64()).collect()
            };

            // Pre-populate columns cache with all dataframe columns
            for col_name in frame.get_column_names() {
                let vals: Vec<String> = col_values(col_name).iter().map(extract_raw_string).collect();
                plot_spec.columns_cache.insert(col_name.to_string(), vals);
            }

            let aes_opt = if let Some(Value::Aesthetic(aes)) = args.get(1) {
                Some(aes.clone())
            } else if let Some(x_arg) = args.get(1) {
                let x_name = extract_raw_string(x_arg);
                let y_name = args.get(2).map(extract_raw_string);
                let mut map = AestheticMap::new(x_name);
                if let Some(y) = y_name {
                    map = map.with_y(y);
                }
                Some(map)
            } else {
                None
            };

            if let Some(aes) = aes_opt {
                plot_spec = plot_spec.with_mapping(aes.clone());
                let xs = col_f64(&aes.x);
                if xs.is_empty() {
                    let cats: Vec<String> = col_values(&aes.x).iter().map(extract_raw_string).collect();
                    plot_spec = plot_spec.with_categories(cats);
                } else {
                    plot_spec = plot_spec.with_x_data(xs.clone());
                }
                plot_spec.labels.x_label = Some(aes.x.clone());

                // Size aesthetic mapping
                let size_col: Vec<f64> = if let Some(ref size_name) = aes.size {
                    let s_vals = col_f64(size_name);
                    plot_spec.size_data = s_vals.clone();
                    plot_spec.labels.size_label = Some(size_name.clone());
                    s_vals
                } else {
                    Vec::new()
                };

                // Shape aesthetic mapping
                let shape_col: Vec<String> = if let Some(ref shape_name) = aes.shape {
                    let sh_vals: Vec<String> = col_values(shape_name).iter().map(extract_raw_string).collect();
                    plot_spec.shape_data = sh_vals.clone();
                    plot_spec.labels.shape_label = Some(shape_name.clone());
                    sh_vals
                } else {
                    Vec::new()
                };

                if let Some(ref y_name) = aes.y {
                    let ys = col_f64(y_name);
                    plot_spec.y_data = ys.clone();
                    plot_spec.labels.y_label = Some(y_name.clone());
                    plot_spec.labels.title = Some(format!("Plot: {} vs {}", y_name, aes.x));

                    // Group by color aesthetic if specified
                    if let Some(ref color_name) = aes.color {
                        let color_col = col_values(color_name);
                        let n = xs.len().min(ys.len()).min(color_col.len());
                        let mut groups: BTreeMap<String, (Vec<f64>, Vec<f64>, Vec<f64>, Vec<String>)> = BTreeMap::new();
                        for i in 0..n {
                            let g_key = extract_raw_string(&color_col[i]);
                            let entry = groups.entry(g_key).or_insert_with(|| (Vec::new(), Vec::new(), Vec::new(), Vec::new()));
                            entry.0.push(xs[i]);
                            entry.1.push(ys[i]);
                            if let Some(&sz) = size_col.get(i) {
                                entry.2.push(sz);
                            }
                            if let Some(sh) = shape_col.get(i) {
                                entry.3.push(sh.clone());
                            }
                        }
                        let mut series = Vec::new();
                        for (g_name, (g_xs, g_ys, g_szs, g_shs)) in groups {
                            series.push(DataSeries {
                                group_name: Some(g_name),
                                color_hex: None,
                                x_values: g_xs,
                                y_values: g_ys,
                                categories: Vec::new(),
                                size_values: g_szs,
                                shape_values: g_shs,
                            });
                        }
                        plot_spec = plot_spec.with_series(series);
                        plot_spec.labels.color_label = Some(color_name.clone());
                    } else if let Some(ref _shape_name) = aes.shape {
                        // If color is not specified but shape is, group by shape for legend entries
                        let n = xs.len().min(ys.len()).min(shape_col.len());
                        let mut groups: BTreeMap<String, (Vec<f64>, Vec<f64>, Vec<f64>, Vec<String>)> = BTreeMap::new();
                        for i in 0..n {
                            let g_key = shape_col[i].clone();
                            let entry = groups.entry(g_key.clone()).or_insert_with(|| (Vec::new(), Vec::new(), Vec::new(), Vec::new()));
                            entry.0.push(xs[i]);
                            entry.1.push(ys[i]);
                            if let Some(&sz) = size_col.get(i) {
                                entry.2.push(sz);
                            }
                            entry.3.push(g_key);
                        }
                        let mut series = Vec::new();
                        for (g_name, (g_xs, g_ys, g_szs, g_shs)) in groups {
                            series.push(DataSeries {
                                group_name: Some(g_name),
                                color_hex: None,
                                x_values: g_xs,
                                y_values: g_ys,
                                categories: Vec::new(),
                                size_values: g_szs,
                                shape_values: g_shs,
                            });
                        }
                        plot_spec = plot_spec.with_series(series);
                    }
                } else {
                    plot_spec.labels.title = Some(format!("Distribution of {}", aes.x));
                }

                // If aes specified facet_col or facet_row
                if let Some(ref fc) = aes.facet_col {
                    if let Some(col) = plot_spec.columns_cache.get(fc) {
                        plot_spec.facet_data = col.clone();
                        plot_spec.facet = Some(ghl_plot::FacetSpec::wrap(fc.clone(), None, None, ghl_plot::FacetScales::Fixed));
                    }
                }
                if let Some(ref fr) = aes.facet_row {
                    if let Some(col) = plot_spec.columns_cache.get(fr) {
                        plot_spec.facet_row_data = col.clone();
                    }
                }
            }
            Ok(Value::Plot(Box::new(plot_spec)))
        }
        Value::Vector(items) => {
            let mut plot_spec = PlotSpec::new();
            let xs: Vec<f64> = items.iter().filter_map(|v| v.as_f64()).collect();
            plot_spec = plot_spec.with_x_data(xs);

            if let Some(Value::Vector(y_items)) = args.get(1) {
                let ys: Vec<f64> = y_items.iter().filter_map(|v| v.as_f64()).collect();
                plot_spec.y_data = ys;
                plot_spec.labels.title = Some("Scatter Plot (Y vs X)".into());
            } else {
                plot_spec.labels.title = Some("Vector Distribution".into());
            }
            Ok(Value::Plot(Box::new(plot_spec)))
        }
        Value::Plot(p) => Ok(Value::Plot(p.clone())),
        other => Err(Diagnostic::compute_error(
            "C0302",
            format!("Cannot create plot from `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn extract_layer_data_from_df(
    frame: &DataFrame,
    na_reasons: &NaReasonTable,
    aes_opt: Option<&AestheticMap>,
) -> ghl_plot::LayerData {
    let mut layer_data = ghl_plot::LayerData::new();
    let col_values = |name: &str| -> Vec<Value> {
        crate::polars_bridge::pull_column_as_values(frame, na_reasons, name)
            .unwrap_or_default()
    };
    let col_f64 = |name: &str| -> Vec<f64> {
        col_values(name).iter().filter_map(|v| v.as_f64()).collect()
    };

    // Pre-populate columns cache with all dataframe columns
    for col_name in frame.get_column_names() {
        let vals: Vec<String> = col_values(col_name).iter().map(extract_raw_string).collect();
        layer_data.columns_cache.insert(col_name.to_string(), vals);
    }

    if let Some(aes) = aes_opt {
        let xs = col_f64(&aes.x);
        if xs.is_empty() {
            let cats: Vec<String> = col_values(&aes.x).iter().map(extract_raw_string).collect();
            layer_data.categories = cats;
        } else {
            layer_data.x_values = xs.clone();
        }

        let size_col: Vec<f64> = if let Some(ref size_name) = aes.size {
            let s_vals = col_f64(size_name);
            layer_data.size_values = s_vals.clone();
            s_vals
        } else {
            Vec::new()
        };

        let shape_col: Vec<String> = if let Some(ref shape_name) = aes.shape {
            let sh_vals: Vec<String> = col_values(shape_name).iter().map(extract_raw_string).collect();
            layer_data.shape_values = sh_vals.clone();
            sh_vals
        } else {
            Vec::new()
        };

        if let Some(ref y_name) = aes.y {
            let ys = col_f64(y_name);
            layer_data.y_values = ys.clone();

            if let Some(ref color_name) = aes.color {
                let color_col = col_values(color_name);
                let n = xs.len().min(ys.len()).min(color_col.len());
                let mut groups: BTreeMap<String, (Vec<f64>, Vec<f64>, Vec<f64>, Vec<String>)> = BTreeMap::new();
                for i in 0..n {
                    let g_key = extract_raw_string(&color_col[i]);
                    let entry = groups.entry(g_key).or_insert_with(|| (Vec::new(), Vec::new(), Vec::new(), Vec::new()));
                    entry.0.push(xs[i]);
                    entry.1.push(ys[i]);
                    if let Some(&sz) = size_col.get(i) {
                        entry.2.push(sz);
                    }
                    if let Some(sh) = shape_col.get(i) {
                        entry.3.push(sh.clone());
                    }
                }
                let mut series = Vec::new();
                for (g_name, (g_xs, g_ys, g_szs, g_shs)) in groups {
                    series.push(DataSeries {
                        group_name: Some(g_name),
                        color_hex: None,
                        x_values: g_xs,
                        y_values: g_ys,
                        categories: Vec::new(),
                        size_values: g_szs,
                        shape_values: g_shs,
                    });
                }
                layer_data.series = series;
            }
        }
    }

    layer_data
}

struct ParsedGeomConfig {
    plot: Option<PlotSpec>,
    layer_data: Option<ghl_plot::LayerData>,
    mapping: Option<AestheticMap>,
    extra_pos: Vec<Value>,
    extra_named: BTreeMap<String, Value>,
}

fn parse_geom_layer_args(args: Vec<Value>) -> ParsedGeomConfig {
    let mut plot = None;
    let mut df_val = None;
    let mut mapping = None;
    let mut extra_pos = Vec::new();
    let mut extra_named = BTreeMap::new();

    for arg in args {
        match arg {
            Value::Plot(p) => {
                plot = Some(*p);
            }
            Value::DataFrame { frame, na_reasons } => {
                df_val = Some((frame, na_reasons));
            }
            Value::Aesthetic(m) => {
                mapping = Some(m);
            }
            Value::NamedArg(name, val) => {
                match name.as_str() {
                    "data" => {
                        if let Value::DataFrame { frame, na_reasons } = *val {
                            df_val = Some((frame, na_reasons));
                        }
                    }
                    "mapping" | "aes" => {
                        if let Value::Aesthetic(m) = *val {
                            mapping = Some(m);
                        }
                    }
                    _ => {
                        extra_named.insert(name, *val);
                    }
                }
            }
            other => {
                extra_pos.push(other);
            }
        }
    }

    let layer_data = df_val.map(|(frame, na_reasons)| {
        extract_layer_data_from_df(&frame, &na_reasons, mapping.as_ref())
    });

    ParsedGeomConfig {
        plot,
        layer_data,
        mapping,
        extra_pos,
        extra_named,
    }
}

fn finalize_geom_layer(
    mut layer: GeomLayer,
    parsed: ParsedGeomConfig,
) -> Result<Value, Diagnostic> {
    layer.data = parsed.layer_data;
    layer.mapping = parsed.mapping;

    if let Some(mut p) = parsed.plot {
        // Resolve inheritance if piped from a plot
        if let Some(ref mut ld) = layer.data {
            if layer.mapping.is_none() {
                if let Some(ref p_map) = p.mapping {
                    ld.resolve_mapping(p_map);
                    layer.mapping = Some(p_map.clone());
                }
            }
        } else if let Some(ref map) = layer.mapping {
            let mut ld = ghl_plot::LayerData::new();
            ld.columns_cache = p.columns_cache.clone();
            ld.resolve_mapping(map);
            layer.data = Some(ld);
        }

        layer = match &layer.kind {
            ghl_plot::GeomKind::Smooth { fit: None, se } => {
                let (xs, ys) = layer.effective_xy(&p);
                match crate::plot_stats::simple_linear_fit(xs, ys) {
                    Some(fit) => GeomLayer {
                        kind: ghl_plot::GeomKind::Smooth { fit: Some(fit), se: *se },
                        mapping: layer.mapping.clone(),
                        data: layer.data.clone(),
                    },
                    None => layer,
                }
            }
            ghl_plot::GeomKind::Boxplot { stats: None, multi_stats } if multi_stats.is_empty() => {
                let (xs, ys) = layer.effective_xy(&p);
                if !p.categories.is_empty() && !ys.is_empty() {
                    let n = p.categories.len().min(ys.len());
                    let mut grouped: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                    for i in 0..n {
                        grouped.entry(p.categories[i].clone()).or_default().push(ys[i]);
                    }
                    let mut group_stats = Vec::new();
                    for (cat, vals) in grouped {
                        if let Some(st) = crate::plot_stats::five_number_summary(&vals) {
                            group_stats.push((cat, st));
                        }
                    }
                    if !group_stats.is_empty() {
                        GeomLayer {
                            kind: ghl_plot::GeomKind::Boxplot { stats: None, multi_stats: group_stats },
                            mapping: layer.mapping.clone(),
                            data: layer.data.clone(),
                        }
                    } else {
                        layer
                    }
                } else {
                    match crate::plot_stats::five_number_summary(xs) {
                        Some(stats) => GeomLayer {
                            kind: ghl_plot::GeomKind::Boxplot { stats: Some(stats), multi_stats: Vec::new() },
                            mapping: layer.mapping.clone(),
                            data: layer.data.clone(),
                        },
                        None => layer,
                    }
                }
            }
            _ => layer,
        };

        p = p.add_layer(layer);
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Geom(layer))
    }
}

pub(crate) fn native_geom_point(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let parsed = parse_geom_layer_args(args);
    let size = parsed.extra_named.get("size").and_then(|v| v.as_f64())
        .or_else(|| parsed.extra_pos.first().and_then(|v| v.as_f64()));
    let glyph = parsed.extra_named.get("glyph").and_then(|v| match v {
        Value::String(s) => s.chars().next(),
        _ => None,
    });
    let layer = match (size, glyph) {
        (s, g) if s.is_some() || g.is_some() => GeomLayer {
            kind: ghl_plot::GeomKind::Point { size: s, glyph: g },
            mapping: None,
            data: None,
        },
        _ => GeomLayer::point(),
    };
    finalize_geom_layer(layer, parsed)
}

pub(crate) fn native_geom_line(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let parsed = parse_geom_layer_args(args);
    let width = parsed.extra_named.get("width").and_then(|v| v.as_i64()).map(|w| w as u32)
        .or_else(|| parsed.extra_pos.first().and_then(|v| v.as_i64()).map(|w| w as u32));
    let layer = match width {
        Some(w) => GeomLayer {
            kind: ghl_plot::GeomKind::Line { width: Some(w) },
            mapping: None,
            data: None,
        },
        None => GeomLayer::line(),
    };
    finalize_geom_layer(layer, parsed)
}

pub(crate) fn native_geom_smooth(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let parsed = parse_geom_layer_args(args);
    let layer = GeomLayer::smooth();
    finalize_geom_layer(layer, parsed)
}

pub(crate) fn native_geom_histogram(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let parsed = parse_geom_layer_args(args);
    let bins = parsed.extra_named.get("bins").and_then(|v| v.as_i64())
        .or_else(|| parsed.extra_pos.first().and_then(|v| v.as_i64()))
        .unwrap_or(8) as usize;
    let layer = GeomLayer::histogram(bins);
    finalize_geom_layer(layer, parsed)
}

pub(crate) fn native_geom_boxplot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let parsed = parse_geom_layer_args(args);
    let layer = GeomLayer::boxplot();
    finalize_geom_layer(layer, parsed)
}

pub(crate) fn native_geom_bar(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let parsed = parse_geom_layer_args(args);
    let layer = GeomLayer::bar();
    finalize_geom_layer(layer, parsed)
}

pub(crate) fn native_geom_area(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let parsed = parse_geom_layer_args(args);
    let alpha = parsed.extra_named.get("alpha").and_then(|v| v.as_f64());
    let layer = GeomLayer {
        kind: ghl_plot::GeomKind::Area { alpha },
        mapping: None,
        data: None,
    };
    finalize_geom_layer(layer, parsed)
}

pub(crate) fn native_geom_rug(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let parsed = parse_geom_layer_args(args);
    let layer = GeomLayer::rug();
    finalize_geom_layer(layer, parsed)
}

pub(crate) fn native_labs(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut plot_opt: Option<PlotSpec> = None;
    let mut title: Option<String> = None;
    let mut subtitle: Option<String> = None;
    let mut x_label: Option<String> = None;
    let mut y_label: Option<String> = None;
    let mut color_label: Option<String> = None;
    let mut size_label: Option<String> = None;
    let mut shape_label: Option<String> = None;
    let mut caption: Option<String> = None;

    let mut pos_args = Vec::new();
    for arg in args {
        match arg {
            Value::Plot(p) => {
                plot_opt = Some(*p);
            }
            Value::NamedArg(name, val) => match name.as_str() {
                "title" => title = Some(extract_raw_string(&val)),
                "subtitle" => subtitle = Some(extract_raw_string(&val)),
                "x" | "x_label" | "xlab" => x_label = Some(extract_raw_string(&val)),
                "y" | "y_label" | "ylab" => y_label = Some(extract_raw_string(&val)),
                "color" | "colour" => color_label = Some(extract_raw_string(&val)),
                "size" => size_label = Some(extract_raw_string(&val)),
                "shape" => shape_label = Some(extract_raw_string(&val)),
                "caption" => caption = Some(extract_raw_string(&val)),
                _ => {}
            },
            other => pos_args.push(other),
        }
    }

    let mut pos_idx = 0;
    if title.is_none() && pos_idx < pos_args.len() {
        title = Some(extract_raw_string(&pos_args[pos_idx]));
        pos_idx += 1;
    }
    if x_label.is_none() && pos_idx < pos_args.len() {
        x_label = Some(extract_raw_string(&pos_args[pos_idx]));
        pos_idx += 1;
    }
    if y_label.is_none() && pos_idx < pos_args.len() {
        y_label = Some(extract_raw_string(&pos_args[pos_idx]));
    }

    if let Some(mut p) = plot_opt {
        if let Some(t) = title { p.labels.title = Some(t); }
        if let Some(s) = subtitle { p.labels.subtitle = Some(s); }
        if let Some(x) = x_label { p.labels.x_label = Some(x); }
        if let Some(y) = y_label { p.labels.y_label = Some(y); }
        if let Some(c) = color_label { p.labels.color_label = Some(c); }
        if let Some(sz) = size_label { p.labels.size_label = Some(sz); }
        if let Some(sh) = shape_label { p.labels.shape_label = Some(sh); }
        if let Some(cap) = caption { p.labels.caption = Some(cap); }
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Labels(ghl_plot::PlotLabels {
            title,
            subtitle,
            x_label,
            y_label,
            color_label,
            size_label,
            shape_label,
            caption,
        }))
    }
}

pub(crate) fn native_scale_x_log10(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            p = p.scale_x_log10();
            Ok(Value::Plot(Box::new(p)))
        }
        None => Ok(Value::Scale(ghl_plot::ScaleModifier::XLog10)),
        _ => Err(Diagnostic::compute_error("C0315", "`scale_x_log10()` requires a Plot as first argument")),
    }
}

pub(crate) fn native_scale_y_log10(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            p = p.scale_y_log10();
            Ok(Value::Plot(Box::new(p)))
        }
        None => Ok(Value::Scale(ghl_plot::ScaleModifier::YLog10)),
        _ => Err(Diagnostic::compute_error("C0315", "`scale_y_log10()` requires a Plot as first argument")),
    }
}

pub(crate) fn native_scale_size(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut plot_opt: Option<PlotSpec> = None;
    let mut min_size: Option<f64> = None;
    let mut max_size: Option<f64> = None;
    let mut pos_args = Vec::new();

    for arg in args {
        match arg {
            Value::Plot(p) => {
                plot_opt = Some(*p);
            }
            Value::NamedArg(name, val) => match name.as_str() {
                "range" => {
                    match val.as_ref() {
                        Value::Vector(v) => {
                            if let Some(r0) = v.iter().nth(0).and_then(|x| x.as_f64()) {
                                min_size = Some(r0);
                            }
                            if let Some(r1) = v.iter().nth(1).and_then(|x| x.as_f64()) {
                                max_size = Some(r1);
                            }
                        }
                        other => {
                            if let Some(n) = other.as_f64() {
                                min_size = Some(n);
                            }
                        }
                    }
                }
                "min" => min_size = val.as_f64(),
                "max" => max_size = val.as_f64(),
                _ => {}
            },
            other => pos_args.push(other),
        }
    }

    let mut pos_idx = 0;
    if min_size.is_none() && pos_idx < pos_args.len() {
        if let Value::Vector(v) = &pos_args[pos_idx] {
            if let Some(r0) = v.iter().nth(0).and_then(|x| x.as_f64()) {
                min_size = Some(r0);
            }
            if let Some(r1) = v.iter().nth(1).and_then(|x| x.as_f64()) {
                max_size = Some(r1);
            }
            pos_idx += 1;
        } else if let Some(n) = pos_args[pos_idx].as_f64() {
            min_size = Some(n);
            pos_idx += 1;
        }
    }
    if max_size.is_none() && pos_idx < pos_args.len() {
        if let Some(n) = pos_args[pos_idx].as_f64() {
            max_size = Some(n);
        }
    }

    let range = (min_size.unwrap_or(1.0), max_size.unwrap_or(6.0));

    if let Some(mut p) = plot_opt {
        p.size_range = range;
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Scale(ghl_plot::ScaleModifier::Size { range }))
    }
}

pub(crate) fn native_theme(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut plot_opt: Option<PlotSpec> = None;
    let mut font_family: Option<String> = None;
    let mut theme_style: Option<ghl_plot::PlotTheme> = None;

    let mut remaining = Vec::new();
    for arg in args {
        match arg {
            Value::Plot(p) => {
                plot_opt = Some(*p);
            }
            Value::NamedArg(name, val) => match name.as_str() {
                "font" | "font_family" | "family" => {
                    font_family = Some(extract_raw_string(&val));
                }
                "theme" | "style" => {
                    let s = extract_raw_string(&val).to_lowercase();
                    match s.as_str() {
                        "minimal" => theme_style = Some(ghl_plot::PlotTheme::Minimal),
                        "classic" => theme_style = Some(ghl_plot::PlotTheme::Classic),
                        "dark" => theme_style = Some(ghl_plot::PlotTheme::Dark),
                        "default" => theme_style = Some(ghl_plot::PlotTheme::Default),
                        _ => {}
                    }
                }
                _ => {}
            },
            other => remaining.push(other),
        }
    }

    if font_family.is_none() && !remaining.is_empty() {
        let s = extract_raw_string(&remaining[0]);
        if !s.is_empty() {
            match s.to_lowercase().as_str() {
                "minimal" => theme_style = Some(ghl_plot::PlotTheme::Minimal),
                "classic" => theme_style = Some(ghl_plot::PlotTheme::Classic),
                "dark" => theme_style = Some(ghl_plot::PlotTheme::Dark),
                "default" => theme_style = Some(ghl_plot::PlotTheme::Default),
                _ => font_family = Some(s),
            }
        }
    }

    if let Some(mut p) = plot_opt {
        if let Some(font) = font_family {
            p.font_family = Some(font);
        }
        if let Some(t) = theme_style {
            p.theme = t;
        }
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Theme(ghl_plot::ThemeModifier {
            theme: theme_style,
            font_family,
        }))
    }
}

pub(crate) fn native_theme_minimal(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if let Some(Value::Plot(plot)) = args.first() {
        let mut p = (**plot).clone();
        p = p.theme_minimal();
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Theme(ghl_plot::ThemeModifier {
            theme: Some(ghl_plot::PlotTheme::Minimal),
            font_family: None,
        }))
    }
}

pub(crate) fn native_theme_classic(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if let Some(Value::Plot(plot)) = args.first() {
        let mut p = (**plot).clone();
        p = p.theme_classic();
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Theme(ghl_plot::ThemeModifier {
            theme: Some(ghl_plot::PlotTheme::Classic),
            font_family: None,
        }))
    }
}

pub(crate) fn native_theme_dark(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if let Some(Value::Plot(plot)) = args.first() {
        let mut p = (**plot).clone();
        p = p.theme_dark();
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Theme(ghl_plot::ThemeModifier {
            theme: Some(ghl_plot::PlotTheme::Dark),
            font_family: None,
        }))
    }
}

pub(crate) fn native_beside(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0322",
            "`beside(p1, p2)` requires two Plot arguments (or `p1 |> beside(p2)`)"
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Plot(p1), Value::Plot(p2)) => {
            let comp = (**p1).clone().beside((**p2).clone());
            Ok(Value::Plot(Box::new(comp)))
        }
        _ => Err(Diagnostic::compute_error(
            "C0322",
            "`beside()` arguments must be Plots"
        )),
    }
}

pub(crate) fn native_stack(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0323",
            "`stack(p1, p2)` requires two Plot arguments (or `p1 |> stack(p2)`)"
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Plot(p1), Value::Plot(p2)) => {
            let comp = (**p1).clone().stack((**p2).clone());
            Ok(Value::Plot(Box::new(comp)))
        }
        _ => Err(Diagnostic::compute_error(
            "C0323",
            "`stack()` arguments must be Plots"
        )),
    }
}

pub(crate) fn native_factor(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`factor(x, [levels], [contrast])` requires at least 1 argument"));
    }
    let (items, given_levels) = match &args[0] {
        Value::Vector(v) => (v.clone(), None),
        Value::Factor { levels, indices, .. } => {
            let str_items: Vec<Value> = indices.iter().map(|&i| Value::String(levels[i].clone())).collect();
            (crate::vector_data::VectorData::from_values(str_items), Some(levels.clone()))
        }
        other => {
            return Err(Diagnostic::compute_error("C0201", format!("`factor()` argument must be a Vector, found `{}`", other.type_name())));
        }
    };

    let explicit_levels: Option<Vec<String>> = if let Some(l_arg) = args.get(1) {
        match l_arg {
            Value::Vector(v) => Some(v.iter().map(|val| match val {
                Value::String(s) => s.clone(),
                other => format!("{other}"),
            }).collect()),
            _ => None,
        }
    } else {
        given_levels
    };

    let levels: Vec<String> = if let Some(expl) = explicit_levels {
        expl
    } else {
        let mut set = std::collections::BTreeSet::new();
        for v in items.iter() {
            match v {
                Value::String(s) => { set.insert(s.clone()); }
                Value::NA(_) => {}
                other => { set.insert(format!("{other}")); }
            }
        }
        set.into_iter().collect()
    };

    let mut indices = Vec::with_capacity(items.len());
    for v in items.iter() {
        let s = match v {
            Value::String(s) => s.clone(),
            other => format!("{other}"),
        };
        let idx = levels.iter().position(|l| l == &s).unwrap_or(0);
        indices.push(idx);
    }

    let contrast = if let Some(c_arg) = args.get(2).and_then(|v| v.as_str()) {
        match c_arg.to_lowercase().as_str() {
            "sum" => ContrastScheme::Sum,
            "helmert" => ContrastScheme::Helmert,
            "poly" | "polynomial" => ContrastScheme::Polynomial,
            _ => ContrastScheme::Treatment,
        }
    } else {
        ContrastScheme::Treatment
    };

    Ok(Value::Factor {
        levels,
        indices,
        ordered: false,
        contrast,
    })
}

pub(crate) fn native_ordered_factor(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut factor_val = native_factor(args)?;
    if let Value::Factor { ordered, contrast, .. } = &mut factor_val {
        *ordered = true;
        if *contrast == ContrastScheme::Treatment {
            *contrast = ContrastScheme::Polynomial;
        }
    }
    Ok(factor_val)
}

pub(crate) fn native_levels(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`levels(x)` requires a Factor argument")
    })?;
    match target {
        Value::Factor { levels, .. } => {
            let vals = levels.iter().map(|s| Value::String(s.clone())).collect();
            Ok(Value::Vector(crate::vector_data::VectorData::from_values(vals)))
        }
        other => Err(Diagnostic::compute_error("C0201", format!("`levels()` expects a Factor, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_show(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0305", "`show()` requires a Plot or Value to render")
    })?;

    let caps = RenderCaps::detect();
    match target {
        Value::Plot(p) => {
            println!("{}\n", p.render(&caps));

            // Check if Positron or GHL plots directory is configured
            if let Some(plots_dir) = std::env::var("POSITRON_PLOTS_DIR")
                .ok()
                .or_else(|| std::env::var("GHL_PLOTS_DIR").ok())
            {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let plot_path = std::path::Path::new(&plots_dir).join(format!("ghl_plot_{}_{}.svg", std::process::id(), timestamp));
                if let Err(e) = p.save_file(&plot_path.to_string_lossy()) {
                    eprintln!("{} [Plots Pane] Warning: failed to save SVG plot: {e}", caps.gojo("/ᐠ ¬`‸´¬ マ"));
                }
            }
        }
        other => {
            println!("{}\n", other.render_styled(&caps));
        }
    }
    Ok(Value::Unit)
}

pub(crate) fn native_view(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`view()` requires a DataFrame: `view(df)` or `df |> view()`")
    })?;

    let (nrow, ncol) = match df_val {
        Value::DataFrame { frame, .. } => (frame.height(), frame.width()),
        other => {
            return Err(Diagnostic::compute_error(
                "C0201",
                format!("`view()` requires a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };

    let temp_dir = std::env::temp_dir();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let filename = format!("ghl_data_{}_{}.parquet", std::process::id(), timestamp);
    let file_path = temp_dir.join(&filename);
    let path_str = file_path.to_string_lossy().to_string();

    crate::io::write_parquet_file(df_val, &path_str)?;

    let opened = if let Ok(mut child) = std::process::Command::new("positron").arg(&path_str).spawn() {
        let _ = child.wait();
        true
    } else if let Ok(mut child) = std::process::Command::new("code").arg(&path_str).spawn() {
        let _ = child.wait();
        true
    } else {
        false
    };

    if opened {
        println!("ฅ(•⩊ •マ [Data Explorer] Opened {} ({} rows, {} cols) in Positron Data Explorer", filename, nrow, ncol);
    } else {
        println!("ฅ(•⩊ •マ [Data Explorer] Exported {} ({} rows, {} cols) to:\n   {}", filename, nrow, ncol, path_str);
        println!("   ฅ(•⩊ •マ Haru ready! Open this Parquet file in Positron Data Explorer");
    }

    Ok(Value::String(path_str))
}

pub(crate) fn native_scatter(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_point(vec![plot_val])
}

pub(crate) fn native_hist(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_histogram(vec![plot_val])
}

pub(crate) fn native_boxplot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_boxplot(vec![plot_val])
}

pub(crate) fn native_save(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0310", "`save()` / `ggsave()` requires a Plot as first argument")
    })?;

    let path_val = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error(
            "C0311",
            "`save()` requires a destination file path string as second argument (e.g. `save(p, \"output.png\")`)"
        )
    })?;

    match plot_val {
        Value::Plot(p) => {
            p.save_file(path_val).map_err(|e| {
                Diagnostic::compute_error("C0312", format!("Failed to export plot to `{path_val}`: {e}"))
            })?;
            println!("ฅ(•⩊ •マ Haru successfully exported plot to `{}`", path_val);
            Ok(Value::Unit)
        }
        other => Err(Diagnostic::compute_error(
            "C0310",
            format!("`save()` requires a Plot, found `{}`", other.type_name()),
        )),
    }
}

pub(crate) fn native_to_vega_json(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0316", "`to_vega_json()` requires a Plot as first argument")
    })?;
    match plot_val {
        Value::Plot(p) => {
            let json = p.to_vega_json().map_err(|e| Diagnostic::compute_error("C0317", e))?;
            Ok(Value::String(json))
        }
        other => Err(Diagnostic::compute_error("C0316", format!("`to_vega_json()` requires a Plot, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_to_svg(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0318", "`to_svg()` requires a Plot as first argument")
    })?;
    let width = args.get(1).and_then(|v| v.as_i64()).unwrap_or(800) as u32;
    let height = args.get(2).and_then(|v| v.as_i64()).unwrap_or(600) as u32;
    match plot_val {
        Value::Plot(p) => {
            let svg = p.to_svg(width, height).map_err(|e| Diagnostic::compute_error("C0319", e))?;
            Ok(Value::String(svg))
        }
        other => Err(Diagnostic::compute_error("C0318", format!("`to_svg()` requires a Plot, found `{}`", other.type_name()))),
    }
}

fn extract_facet_var_name(v: &Value) -> String {
    match v {
        Value::Formula { response, terms, .. } => {
            if !terms.is_empty() && terms[0] != "." {
                terms[0].clone()
            } else if !response.is_empty() && response != "." {
                response.clone()
            } else {
                String::new()
            }
        }
        _ => extract_raw_string(v),
    }
}

pub(crate) fn native_facet_wrap(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut plot_opt: Option<PlotSpec> = None;
    let mut variable = String::new();
    let mut ncol: Option<usize> = None;
    let mut nrow: Option<usize> = None;
    let mut scales = ghl_plot::FacetScales::Fixed;

    let mut remaining = Vec::new();
    for arg in args {
        match arg {
            Value::Plot(p) => {
                plot_opt = Some(*p);
            }
            Value::NamedArg(name, val) => match name.as_str() {
                "ncol" => {
                    ncol = val.as_i64().map(|n| n.max(1) as usize);
                }
                "nrow" => {
                    nrow = val.as_i64().map(|n| n.max(1) as usize);
                }
                "scales" => {
                    scales = ghl_plot::FacetScales::from_str_loose(&extract_raw_string(&val));
                }
                "facets" | "by" | "var" | "variable" => {
                    variable = extract_facet_var_name(&val);
                }
                _ => {}
            },
            other => {
                remaining.push(other);
            }
        }
    }

    let mut pos_idx = 0;
    if variable.is_empty() && pos_idx < remaining.len() {
        variable = extract_facet_var_name(&remaining[pos_idx]);
        pos_idx += 1;
    }
    if ncol.is_none() && pos_idx < remaining.len() {
        if let Some(n) = remaining[pos_idx].as_i64() {
            ncol = Some(n.max(1) as usize);
            pos_idx += 1;
        }
    }
    if nrow.is_none() && pos_idx < remaining.len() {
        if let Some(r) = remaining[pos_idx].as_i64() {
            nrow = Some(r.max(1) as usize);
            pos_idx += 1;
        }
    }
    if scales == ghl_plot::FacetScales::Fixed && pos_idx < remaining.len() {
        let s_str = extract_raw_string(&remaining[pos_idx]);
        scales = ghl_plot::FacetScales::from_str_loose(&s_str);
    }

    if variable.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0320",
            "`facet_wrap()` requires a faceting variable (e.g. `facet_wrap(\"species\")` or `facet_wrap(~ species)`)"
        ));
    }

    let facet_spec = ghl_plot::FacetSpec::wrap(variable.clone(), ncol, nrow, scales);

    if let Some(mut p) = plot_opt {
        if let Some(col) = p.columns_cache.get(&variable) {
            p.facet_data = col.clone();
        }
        p.facet = Some(facet_spec);
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Facet(facet_spec))
    }
}

pub(crate) fn native_facet_grid(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut plot_opt: Option<PlotSpec> = None;
    let mut row_var: Option<String> = None;
    let mut col_var: Option<String> = None;
    let mut scales = ghl_plot::FacetScales::Fixed;

    let mut remaining = Vec::new();
    for arg in args {
        match arg {
            Value::Plot(p) => {
                plot_opt = Some(*p);
            }
            Value::NamedArg(name, val) => match name.as_str() {
                "rows" | "row" => {
                    let s = extract_facet_var_name(&val);
                    if !s.is_empty() && s != "." {
                        row_var = Some(s);
                    }
                }
                "cols" | "col" => {
                    let s = extract_facet_var_name(&val);
                    if !s.is_empty() && s != "." {
                        col_var = Some(s);
                    }
                }
                "scales" => {
                    scales = ghl_plot::FacetScales::from_str_loose(&extract_raw_string(&val));
                }
                _ => {}
            },
            Value::Formula { response, terms, .. } => {
                if !response.is_empty() && response != "." {
                    row_var = Some(response.clone());
                }
                if !terms.is_empty() && terms[0] != "." {
                    col_var = Some(terms[0].clone());
                }
            }
            other => {
                remaining.push(other);
            }
        }
    }

    let mut pos_idx = 0;
    if row_var.is_none() && pos_idx < remaining.len() {
        let s = extract_facet_var_name(&remaining[pos_idx]);
        if !s.is_empty() && s != "." {
            row_var = Some(s);
        }
        pos_idx += 1;
    }
    if col_var.is_none() && pos_idx < remaining.len() {
        let s = extract_facet_var_name(&remaining[pos_idx]);
        if !s.is_empty() && s != "." {
            col_var = Some(s);
        }
        pos_idx += 1;
    }
    if scales == ghl_plot::FacetScales::Fixed && pos_idx < remaining.len() {
        let s_str = extract_raw_string(&remaining[pos_idx]);
        scales = ghl_plot::FacetScales::from_str_loose(&s_str);
    }

    if row_var.is_none() && col_var.is_none() {
        return Err(Diagnostic::compute_error(
            "C0321",
            "`facet_grid()` requires at least a row or column variable (e.g. `facet_grid(drv ~ cyl)` or `facet_grid(rows = \"drv\", cols = \"cyl\")`)"
        ));
    }

    let facet_spec = ghl_plot::FacetSpec::grid(row_var.clone(), col_var.clone(), scales);

    if let Some(mut p) = plot_opt {
        if let Some(ref r) = row_var {
            if let Some(col) = p.columns_cache.get(r) {
                p.facet_row_data = col.clone();
            }
        }
        if let Some(ref c) = col_var {
            if let Some(col) = p.columns_cache.get(c) {
                p.facet_data = col.clone();
            }
        }
        p.facet = Some(facet_spec);
        Ok(Value::Plot(Box::new(p)))
    } else {
        Ok(Value::Facet(facet_spec))
    }
}
