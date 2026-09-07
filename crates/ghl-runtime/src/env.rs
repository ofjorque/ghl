use std::collections::HashMap;
use ghl_diagnostics::{AestheticMap, Diagnostic, GeomLayer, PlotSpec, RenderCaps};
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEnv {
    pub scopes: Vec<HashMap<String, Value>>,
}

impl RuntimeEnv {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn with_prelude() -> Self {
        let mut env = Self::new();

        // Native function: mean
        env.set("mean".into(), Value::NativeFn(native_mean));

        // Native function: sum
        env.set("sum".into(), Value::NativeFn(native_sum));

        // Native function: std_dev
        env.set("std_dev".into(), Value::NativeFn(native_std_dev));

        // Native function: var
        env.set("var".into(), Value::NativeFn(native_var));

        // Native function: min
        env.set("min".into(), Value::NativeFn(native_min));

        // Native function: max
        env.set("max".into(), Value::NativeFn(native_max));

        // Native function: col (extracts column reference)
        env.set("col".into(), Value::NativeFn(native_col));

        // Native function: filter (for dataframes)
        env.set("filter".into(), Value::NativeFn(native_filter));

        // Native function: purr (diagnostic trace)
        env.set("purr".into(), Value::NativeFn(|args| {
            let msg = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            println!("(=^･ω･^=) [purr] {}", msg);
            Ok(Value::Unit)
        }));

        // Native function: pounce (diagnostic assertion)
        env.set("pounce".into(), Value::NativeFn(|args| {
            let cond = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
            let msg = args.get(1).map(|v| format!("{}", v)).unwrap_or_else(|| "Assertion failed".into());
            if !cond {
                Err(Diagnostic::compute_error("C0999", format!("(・`ω´・) [pounce failed] {}", msg)))
            } else {
                Ok(Value::Unit)
            }
        }));

        // Print helpers
        env.set("println".into(), Value::NativeFn(|args| {
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    print!(" ");
                }
                match a {
                    Value::String(s) => print!("{}", s),
                    other => print!("{}", other),
                }
            }
            println!();
            Ok(Value::Unit)
        }));

        // NEKO Statistical Modeling Verbs
        env.set("fit".into(), Value::NativeFn(native_fit_ols));
        env.set("ols".into(), Value::NativeFn(native_fit_ols));
        env.set("summary".into(), Value::NativeFn(native_summary));
        env.set("tidy".into(), Value::NativeFn(native_tidy));
        env.set("glance".into(), Value::NativeFn(native_glance));
        env.set("augment".into(), Value::NativeFn(native_augment));
        env.set("predict".into(), Value::NativeFn(native_predict));
        env.set("residuals".into(), Value::NativeFn(native_residuals));
        env.set("coef".into(), Value::NativeFn(native_coef));
        env.set("vcov".into(), Value::NativeFn(native_vcov));

        // Grammar of Graphics (RFC 09) Verbs
        env.set("plot".into(), Value::NativeFn(native_plot));
        env.set("aes".into(), Value::NativeFn(native_aes));
        env.set("geom_point".into(), Value::NativeFn(native_geom_point));
        env.set("geom_line".into(), Value::NativeFn(native_geom_line));
        env.set("geom_smooth".into(), Value::NativeFn(native_geom_smooth));
        env.set("geom_histogram".into(), Value::NativeFn(native_geom_histogram));
        env.set("geom_boxplot".into(), Value::NativeFn(native_geom_boxplot));
        env.set("geom_bar".into(), Value::NativeFn(native_geom_bar));
        env.set("labs".into(), Value::NativeFn(native_labs));
        env.set("show".into(), Value::NativeFn(native_show));
        env.set("save".into(), Value::NativeFn(native_save));
        env.set("scatter".into(), Value::NativeFn(native_scatter));
        env.set("hist".into(), Value::NativeFn(native_hist));
        env.set("histogram".into(), Value::NativeFn(native_hist));
        env.set("boxplot".into(), Value::NativeFn(native_boxplot));

        // Primitive File I/O
        env.set("read_file".into(), Value::NativeFn(native_read_file));
        env.set("read_lines".into(), Value::NativeFn(native_read_lines));
        env.set("write_file".into(), Value::NativeFn(native_write_file));
        env.set("append_file".into(), Value::NativeFn(native_append_file));
        env.set("file_exists".into(), Value::NativeFn(native_file_exists));

        // Tabular CSV I/O
        env.set("read_csv".into(), Value::NativeFn(native_read_csv));
        env.set("parse_csv".into(), Value::NativeFn(native_parse_csv));
        env.set("write_csv".into(), Value::NativeFn(native_write_csv));

        // DataFrame Wrangling Verbs
        env.set("select".into(), Value::NativeFn(native_select));
        env.set("head".into(), Value::NativeFn(native_head));
        env.set("tail".into(), Value::NativeFn(native_tail));

        env
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn set(&mut self, name: String, val: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, val);
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val.clone());
            }
        }
        None
    }

    pub fn assign(&mut self, name: &str, val: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), val);
                return true;
            }
        }
        false
    }
}

// Built-in Native Functions

fn native_mean(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`mean()` requires at least 1 argument")
    })?;

    match vec_val {
        Value::Vector(items) => {
            if items.is_empty() {
                return Ok(Value::NA(Some("EmptyVector".into())));
            }

            let mut sum = 0.0;
            let mut count = 0;

            for item in items {
                if let Value::NA(r) = item {
                    // Under GHL Kleene semantics: NA propagates through mean unless skip_na is active
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    sum += x;
                    count += 1;
                }
            }

            if count == 0 {
                Ok(Value::NA(None))
            } else {
                Ok(Value::F64(sum / count as f64))
            }
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`mean()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

fn native_sum(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`sum()` requires 1 argument")
    })?;

    match vec_val {
        Value::Vector(items) => {
            let mut sum = 0.0;
            let mut has_float = false;

            for item in items {
                if let Value::NA(r) = item {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    if matches!(item, Value::F64(_)) {
                        has_float = true;
                    }
                    sum += x;
                }
            }

            if has_float {
                Ok(Value::F64(sum))
            } else {
                Ok(Value::I64(sum as i64))
            }
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`sum()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

fn native_var(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`var()` requires 1 argument")
    })?;

    match vec_val {
        Value::Vector(items) => {
            if items.len() < 2 {
                return Err(Diagnostic::statistical_warning(
                    "SW0002",
                    "Sample variance requires at least N=2 observations (N-1 degrees of freedom)",
                ));
            }

            let mut numbers = Vec::with_capacity(items.len());
            for item in items {
                if let Value::NA(r) = item {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    numbers.push(x);
                }
            }

            let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
            let sum_sq = numbers.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
            let var = sum_sq / (numbers.len() - 1) as f64;

            Ok(Value::F64(var))
        }
        _ => Err(Diagnostic::compute_error(
            "C0202",
            format!("`var()` expects a Vector, found `{}`", vec_val.type_name()),
        )),
    }
}

fn native_std_dev(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let var_val = native_var(args)?;
    if let Value::F64(v) = var_val {
        Ok(Value::F64(v.sqrt()))
    } else {
        Ok(var_val)
    }
}

fn native_min(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`min()` requires 1 argument")
    })?;

    match vec_val {
        Value::Vector(items) => {
            let mut min_val = f64::INFINITY;
            let mut found = false;

            for item in items {
                if let Value::NA(r) = item {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    if x < min_val {
                        min_val = x;
                        found = true;
                    }
                }
            }

            if found {
                Ok(Value::F64(min_val))
            } else {
                Ok(Value::NA(None))
            }
        }
        _ => Err(Diagnostic::compute_error("C0202", "`min()` expects a Vector")),
    }
}

fn native_max(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let vec_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`max()` requires 1 argument")
    })?;

    match vec_val {
        Value::Vector(items) => {
            let mut max_val = f64::NEG_INFINITY;
            let mut found = false;

            for item in items {
                if let Value::NA(r) = item {
                    return Ok(Value::NA(r.clone()));
                } else if let Some(x) = item.as_f64() {
                    if x > max_val {
                        max_val = x;
                        found = true;
                    }
                }
            }

            if found {
                Ok(Value::F64(max_val))
            } else {
                Ok(Value::NA(None))
            }
        }
        _ => Err(Diagnostic::compute_error("C0202", "`max()` expects a Vector")),
    }
}

fn native_col(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let col_name = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`col()` requires a string column name")
    })?;
    Ok(Value::ColRef(col_name.to_string()))
}

fn eval_predicate(op: ghl_syntax::ast::BinaryOp, left: &Value, right: &Value) -> bool {
    use ghl_syntax::ast::BinaryOp;
    match op {
        BinaryOp::Eq => left == right,
        BinaryOp::NotEq => left != right,
        BinaryOp::Lt => left.as_f64().and_then(|l| right.as_f64().map(|r| l < r)).unwrap_or(false),
        BinaryOp::LtEq => left.as_f64().and_then(|l| right.as_f64().map(|r| l <= r)).unwrap_or(false),
        BinaryOp::Gt => left.as_f64().and_then(|l| right.as_f64().map(|r| l > r)).unwrap_or(false),
        BinaryOp::GtEq => left.as_f64().and_then(|l| right.as_f64().map(|r| l >= r)).unwrap_or(false),
        _ => false,
    }
}

fn native_filter(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`filter()` requires a DataFrame"));
    }

    let df = args[0].clone();
    match df {
        Value::DataFrame { columns, data } => {
            // If condition was passed as second argument:
            if args.len() > 1 {
                // If the second argument is a ColPredicate
                if let Value::ColPredicate { col, op, rhs } = &args[1] {
                    let mut new_data: HashMap<String, Vec<Value>> = HashMap::new();
                    for c in &columns {
                        new_data.insert(c.clone(), Vec::new());
                    }

                    if let Some(col_vec) = data.get(col) {
                        for (row_idx, item) in col_vec.iter().enumerate() {
                            if eval_predicate(*op, item, rhs) {
                                for c in &columns {
                                    if let Some(c_vec) = data.get(c) {
                                        if let Some(val) = c_vec.get(row_idx) {
                                            new_data.get_mut(c).unwrap().push(val.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    return Ok(Value::DataFrame {
                        columns,
                        data: new_data,
                    });
                }

                // If the second argument is a boolean Vector (mask)
                if let Value::Vector(mask) = &args[1] {
                    let mut new_data: HashMap<String, Vec<Value>> = HashMap::new();
                    for col in &columns {
                        new_data.insert(col.clone(), Vec::new());
                    }

                    for (row_idx, m) in mask.iter().enumerate() {
                        if m.as_bool() == Some(true) {
                            for col in &columns {
                                if let Some(col_vec) = data.get(col) {
                                    if let Some(val) = col_vec.get(row_idx) {
                                        new_data.get_mut(col).unwrap().push(val.clone());
                                    }
                                }
                            }
                        }
                    }

                    return Ok(Value::DataFrame {
                        columns,
                        data: new_data,
                    });
                }
            }

            Ok(Value::DataFrame { columns, data })
        }
        other => Ok(other),
    }
}

// NEKO Native Invocations

fn native_fit_ols(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`fit()` requires Formula and DataFrame arguments: `fit(model, df)`",
        ));
    }

    let (response, terms) = match &args[0] {
        Value::Formula { response, terms } => (response.clone(), terms.clone()),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("First argument of `fit()` must be a Formula, found `{}`", other.type_name()),
            ));
        }
    };

    let (columns, data) = match &args[1] {
        Value::DataFrame { columns, data } => (columns, data),
        other => {
            return Err(Diagnostic::statistical_error(
                "S0200",
                format!("Second argument of `fit()` must be a DataFrame, found `{}`", other.type_name()),
            ));
        }
    };

    let blueprint = crate::neko::Blueprint::new(response, terms);
    let model = crate::neko::FittedModel::fit_ols(blueprint, columns, data)?;
    Ok(Value::ModelFit(Box::new(model)))
}

fn native_summary(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`summary()` requires a ModelFit or printable object")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            println!("{}", m);
            Ok(Value::Unit)
        }
        other => {
            println!("{}", other);
            Ok(Value::Unit)
        }
    }
}

fn native_tidy(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`tidy()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(m.tidy()),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`tidy()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_glance(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`glance()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => Ok(m.glance()),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`glance()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_augment(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`augment()` requires ModelFit and DataFrame arguments: `augment(model, df)`",
        ));
    }

    match &args[0] {
        Value::ModelFit(m) => m.augment(&args[1]),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("First argument of `augment()` must be a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_predict(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`predict()` requires ModelFit and DataFrame arguments: `predict(model, df)`",
        ));
    }

    match &args[0] {
        Value::ModelFit(m) => m.predict(&args[1]),
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("First argument of `predict()` must be a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_residuals(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`residuals()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            let res_vals = m.residuals.iter().map(|&x| Value::F64(x)).collect();
            Ok(Value::Vector(res_vals))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`residuals()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_coef(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`coef()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            let coef_vals = m.coefficients.iter().map(|&x| Value::F64(x)).collect();
            Ok(Value::Vector(coef_vals))
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`coef()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

fn native_vcov(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let model_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`vcov()` requires a ModelFit")
    })?;

    match model_val {
        Value::ModelFit(m) => {
            let kind = if let Some(k_str) = args.get(1).and_then(|v| v.as_str()) {
                match k_str.to_uppercase().as_str() {
                    "HC0" => crate::neko::VcovKind::HC0,
                    "HC1" => crate::neko::VcovKind::HC1,
                    "HC2" => crate::neko::VcovKind::HC2,
                    "HC3" => crate::neko::VcovKind::HC3,
                    _ => crate::neko::VcovKind::Classical,
                }
            } else {
                crate::neko::VcovKind::Classical
            };
            let p = m.blueprint.term_names.len();
            let vcov_data = m.compute_vcov(kind);
            Ok(Value::Matrix {
                rows: p,
                cols: p,
                data: vcov_data,
            })
        }
        other => Err(Diagnostic::statistical_error(
            "S0200",
            format!("`vcov()` requires a ModelFit, found `{}`", other.type_name()),
        )),
    }
}

// =========================================================================
// Grammar of Graphics (std::plot - RFC 09) Native Functions
// =========================================================================

fn native_aes(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

fn native_plot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Ok(Value::Plot(Box::new(PlotSpec::new())));
    }

    let first = &args[0];
    match first {
        Value::DataFrame { columns: _, data } => {
            let mut plot_spec = PlotSpec::new();

            if let Some(Value::Aesthetic(aes)) = args.get(1) {
                plot_spec = plot_spec.with_mapping(aes.clone());
                if let Some(col_data) = data.get(&aes.x) {
                    let xs: Vec<f64> = col_data.iter().filter_map(|v| v.as_f64()).collect();
                    plot_spec = plot_spec.with_x_data(xs);
                }
                if let Some(ref y_name) = aes.y {
                    if let Some(col_data) = data.get(y_name) {
                        let ys: Vec<f64> = col_data.iter().filter_map(|v| v.as_f64()).collect();
                        plot_spec.y_data = ys;
                    }
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
                if let Some(col_data) = data.get(&x_name) {
                    let xs: Vec<f64> = col_data.iter().filter_map(|v| v.as_f64()).collect();
                    plot_spec = plot_spec.with_x_data(xs);
                }
                plot_spec.labels.x_label = Some(x_name.clone());

                if let Some(y_arg) = args.get(2) {
                    let y_name = match y_arg {
                        Value::ColRef(s) | Value::String(s) => s.clone(),
                        other => format!("{other}"),
                    };
                    if let Some(col_data) = data.get(&y_name) {
                        let ys: Vec<f64> = col_data.iter().filter_map(|v| v.as_f64()).collect();
                        plot_spec.y_data = ys;
                    }
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

fn native_geom_point(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::point());
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_line(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::line());
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_smooth(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::smooth());
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_histogram(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    let bins = args.get(1).and_then(|v| v.as_i64()).unwrap_or(8) as usize;
    p = p.add_layer(GeomLayer::histogram(bins));
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_boxplot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::boxplot());
    Ok(Value::Plot(Box::new(p)))
}

fn native_geom_bar(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let mut p = match args.first() {
        Some(Value::Plot(plot)) => (**plot).clone(),
        _ => PlotSpec::new(),
    };
    p = p.add_layer(GeomLayer::bar());
    Ok(Value::Plot(Box::new(p)))
}

fn native_labs(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

fn native_show(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let target = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0305", "`show()` requires a Plot or Value to render")
    })?;

    let caps = RenderCaps::detect();
    match target {
        Value::Plot(p) => {
            println!("{}\n", p.render(&caps));
        }
        other => {
            println!("{}\n", other.render_styled(&caps));
        }
    }
    Ok(Value::Unit)
}

fn native_scatter(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_point(vec![plot_val])
}

fn native_hist(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_histogram(vec![plot_val])
}

fn native_boxplot(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let plot_val = native_plot(args)?;
    native_geom_boxplot(vec![plot_val])
}

fn native_save(args: Vec<Value>) -> Result<Value, Diagnostic> {
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

// =========================================================================
// Primitive and Tabular I/O Native Functions
// =========================================================================

fn native_read_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`read_file()` requires a file path string")
    })?;
    let content = crate::io::read_file(path)?;
    Ok(Value::String(content))
}

fn native_read_lines(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`read_lines()` requires a file path string")
    })?;
    let lines = crate::io::read_lines(path)?;
    let vec_vals = lines.into_iter().map(Value::String).collect();
    Ok(Value::Vector(vec_vals))
}

fn resolve_path_and_content(s1: &str, s2: &str) -> (String, String) {
    if s1.contains('\n') || s1.contains('\r') || s1.contains(',') {
        (s2.to_string(), s1.to_string())
    } else if s2.contains('\n') || s2.contains('\r') || s2.contains(',') {
        (s1.to_string(), s2.to_string())
    } else if s2.starts_with('/')
        || s2.starts_with("./")
        || s2.starts_with("../")
        || s2.ends_with(".txt")
        || s2.ends_with(".csv")
        || s2.ends_with(".tsv")
        || s2.ends_with(".json")
        || s2.ends_with(".gh")
        || s2.ends_with(".log")
    {
        (s2.to_string(), s1.to_string())
    } else {
        (s1.to_string(), s2.to_string())
    }
}

fn native_write_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0402",
            "`write_file()` requires destination path and content: `write_file(path, content)` or `content |> write_file(path)`",
        ));
    }

    let (path, content): (String, String) = if let (Some(s1), Some(s2)) = (args[0].as_str(), args[1].as_str()) {
        resolve_path_and_content(s1, s2)
    } else if let Some(p) = args[1].as_str() {
        (p.to_string(), args[0].to_string())
    } else if let Some(p) = args[0].as_str() {
        (p.to_string(), args[1].to_string())
    } else {
        return Err(Diagnostic::compute_error("C0402", "`write_file()` requires string arguments"));
    };

    crate::io::write_file(&path, &content)?;
    Ok(Value::Unit)
}

fn native_append_file(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0402",
            "`append_file()` requires destination path and content: `append_file(path, content)` or `content |> append_file(path)`",
        ));
    }

    let (path, content): (String, String) = if let (Some(s1), Some(s2)) = (args[0].as_str(), args[1].as_str()) {
        resolve_path_and_content(s1, s2)
    } else if let Some(p) = args[1].as_str() {
        (p.to_string(), args[0].to_string())
    } else if let Some(p) = args[0].as_str() {
        (p.to_string(), args[1].to_string())
    } else {
        return Err(Diagnostic::compute_error("C0402", "`append_file()` requires string arguments"));
    };

    crate::io::append_file(&path, &content)?;
    Ok(Value::Unit)
}

fn native_file_exists(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0401", "`file_exists()` requires a file path string")
    })?;
    Ok(Value::Bool(crate::io::file_exists(path)))
}

fn native_read_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let path = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0403", "`read_csv()` requires a file path string")
    })?;
    let delim = args.get(1).and_then(|v| v.as_str()).and_then(|s| s.chars().next());
    crate::io::read_csv_file(path, delim)
}

fn native_parse_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let content = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0403", "`parse_csv()` requires a CSV text string")
    })?;
    let delim = args.get(1).and_then(|v| v.as_str()).and_then(|s| s.chars().next());
    crate::io::parse_csv_string(content, delim)
}

fn native_write_csv(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0404",
            "`write_csv()` requires DataFrame and path arguments: `write_csv(df, \"out.csv\")` or `df |> write_csv(\"out.csv\")`",
        ));
    }

    let (df, path, delim_idx) = match (&args[0], &args[1]) {
        (Value::DataFrame { .. }, _) => (&args[0], args[1].as_str(), 2),
        (_, Value::DataFrame { .. }) => (&args[1], args[0].as_str(), 2),
        _ => {
            return Err(Diagnostic::compute_error(
                "C0404",
                "`write_csv()` requires a DataFrame argument",
            ));
        }
    };

    let p = path.ok_or_else(|| {
        Diagnostic::compute_error("C0404", "`write_csv()` requires a string destination path")
    })?;

    let delim = args.get(delim_idx).and_then(|v| v.as_str()).and_then(|s| s.chars().next());
    crate::io::write_csv_file(df, p, delim)?;
    Ok(Value::Unit)
}

fn native_select(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`select()` requires a DataFrame"));
    }

    let df = &args[0];
    let mut cols = Vec::new();

    for arg in &args[1..] {
        match arg {
            Value::Vector(items) => {
                for it in items {
                    match it {
                        Value::ColRef(s) | Value::String(s) => cols.push(s.clone()),
                        other => cols.push(format!("{other}")),
                    }
                }
            }
            Value::ColRef(s) | Value::String(s) => cols.push(s.clone()),
            other => cols.push(format!("{other}")),
        }
    }

    crate::io::df_select(df, &cols)
}

fn native_head(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`head()` requires a DataFrame")
    })?;
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    crate::io::df_head(df, n)
}

fn native_tail(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let df = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`tail()` requires a DataFrame")
    })?;
    let n = args.get(1).and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    crate::io::df_tail(df, n)
}

