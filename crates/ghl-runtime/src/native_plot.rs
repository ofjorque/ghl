//! Grammar of Graphics (`std::plot`, RFC 09) native functions.
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use ghl_diagnostics::{AestheticMap, Diagnostic, GeomLayer, PlotSpec, RenderCaps};
use ghl_types::ContrastScheme;
use crate::value::Value;


// =========================================================================
// Grammar of Graphics (std::plot - RFC 09) Native Functions
// =========================================================================

pub(crate) fn native_aes(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let extract_name = |v: &Value| -> String {
        match v {
            Value::ColRef(s) => s.clone(),
            Value::String(s) => s.clone(),
            other => format!("{other}"),
        }
    };

    let x = args.first().map(extract_name).ok_or_else(|| {
        Diagnostic::compute_error("C0301", "`aes()` requires at least an `x` aesthetic")
    })?;

    let y = args.get(1).map(extract_name);
    let color = args.get(2).map(extract_name);

    Ok(Value::Aesthetic(AestheticMap { x, y, color }))
}

pub(crate) fn native_plot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Ok(Value::Plot(Box::new(PlotSpec::new())));
    }

    let first = &args[0];
    match first {
        Value::DataFrame { frame, na_reasons } => {
            let mut plot_spec = PlotSpec::new();
            let col_f64 = |name: &str| -> Vec<f64> {
                crate::polars_bridge::pull_column_as_values(frame, na_reasons, name)
                    .map(|vals| vals.iter().filter_map(|v| v.as_f64()).collect())
                    .unwrap_or_default()
            };

            if let Some(Value::Aesthetic(aes)) = args.get(1) {
                plot_spec = plot_spec.with_mapping(aes.clone());
                plot_spec = plot_spec.with_x_data(col_f64(&aes.x));
                if let Some(ref y_name) = aes.y {
                    plot_spec.y_data = col_f64(y_name);
                    plot_spec.labels.y_label = Some(y_name.clone());
                    plot_spec.labels.title = Some(format!("Plot: {} vs {}", y_name, aes.x));
                } else {
                    plot_spec.labels.title = Some(format!("Distribution of {}", aes.x));
                }
                plot_spec.labels.x_label = Some(aes.x.clone());
            } else if let Some(x_arg) = args.get(1) {
                let x_name = match x_arg {
                    Value::ColRef(s) | Value::String(s) => s.clone(),
                    other => format!("{other}"),
                };
                plot_spec = plot_spec.with_x_data(col_f64(&x_name));
                plot_spec.labels.x_label = Some(x_name.clone());

                if let Some(y_arg) = args.get(2) {
                    let y_name = match y_arg {
                        Value::ColRef(s) | Value::String(s) => s.clone(),
                        other => format!("{other}"),
                    };
                    plot_spec.y_data = col_f64(&y_name);
                    plot_spec.labels.y_label = Some(y_name.clone());
                    plot_spec.labels.title = Some(format!("Plot: {} vs {}", y_name, x_name));
                } else {
                    plot_spec.labels.title = Some(format!("Distribution of {}", x_name));
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
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::point());
    Ok(Value::Plot(Box::new(p)))
}

pub(crate) fn native_geom_line(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::line());
    Ok(Value::Plot(Box::new(p)))
}

pub(crate) fn native_geom_smooth(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    let layer = match crate::plot_stats::simple_linear_fit(&p.x_data, &p.y_data) {
        Some(fit) => GeomLayer::smooth_with_fit(fit),
        None => GeomLayer::smooth(),
    };
    p = p.add_layer(layer);
    Ok(Value::Plot(Box::new(p)))
}

pub(crate) fn native_geom_histogram(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    let bins = args.get(1).and_then(|v| v.as_i64()).unwrap_or(8) as usize;
    p = p.add_layer(GeomLayer::histogram(bins));
    Ok(Value::Plot(Box::new(p)))
}

pub(crate) fn native_geom_boxplot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    let layer = match crate::plot_stats::five_number_summary(&p.x_data) {
        Some(stats) => GeomLayer::boxplot_with_stats(stats),
        None if p.x_data.is_empty() => GeomLayer::boxplot(),
        None => {
            return Err(Diagnostic::statistical_error(
                "S0302",
                format!("`geom_boxplot()` requires at least 4 observations, found {}", p.x_data.len()),
            ));
        }
    };
    p = p.add_layer(layer);
    Ok(Value::Plot(Box::new(p)))
}

pub(crate) fn native_geom_bar(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::bar());
    Ok(Value::Plot(Box::new(p)))
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
                    eprintln!("(=^･ω･^=) [Plots Pane] Warning: failed to save SVG plot: {e}");
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
        println!("(=^･ω･^=) [Data Explorer] Opened {} ({} rows, {} cols) in Positron Data Explorer", filename, nrow, ncol);
    } else {
        println!("(=^･ω･^=) [Data Explorer] Exported {} ({} rows, {} cols) to:\n   {}", filename, nrow, ncol, path_str);
        println!("   (U・ᴥ・U) Haru ready! Open this Parquet file in Positron Data Explorer");
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
        Diagnostic::compute_error("C0310", "`save()` requires a Plot as first argument")
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
            println!("(U・ᴥ・U) Haru successfully exported plot to `{}`", path_val);
            Ok(Value::Unit)
        }
        other => Err(Diagnostic::compute_error(
            "C0310",
            format!("`save()` requires a Plot, found `{}`", other.type_name()),
        )),
    }
}
