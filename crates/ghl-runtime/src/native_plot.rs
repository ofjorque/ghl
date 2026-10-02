//! Grammar of Graphics (`std::plot`, RFC 16) native functions.
//!
//! Grounded in GHL native types (F64, I64, Factor, String, Bool, Date).
//! Provides multi-layer composition, color grouping with Okabe-Ito palettes,
//! interactive Vega-Lite JSON export, and Positron Plots pane integration.

use std::collections::BTreeMap;
use ghl_diagnostics::{Diagnostic, RenderCaps};
use ghl_plot::{AestheticMap, DataSeries, GeomLayer, PlotSpec};
use ghl_types::ContrastScheme;
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
    let x = args.first().map(extract_raw_string).ok_or_else(|| {
        Diagnostic::compute_error("C0301", "`aes()` requires at least an `x` aesthetic")
    })?;

    let y = args.get(1).map(extract_raw_string);
    let color = args.get(2).map(extract_raw_string);

    Ok(Value::Aesthetic(AestheticMap {
        x,
        y,
        color,
        size: None,
        shape: None,
        facet_col: None,
        facet_row: None,
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

                if let Some(ref y_name) = aes.y {
                    let ys = col_f64(y_name);
                    plot_spec.y_data = ys.clone();
                    plot_spec.labels.y_label = Some(y_name.clone());
                    plot_spec.labels.title = Some(format!("Plot: {} vs {}", y_name, aes.x));

                    // Group by color aesthetic if specified
                    if let Some(ref color_name) = aes.color {
                        let color_col = col_values(color_name);
                        let n = xs.len().min(ys.len()).min(color_col.len());
                        let mut groups: BTreeMap<String, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
                        for i in 0..n {
                            let g_key = extract_raw_string(&color_col[i]);
                            let entry = groups.entry(g_key).or_insert_with(|| (Vec::new(), Vec::new()));
                            entry.0.push(xs[i]);
                            entry.1.push(ys[i]);
                        }
                        let mut series = Vec::new();
                        for (g_name, (g_xs, g_ys)) in groups {
                            series.push(DataSeries {
                                group_name: Some(g_name),
                                color_hex: None,
                                x_values: g_xs,
                                y_values: g_ys,
                                categories: Vec::new(),
                            });
                        }
                        plot_spec = plot_spec.with_series(series);
                        plot_spec.labels.color_label = Some(color_name.clone());
                    }
                } else {
                    plot_spec.labels.title = Some(format!("Distribution of {}", aes.x));
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

pub(crate) fn native_geom_point(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            p = p.add_layer(GeomLayer::point());
            Ok(Value::Plot(Box::new(p)))
        }
        _ => Ok(Value::Geom(GeomLayer::point())),
    }
}

pub(crate) fn native_geom_line(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            p = p.add_layer(GeomLayer::line());
            Ok(Value::Plot(Box::new(p)))
        }
        _ => Ok(Value::Geom(GeomLayer::line())),
    }
}

pub(crate) fn native_geom_smooth(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            let layer = match crate::plot_stats::simple_linear_fit(&p.x_data, &p.y_data) {
                Some(fit) => GeomLayer::smooth_with_fit(fit),
                None => GeomLayer::smooth(),
            };
            p = p.add_layer(layer);
            Ok(Value::Plot(Box::new(p)))
        }
        _ => Ok(Value::Geom(GeomLayer::smooth())),
    }
}

pub(crate) fn native_geom_histogram(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let (plot_opt, bins) = match args.first() {
        Some(Value::Plot(plot)) => {
            let b = args.get(1).and_then(|v| v.as_i64()).unwrap_or(8) as usize;
            (Some(plot), b)
        }
        Some(other) => {
            let b = other.as_i64().unwrap_or(8) as usize;
            (None, b)
        }
        None => (None, 8),
    };

    match plot_opt {
        Some(plot) => {
            let mut p = (**plot).clone();
            p = p.add_layer(GeomLayer::histogram(bins));
            Ok(Value::Plot(Box::new(p)))
        }
        None => Ok(Value::Geom(GeomLayer::histogram(bins))),
    }
}

pub(crate) fn native_geom_boxplot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            let layer = if !p.categories.is_empty() && !p.y_data.is_empty() {
                let n = p.categories.len().min(p.y_data.len());
                let mut grouped: BTreeMap<String, Vec<f64>> = BTreeMap::new();
                for i in 0..n {
                    grouped.entry(p.categories[i].clone()).or_default().push(p.y_data[i]);
                }
                let mut group_stats = Vec::new();
                for (cat, vals) in grouped {
                    if let Some(st) = crate::plot_stats::five_number_summary(&vals) {
                        group_stats.push((cat, st));
                    }
                }
                if !group_stats.is_empty() {
                    GeomLayer::boxplot_with_multi_stats(group_stats)
                } else {
                    GeomLayer::boxplot()
                }
            } else {
                match crate::plot_stats::five_number_summary(&p.x_data) {
                    Some(stats) => GeomLayer::boxplot_with_stats(stats),
                    None if p.x_data.is_empty() => GeomLayer::boxplot(),
                    None => {
                        return Err(Diagnostic::statistical_error(
                            "S0302",
                            format!("`geom_boxplot()` requires at least 4 observations, found {}", p.x_data.len()),
                        ));
                    }
                }
            };
            p = p.add_layer(layer);
            Ok(Value::Plot(Box::new(p)))
        }
        _ => Ok(Value::Geom(GeomLayer::boxplot())),
    }
}

pub(crate) fn native_geom_bar(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            p = p.add_layer(GeomLayer::bar());
            Ok(Value::Plot(Box::new(p)))
        }
        _ => Ok(Value::Geom(GeomLayer::bar())),
    }
}

pub(crate) fn native_geom_area(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            p = p.add_layer(GeomLayer::area());
            Ok(Value::Plot(Box::new(p)))
        }
        _ => Ok(Value::Geom(GeomLayer::area())),
    }
}

pub(crate) fn native_geom_rug(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            p = p.add_layer(GeomLayer::rug());
            Ok(Value::Plot(Box::new(p)))
        }
        _ => Ok(Value::Geom(GeomLayer::rug())),
    }
}

pub(crate) fn native_labs(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => return Err(Diagnostic::compute_error("C0303", "`labs()` requires a Plot as first argument")),
    };

    if let Some(title) = args.get(1).and_then(|v| v.as_str()) {
        p.labels.title = Some(title.to_string());
    }
    if let Some(x_lab) = args.get(2).and_then(|v| v.as_str()) {
        p.labels.x_label = Some(x_lab.to_string());
    }
    if let Some(y_lab) = args.get(3).and_then(|v| v.as_str()) {
        p.labels.y_label = Some(y_lab.to_string());
    }

    Ok(Value::Plot(Box::new(p)))
}

pub(crate) fn native_scale_x_log10(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            let mut p = (**plot).clone();
            p = p.scale_x_log10();
            Ok(Value::Plot(Box::new(p)))
        }
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
        _ => Err(Diagnostic::compute_error("C0315", "`scale_y_log10()` requires a Plot as first argument")),
    }
}

pub(crate) fn native_theme_minimal(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.theme_minimal();
    Ok(Value::Plot(Box::new(p)))
}

pub(crate) fn native_theme_classic(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.theme_classic();
    Ok(Value::Plot(Box::new(p)))
}

pub(crate) fn native_theme_dark(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.theme_dark();
    Ok(Value::Plot(Box::new(p)))
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
