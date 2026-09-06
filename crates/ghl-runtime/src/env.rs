use std::collections::HashMap;
use ghl_diagnostics::Diagnostic;
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

