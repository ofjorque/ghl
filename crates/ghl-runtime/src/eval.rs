use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::*;
use polars_core::prelude::DataType;
use rayon::prelude::*;
use crate::value::Value;
use crate::vector_data::VectorData;
use crate::env::RuntimeEnv;
use crate::matrix::MatrixOps;

/// See `Interpreter::eval_expr_tail`.
enum TailOutcome {
    Value(Value),
    TailCall { callee: Value, args: Vec<Value> },
}

pub struct Interpreter {
    pub env: RuntimeEnv,
    /// Set by `StmtKind::Return` and read by every statement-sequencing loop (`Block`,
    /// `eval_program`) to short-circuit the rest of the current sequence once a `return`
    /// has fired -- see the doc comment on `ExprKind::Block`'s handling below for why a
    /// dedicated field was chosen over threading a new `Result` variant through
    /// `eval_expr`'s ~40-armed match. Cleared only at a function-call boundary
    /// (`call_value`'s `Closure` arm) or at the end of `eval_program` -- everywhere in
    /// between, it stays set and unconsumed so an outer, enclosing `Block` (a `return`
    /// nested inside an `if`/`match`) also observes it and short-circuits in turn.
    pending_return: Option<Value>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            env: RuntimeEnv::with_prelude(),
            pending_return: None,
        }
    }

    pub fn with_env(env: RuntimeEnv) -> Self {
        Self {
            env,
            pending_return: None,
        }
    }

    pub fn eval_program(&mut self, program: &Program) -> Result<Value, Diagnostic> {
        let mut last_val = Value::Unit;
        for stmt in &program.statements {
            last_val = self.eval_stmt(stmt)?;
            if self.pending_return.is_some() {
                break;
            }
        }
        self.pending_return = None;
        Ok(last_val)
    }

    pub fn eval_stmt(&mut self, stmt: &Stmt) -> Result<Value, Diagnostic> {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                let val = self.eval_expr(init)?;
                self.env.set(name.clone(), val.clone());
                Ok(val)
            }
            StmtKind::Fn { name, params, body, .. } => {
                let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                let mut captured_env = self.env.clone();
                let placeholder = Value::Closure {
                    params: param_names.clone(),
                    body: body.clone(),
                    env: captured_env.clone(),
                };
                captured_env.set(name.clone(), placeholder);
                let recursive_closure = Value::Closure {
                    params: param_names,
                    body: body.clone(),
                    env: captured_env,
                };
                self.env.set(name.clone(), recursive_closure.clone());
                Ok(recursive_closure)
            }
            StmtKind::Struct(_) => Ok(Value::Unit),
            StmtKind::Trait(_) => Ok(Value::Unit),
            StmtKind::Impl(impl_decl) => {
                for item in &impl_decl.items {
                    if let ImplItem::Method { name, params, body, .. } = item {
                        let param_names: Vec<String> = params
                            .iter()
                            .map(|p| {
                                if p.name == "&self" || p.name == "&mut self" {
                                    "self".to_string()
                                } else {
                                    p.name.clone()
                                }
                            })
                            .collect();
                        let method_closure = Value::Closure {
                            params: param_names,
                            body: body.clone(),
                            env: self.env.clone(),
                        };
                        let method_key = format!("{}::{}", impl_decl.target_type, name);
                        self.env.set(method_key, method_closure);
                    }
                }
                Ok(Value::Unit)
            }
            StmtKind::Expr(expr) => self.eval_expr(expr),
            StmtKind::Return(opt_expr) => {
                let val = if let Some(e) = opt_expr {
                    self.eval_expr(e)?
                } else {
                    Value::Unit
                };
                self.pending_return = Some(val.clone());
                Ok(val)
            }
            StmtKind::Assign { name, value } => {
                let val = self.eval_expr(value)?;
                if !self.env.assign(name, val.clone()) {
                    return Err(Diagnostic::compute_error(
                        "C0101",
                        format!("Cannot assign to undefined variable `{}`", name),
                    ));
                }
                Ok(val)
            }
            StmtKind::Use(use_stmt) => {
                if !crate::modules::is_valid_module_path(&use_stmt.path) {
                    return Err(Diagnostic::compute_error(
                        "C0105",
                        format!("Cannot find module `{}`", use_stmt.path.join("::")),
                    ));
                }

                match &use_stmt.kind {
                    UseKind::Glob => {
                        let items = crate::modules::get_module_items(&use_stmt.path).ok_or_else(|| {
                            Diagnostic::compute_error(
                                "C0105",
                                format!("Module `{}` cannot be glob imported", use_stmt.path.join("::")),
                            )
                        })?;
                        for (name, val) in items {
                            self.env.set(name, val);
                        }
                    }
                    UseKind::Items(items) => {
                        for item in items {
                            let mut full_path = use_stmt.path.clone();
                            full_path.push(item.name.clone());
                            let val = crate::modules::lookup_module_item(&full_path).ok_or_else(|| {
                                Diagnostic::compute_error(
                                    "C0106",
                                    format!(
                                        "Cannot find item `{}` in module `{}`",
                                        item.name,
                                        use_stmt.path.join("::")
                                    ),
                                )
                            })?;
                            let bound_name = item.alias.as_deref().unwrap_or(&item.name);
                            self.env.set(bound_name.to_string(), val);
                        }
                    }
                }
                Ok(Value::Unit)
            }
        }
    }

    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, Diagnostic> {
        self.eval_expr_ctx(expr, false)
    }

    /// Evaluate an expression. When `col_ctx` is true, an identifier that isn't bound
    /// in scope resolves to `Value::ColRef(name)` instead of erroring — this is how bare
    /// column names work inside `filter`/`select`/`arrange`/`group_by`/`summarize`/etc.
    /// `col_ctx` is only ever set to true for the argument trees of those specific verb
    /// calls (see `COLUMN_CONTEXT_VERBS` below); everywhere else in the language,
    /// undefined identifiers still error exactly as before.
    fn eval_expr_ctx(&mut self, expr: &Expr, col_ctx: bool) -> Result<Value, Diagnostic> {
        match &expr.kind {
            ExprKind::Lit(Literal::Int(n)) => Ok(Value::I64(*n)),
            ExprKind::Lit(Literal::Float(x)) => Ok(Value::F64(*x)),
            ExprKind::Lit(Literal::String(s)) => Ok(Value::String(s.clone())),
            ExprKind::Lit(Literal::Bool(b)) => Ok(Value::Bool(*b)),
            ExprKind::Lit(Literal::NA(reason)) => Ok(Value::NA(reason.clone())),

            ExprKind::Ident(name) => {
                self.env.get(name).or_else(|| {
                    if col_ctx { Some(Value::ColRef(name.clone())) } else { None }
                }).ok_or_else(|| {
                    Diagnostic::compute_error("C0101", format!("Undefined identifier `{}`", name))
                })
            }

            ExprKind::VectorLit(items) => {
                let mut evaluated = Vec::with_capacity(items.len());
                for item in items {
                    evaluated.push(self.eval_expr_ctx(item, col_ctx)?);
                }
                Ok(Value::Vector(VectorData::from_values(evaluated)))
            }

            ExprKind::NamedArg { name, value } => {
                let v = self.eval_expr_ctx(value, col_ctx)?;
                Ok(Value::NamedArg(name.clone(), Box::new(v)))
            }

            ExprKind::DataFrameLit(cols) => {
                let mut columns: Vec<(String, Vec<Value>)> = Vec::new();

                for (name, col_expr) in cols {
                    let col_val = self.eval_expr(col_expr)?;
                    let values = match col_val {
                        Value::Vector(vec_data) => vec_data.iter().cloned().collect(),
                        single => vec![single],
                    };
                    columns.push((name.clone(), values));
                }

                let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&columns)?;
                Ok(Value::DataFrame { frame, na_reasons })
            }

            ExprKind::MatrixLit { rows } => {
                let r_count = rows.len();
                let c_count = rows.first().map(|r| r.len()).unwrap_or(0);
                let mut data = Vec::with_capacity(r_count * c_count);

                for row in rows {
                    for cell in row {
                        let cell_val = self.eval_expr(cell)?;
                        data.push(cell_val.as_f64().unwrap_or(0.0));
                    }
                }

                Ok(Value::Matrix {
                    rows: r_count,
                    cols: c_count,
                    data: std::sync::Arc::new(data),
                })
            }

            ExprKind::RecordLit(fields) => {
                let mut map = std::collections::BTreeMap::new();
                for (name, val_expr) in fields {
                    let val = self.eval_expr_ctx(val_expr, col_ctx)?;
                    map.insert(name.clone(), val);
                }
                Ok(Value::Record(std::sync::Arc::new(map)))
            }

            ExprKind::StructLit { name, fields } => {
                let mut map = std::collections::BTreeMap::new();
                for (f_name, f_expr) in fields {
                    let val = self.eval_expr_ctx(f_expr, col_ctx)?;
                    map.insert(f_name.clone(), val);
                }
                Ok(Value::Struct {
                    name: name.clone(),
                    fields: std::sync::Arc::new(map),
                })
            }

            ExprKind::FieldAccess { target, field } => {
                let target_val = self.eval_expr_ctx(target, col_ctx)?;
                match target_val {
                    Value::Record(map) => {
                        map.get(field).cloned().ok_or_else(|| {
                            Diagnostic::compute_error("C0102", format!("Field `{field}` not found in record"))
                        })
                    }
                    Value::Struct { ref name, ref fields } => {
                        if let Some(v) = fields.get(field) {
                            Ok(v.clone())
                        } else {
                            let method_key = format!("{}::{}", name, field);
                            if let Some(m) = self.env.get(&method_key) {
                                Ok(m)
                            } else {
                                Err(Diagnostic::compute_error("C0102", format!("Field or method `{field}` not found in struct `{name}`")))
                            }
                        }
                    }
                    _ => Err(Diagnostic::compute_error("C0202", format!("Cannot access field `{field}` on non-record/non-struct value"))),
                }
            }

            ExprKind::Index { target, indices } => {
                let target_val = self.eval_expr_ctx(target, col_ctx)?;
                self.eval_indexing(target_val, indices, col_ctx)
            }

            ExprKind::Binary { op, lhs, rhs } => {
                let left = self.eval_expr_ctx(lhs, col_ctx)?;
                let right = self.eval_expr_ctx(rhs, col_ctx)?;
                self.eval_binary_op(*op, left, right)
            }

            ExprKind::UnaryNeg(inner) => {
                let val = self.eval_expr_ctx(inner, col_ctx)?;
                match val {
                    Value::I64(n) => Ok(Value::I64(-n)),
                    Value::F64(x) => Ok(Value::F64(-x)),
                    Value::NA(r) => Ok(Value::NA(r)),
                    // Fast path only when the Column is *already* Float64 -- `-[1, 2]`
                    // (an Int64 Vector) must still come back as Int64, and
                    // `as_f64_view()` would silently cast it to Float64 first, breaking
                    // that (an Int64 literal is a real vector in a Vector[i64], so this
                    // isn't just a hypothetical). Int64/mixed/NA-containing vectors fall
                    // through to the boxed loop below, unchanged.
                    Value::Vector(v) if v.null_count() == 0 && v.column().dtype() == &DataType::Float64 => {
                        let view = v.as_f64_view()?;
                        let base = view.as_slice();
                        let data: Vec<f64> = if base.len() >= PARALLEL_THRESHOLD {
                            base.par_iter().map(|&x| -x).collect()
                        } else {
                            base.iter().map(|&x| -x).collect()
                        };
                        Ok(Value::Vector(VectorData::from_f64(data)))
                    }
                    Value::Vector(v) => {
                        let mut res = Vec::new();
                        for item in v.iter().cloned() {
                            match item {
                                Value::I64(n) => res.push(Value::I64(-n)),
                                Value::F64(x) => res.push(Value::F64(-x)),
                                other => res.push(other),
                            }
                        }
                        Ok(Value::Vector(VectorData::from_values(res)))
                    }
                    _ => Err(Diagnostic::compute_error("C0202", "Unary `-` expects numeric operand")),
                }
            }

            ExprKind::UnaryNot(inner) => {
                let val = self.eval_expr_ctx(inner, col_ctx)?;
                match val {
                    Value::Bool(b) => Ok(Value::Bool(!b)),
                    Value::NA(r) => Ok(Value::NA(r)), // Kleene: !NA is NA
                    // `!is_na(col(...))` / `!(col(x) > 5)` etc: the operand is itself a
                    // deferred predicate (no DataFrame to evaluate against yet), so stay
                    // deferred too rather than erroring — `filter()` resolves the whole
                    // tree into one BooleanChunked mask.
                    v if is_predicate(&v) => Ok(Value::NotPredicate(Box::new(v))),
                    _ => Err(Diagnostic::compute_error("C0202", "Unary `!` expects boolean operand")),
                }
            }

            ExprKind::Pipe { expr: src, target } => {
                let val = self.eval_expr(src)?;
                match &target.kind {
                    ExprKind::Call { callee, args } => {
                        let arg_ctx = col_ctx || callee_name(callee).is_some_and(is_column_context_verb);
                        let mut call_args = vec![val];
                        for arg in args {
                            call_args.push(self.eval_expr_ctx(arg, arg_ctx)?);
                        }
                        let callee_val = self.eval_expr(callee)?;
                        self.call_value(callee_val, call_args)
                    }
                    ExprKind::Ident(name) => {
                        let callee_val = self.env.get(name).ok_or_else(|| {
                            Diagnostic::compute_error("C0101", format!("Undefined function `{}`", name))
                        })?;
                        self.call_value(callee_val, vec![val])
                    }
                    ExprKind::Path(_) => {
                        let callee_val = self.eval_expr(target)?;
                        self.call_value(callee_val, vec![val])
                    }
                    _ => self.eval_expr(target),
                }
            }

            ExprKind::Call { callee, args } => {
                if let ExprKind::FieldAccess { target, field } = &callee.kind {
                    let target_val = self.eval_expr(target)?;
                    let is_record_fn = match &target_val {
                        Value::Record(map) => map.get(field).map(|v| matches!(v, Value::NativeFn(_) | Value::NativeFnCtx(_) | Value::Closure { .. })).unwrap_or(false),
                        _ => false,
                    };

                    if is_record_fn {
                        if let Value::Record(map) = target_val {
                            let fn_val = map.get(field).unwrap().clone();
                            let mut evaluated_args = Vec::with_capacity(args.len());
                            for a in args {
                                evaluated_args.push(self.eval_expr_ctx(a, false)?);
                            }
                            return self.call_value(fn_val, evaluated_args);
                        }
                    } else if let Value::Struct { ref name, .. } = target_val {
                        let method_key = format!("{}::{}", name, field);
                        let fn_val = self.env.get(&method_key).or_else(|| self.env.get(field)).ok_or_else(|| {
                            Diagnostic::compute_error("C0101", format!("Undefined method `{field}` on struct `{name}`"))
                        })?;
                        let mut evaluated_args = Vec::with_capacity(args.len() + 1);
                        evaluated_args.push(target_val);
                        for a in args {
                            evaluated_args.push(self.eval_expr_ctx(a, false)?);
                        }
                        return self.call_value(fn_val, evaluated_args);
                    } else {
                        let fn_val = self.env.get(field).ok_or_else(|| {
                            Diagnostic::compute_error("C0101", format!("Undefined function or method `{field}`"))
                        })?;
                        let arg_ctx = is_column_context_verb(field);
                        let mut evaluated_args = Vec::with_capacity(args.len() + 1);
                        evaluated_args.push(target_val);
                        for a in args {
                            evaluated_args.push(self.eval_expr_ctx(a, arg_ctx)?);
                        }
                        return self.call_value(fn_val, evaluated_args);
                    }
                }

                let callee_val = self.eval_expr(callee)?;
                let arg_ctx = col_ctx || callee_name(callee).is_some_and(is_column_context_verb);
                let mut evaluated_args = Vec::with_capacity(args.len());
                for a in args {
                    evaluated_args.push(self.eval_expr_ctx(a, arg_ctx)?);
                }
                self.call_value(callee_val, evaluated_args)
            }

            ExprKind::Block { stmts, expr: opt_expr } => {
                self.env.push_scope();
                for s in stmts {
                    self.eval_stmt(s)?;
                    // A `return` fired inside this statement (directly, or nested
                    // inside an `if`/`match` this statement evaluated) -- stop running
                    // the rest of this block, and skip its own trailing expression too;
                    // `pending_return` deliberately stays set (not `.take()`-n here) so
                    // an outer Block containing this one also observes and short-
                    // circuits, all the way up to the function-call boundary that
                    // actually clears it.
                    if let Some(val) = &self.pending_return {
                        let val = val.clone();
                        self.env.pop_scope();
                        return Ok(val);
                    }
                }
                let res = if let Some(e) = opt_expr {
                    self.eval_expr(e)?
                } else {
                    Value::Unit
                };
                self.env.pop_scope();
                Ok(res)
            }

            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond_val = self.eval_expr(cond)?;
                if cond_val.as_bool() == Some(true) {
                    self.eval_expr(then_branch)
                } else if let Some(el) = else_branch {
                    self.eval_expr(el)
                } else {
                    Ok(Value::Unit)
                }
            }

            ExprKind::While { cond, body } => {
                // A native Rust loop -- no recursion of any kind, so unlike function
                // calls this never risks the native stack regardless of iteration
                // count (TODO.md Fase 7).
                while self.eval_expr(cond)?.as_bool() == Some(true) {
                    self.eval_expr(body)?;
                    if self.pending_return.is_some() {
                        // Same short-circuit `Block` already does: a `return` inside
                        // the loop body must stop the loop too, not just that
                        // iteration's block.
                        break;
                    }
                }
                Ok(Value::Unit)
            }

            ExprKind::For { var, start, end, body } => {
                // Native Rust loop — no recursion, no heap allocation per iteration.
                // The loop variable is bound in its own pushed scope; each iteration
                // overwrites the same slot (no alloc). The scope is popped after the
                // loop so the variable does not leak into the surrounding env.
                let start_val = self.eval_expr(start)?;
                let end_val = self.eval_expr(end)?;
                let (start_i, end_i) = match (&start_val, &end_val) {
                    (Value::I64(s), Value::I64(e)) => (*s, *e),
                    _ => return Err(Diagnostic::compute_error(
                        "C0104",
                        format!("for loop range requires integer bounds, got `{}` and `{}`",
                            start_val.type_name(), end_val.type_name()),
                    )),
                };
                self.env.push_scope();
                self.env.set(var.clone(), Value::I64(start_i));
                for i in start_i..end_i {
                    self.env.set(var.clone(), Value::I64(i));
                    self.eval_expr(body)?;
                    if self.pending_return.is_some() {
                        break;
                    }
                }
                self.env.pop_scope();
                Ok(Value::Unit)
            }

            ExprKind::Match { expr: target, arms } => {
                let target_val = self.eval_expr(target)?;

                for arm in arms {
                    let mut arm_env = self.env.clone();
                    let matches = match_pattern(&arm.pattern, &target_val, &mut arm_env);

                    if matches {
                        // Check optional guard
                        if let Some(guard) = &arm.guard {
                            let old_env = std::mem::replace(&mut self.env, arm_env.clone());
                            let guard_res = self.eval_expr(guard)?;
                            self.env = old_env;

                            if guard_res.as_bool() != Some(true) {
                                continue;
                            }
                        }

                        // Evaluate body in matched environment
                        let old_env = std::mem::replace(&mut self.env, arm_env);
                        let body_res = self.eval_expr(&arm.body);
                        self.env = old_env;
                        return body_res;
                    }
                }

                Ok(Value::Unit)
            }

            ExprKind::Formula { response, terms } => {
                let resp_str = match &response.kind {
                    ExprKind::Ident(s) => s.clone(),
                    _ => format!("{:?}", response.kind),
                };
                let mut term_strs = Vec::new();
                for t in terms {
                    extract_formula_term(t, &mut term_strs);
                }
                Ok(Value::Formula {
                    response: resp_str,
                    terms: term_strs,
                })
            }

            ExprKind::Lambda { params, body } => {
                Ok(Value::Closure {
                    params: params.clone(),
                    body: (**body).clone(),
                    env: self.env.clone(),
                })
            }

            ExprKind::Placeholder => Ok(Value::Unit),

            ExprKind::Range { start, end, inclusive } => {
                let start_val = self.eval_expr(start)?;
                let end_val = self.eval_expr(end)?;
                match (start_val, end_val) {
                    (Value::I64(s), Value::I64(e)) => Ok(Value::Range {
                        start: s,
                        end: e,
                        inclusive: *inclusive,
                    }),
                    (s, e) => Err(Diagnostic::compute_error(
                        "C0104",
                        format!("Range requires integer bounds, got `{}` and `{}`", s.type_name(), e.type_name()),
                    )),
                }
            }

            ExprKind::Path(segments) => {
                if segments.len() == 2 && (segments[0] == "NAReason" || segments[0] == "NAReasons") {
                    Ok(Value::NA(Some(segments[1].clone())))
                } else if let Some(val) = crate::modules::lookup_module_item(segments) {
                    Ok(val)
                } else {
                    let full_name = segments.join("::");
                    self.env.get(&full_name).or_else(|| {
                        if col_ctx { Some(Value::ColRef(full_name.clone())) } else { None }
                    }).ok_or_else(|| {
                        Diagnostic::compute_error("C0101", format!("Undefined path `{}`", full_name))
                    })
                }
            }
        }
    }

    /// Evaluates `expr` knowing its value will become the enclosing function call's own
    /// return value with no further computation on top -- a *tail position*. Only
    /// `Block`'s trailing expression, `If`'s two branches, and `Match`'s arm bodies
    /// pass this flag through to their own final sub-expression (mirroring exactly
    /// where GHL's grammar allows a function body to "end"); a `Call`/`Pipe`-to-call
    /// found there, instead of being invoked immediately (which is what fully-`Ok`-ing
    /// out via `eval_expr` would do, recursing into Rust), is reported back as a
    /// `TailCall` for `call_value`'s trampoline to run as a loop iteration instead --
    /// this is what lets self/mutual GHL recursion run at unbounded depth without
    /// growing the native stack (TODO.md Fase 7's recursion-depth finding).
    ///
    /// `return expr;` is deliberately NOT included: `StmtKind::Return` still evaluates
    /// `expr` with the ordinary, non-tail `eval_expr`, so `return recurse(n - 1);`
    /// does not get this optimization, only the `return`-free idiomatic form
    /// (`if base_case { v } else { recurse(...) }`) does -- see TODO.md for why this
    /// scope line was drawn deliberately, not discovered as a limitation later.
    /// Everywhere else (`Binary`, `UnaryNeg`, literals, ...) a nested call can never
    /// itself be the function's return value without more computation happening on top
    /// (`1 + recurse(n - 1)` needs `recurse`'s result before `+` can run), so those
    /// fall through to plain `eval_expr` unchanged, non-tail, same stack cost as today.
    fn eval_expr_tail(&mut self, expr: &Expr) -> Result<TailOutcome, Diagnostic> {
        match &expr.kind {
            ExprKind::Block { stmts, expr: opt_expr } => {
                self.env.push_scope();
                for s in stmts {
                    self.eval_stmt(s)?;
                    if let Some(val) = &self.pending_return {
                        let val = val.clone();
                        self.env.pop_scope();
                        return Ok(TailOutcome::Value(val));
                    }
                }
                let outcome = match opt_expr {
                    Some(e) => self.eval_expr_tail(e)?,
                    None => TailOutcome::Value(Value::Unit),
                };
                self.env.pop_scope();
                Ok(outcome)
            }

            ExprKind::If { cond, then_branch, else_branch } => {
                let cond_val = self.eval_expr(cond)?;
                if cond_val.as_bool() == Some(true) {
                    self.eval_expr_tail(then_branch)
                } else if let Some(el) = else_branch {
                    self.eval_expr_tail(el)
                } else {
                    Ok(TailOutcome::Value(Value::Unit))
                }
            }

            ExprKind::Match { expr: target, arms } => {
                let target_val = self.eval_expr(target)?;

                for arm in arms {
                    let mut arm_env = self.env.clone();
                    let matches = match_pattern(&arm.pattern, &target_val, &mut arm_env);

                    if matches {
                        if let Some(guard) = &arm.guard {
                            let old_env = std::mem::replace(&mut self.env, arm_env.clone());
                            let guard_res = self.eval_expr(guard)?;
                            self.env = old_env;

                            if guard_res.as_bool() != Some(true) {
                                continue;
                            }
                        }

                        let old_env = std::mem::replace(&mut self.env, arm_env);
                        let body_res = self.eval_expr_tail(&arm.body);
                        self.env = old_env;
                        return body_res;
                    }
                }

                Ok(TailOutcome::Value(Value::Unit))
            }

            ExprKind::Call { callee, args } => {
                if let ExprKind::FieldAccess { target, field } = &callee.kind {
                    let target_val = self.eval_expr(target)?;
                    let is_record_fn = match &target_val {
                        Value::Record(map) => map.get(field).map(|v| matches!(v, Value::NativeFn(_) | Value::NativeFnCtx(_) | Value::Closure { .. })).unwrap_or(false),
                        _ => false,
                    };

                    if is_record_fn {
                        if let Value::Record(map) = target_val {
                            let fn_val = map.get(field).unwrap().clone();
                            let mut evaluated_args = Vec::with_capacity(args.len());
                            for a in args {
                                evaluated_args.push(self.eval_expr_ctx(a, false)?);
                            }
                            return Ok(TailOutcome::TailCall { callee: fn_val, args: evaluated_args });
                        }
                    } else if let Value::Struct { ref name, .. } = target_val {
                        let method_key = format!("{}::{}", name, field);
                        let fn_val = self.env.get(&method_key).or_else(|| self.env.get(field)).ok_or_else(|| {
                            Diagnostic::compute_error("C0101", format!("Undefined method `{field}` on struct `{name}`"))
                        })?;
                        let mut evaluated_args = Vec::with_capacity(args.len() + 1);
                        evaluated_args.push(target_val);
                        for a in args {
                            evaluated_args.push(self.eval_expr_ctx(a, false)?);
                        }
                        return Ok(TailOutcome::TailCall { callee: fn_val, args: evaluated_args });
                    } else {
                        let fn_val = self.env.get(field).ok_or_else(|| {
                            Diagnostic::compute_error("C0101", format!("Undefined function or method `{field}`"))
                        })?;
                        let arg_ctx = is_column_context_verb(field);
                        let mut evaluated_args = Vec::with_capacity(args.len() + 1);
                        evaluated_args.push(target_val);
                        for a in args {
                            evaluated_args.push(self.eval_expr_ctx(a, arg_ctx)?);
                        }
                        return Ok(TailOutcome::TailCall { callee: fn_val, args: evaluated_args });
                    }
                }

                let callee_val = self.eval_expr(callee)?;
                let arg_ctx = callee_name(callee).is_some_and(is_column_context_verb);
                let mut evaluated_args = Vec::with_capacity(args.len());
                for a in args {
                    evaluated_args.push(self.eval_expr_ctx(a, arg_ctx)?);
                }
                Ok(TailOutcome::TailCall { callee: callee_val, args: evaluated_args })
            }

            ExprKind::Pipe { expr: src, target } => {
                let val = self.eval_expr(src)?;
                match &target.kind {
                    ExprKind::Call { callee, args } => {
                        let arg_ctx = callee_name(callee).is_some_and(is_column_context_verb);
                        let mut call_args = vec![val];
                        for arg in args {
                            call_args.push(self.eval_expr_ctx(arg, arg_ctx)?);
                        }
                        let callee_val = self.eval_expr(callee)?;
                        Ok(TailOutcome::TailCall { callee: callee_val, args: call_args })
                    }
                    ExprKind::Ident(name) => {
                        let callee_val = self.env.get(name).ok_or_else(|| {
                            Diagnostic::compute_error("C0101", format!("Undefined function `{}`", name))
                        })?;
                        Ok(TailOutcome::TailCall { callee: callee_val, args: vec![val] })
                    }
                    ExprKind::Path(_) => {
                        let callee_val = self.eval_expr(target)?;
                        Ok(TailOutcome::TailCall { callee: callee_val, args: vec![val] })
                    }
                    _ => Ok(TailOutcome::Value(self.eval_expr(target)?)),
                }
            }

            _ => Ok(TailOutcome::Value(self.eval_expr(expr)?)),
        }
    }

    pub(crate) fn call_value(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, Diagnostic> {
        match callee {
            Value::NativeFn(func) => func(args),
            Value::NativeFnCtx(func) => func(self, args),
            Value::Closure { params, body, env } => {
                // Trampoline: a self/mutual-recursive GHL call in tail position (see
                // `eval_expr_tail`) comes back as `TailOutcome::TailCall` instead of
                // being invoked via a nested `call_value` -- swapping in the next
                // closure's params/body/env and looping here, instead of recursing
                // into Rust, is what lets GHL recursion run at unbounded depth without
                // growing the native stack (TODO.md Fase 7's recursion-depth finding).
                let caller_env = std::mem::replace(&mut self.env, env);
                let mut cur_params = params;
                let mut cur_body = body;
                let mut cur_args = args;
                // Inherit any newly defined globals into the closure environment once before loop
                if let Some(global_scope) = caller_env.scopes.first() {
                    for (k, v) in global_scope {
                        if self.env.get(k).is_none() {
                            self.env.set(k.clone(), v.clone());
                        }
                    }
                }

                let result = loop {
                    self.env.push_scope();
                    for (p, a) in cur_params.iter().zip(cur_args.into_iter()) {
                        self.env.set(p.clone(), a);
                    }

                    match self.eval_expr_tail(&cur_body) {
                        Err(e) => {
                            self.env.pop_scope();
                            break Err(e);
                        }
                        Ok(TailOutcome::Value(v)) => {
                            self.env.pop_scope();
                            break Ok(v);
                        }
                        Ok(TailOutcome::TailCall { callee: Value::Closure { params: p2, body: b2, env: mut e2 }, args: next_args }) => {
                            self.env.pop_scope();
                            if let Some(global_scope) = caller_env.scopes.first() {
                                for (k, v) in global_scope {
                                    if e2.get(k).is_none() {
                                        e2.set(k.clone(), v.clone());
                                    }
                                }
                            }
                            self.env = e2;
                            cur_params = p2;
                            cur_body = b2;
                            cur_args = next_args;
                        }
                        Ok(TailOutcome::TailCall { callee: other, args: next_args }) => {
                            self.env.pop_scope();
                            self.env = caller_env.clone();
                            break self.call_value(other, next_args);
                        }
                    }
                };
                self.env = caller_env;
                // This call is the function boundary `pending_return` was waiting for
                // -- consume it here (whether or not a `return` actually fired) so it
                // never leaks into the caller's own remaining statements.
                self.pending_return = None;
                result
            }
            other => Err(Diagnostic::compute_error(
                "C0203",
                format!("Value `{}` is not callable as a function", other.type_name()),
            )),
        }
    }

    fn eval_indexing(
        &mut self,
        target: Value,
        indices: &[IndexSpec],
        col_ctx: bool,
    ) -> Result<Value, Diagnostic> {
        match target {
            Value::Vector(vd) => {
                if indices.len() != 1 {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("Vector indexing requires exactly 1 index, got {}", indices.len()),
                    ));
                }
                match &indices[0] {
                    IndexSpec::All => Ok(Value::Vector(vd)),
                    IndexSpec::Expr(e) => {
                        let idx_val = self.eval_expr_ctx(e, col_ctx)?;
                        match idx_val {
                            Value::I64(i) => {
                                if i < 0 || (i as usize) >= vd.len() {
                                    return Err(Diagnostic::compute_error(
                                        "C0203",
                                        format!("Index out of bounds: index {i} for vector of length {}", vd.len()),
                                    ));
                                }
                                Ok(vd.value_at(i as usize).unwrap_or(Value::NA(None)))
                            }
                            Value::Vector(idx_vec) => {
                                if idx_vec.column().dtype() == &DataType::Boolean {
                                    if idx_vec.len() != vd.len() {
                                        return Err(Diagnostic::compute_error(
                                            "C0202",
                                            format!(
                                                "Boolean mask length ({}) must match vector length ({})",
                                                idx_vec.len(),
                                                vd.len()
                                            ),
                                        ));
                                    }
                                    if let Ok(ca) = idx_vec.column().bool() {
                                        if let Ok(filtered_col) = vd.column().filter(ca) {
                                            let mut new_reasons = crate::na_reasons::NaReasonTable::new();
                                            if vd.null_count() > 0 {
                                                let mut new_row = 0;
                                                for old_row in 0..vd.len() {
                                                    if ca.get(old_row) == Some(true) {
                                                        if let Some(r) = vd.na_reasons().get(crate::vector_data::VECTOR_COL, old_row) {
                                                            new_reasons.set(crate::vector_data::VECTOR_COL, new_row, r);
                                                        }
                                                        new_row += 1;
                                                    }
                                                }
                                            }
                                            return Ok(Value::Vector(VectorData::from_column_and_reasons(
                                                filtered_col,
                                                std::sync::Arc::new(new_reasons),
                                            )));
                                        }
                                    }
                                    let mut gathered = Vec::new();
                                    for (i, m) in idx_vec.iter().enumerate() {
                                        if m.as_bool() == Some(true) {
                                            gathered.push(vd.value_at(i).unwrap_or(Value::NA(None)));
                                        }
                                    }
                                    Ok(Value::Vector(VectorData::from_values(gathered)))
                                } else {
                                    let mut gathered = Vec::with_capacity(idx_vec.len());
                                    for item in idx_vec.iter() {
                                        let i = item.as_i64().ok_or_else(|| {
                                            Diagnostic::compute_error("C0202", "Vector index must be integer or boolean mask")
                                        })?;
                                        if i < 0 || (i as usize) >= vd.len() {
                                            return Err(Diagnostic::compute_error(
                                                "C0203",
                                                format!("Index out of bounds: index {i} for vector of length {}", vd.len()),
                                            ));
                                        }
                                        gathered.push(vd.value_at(i as usize).unwrap_or(Value::NA(None)));
                                    }
                                    Ok(Value::Vector(VectorData::from_values(gathered)))
                                }
                            }
                            other => Err(Diagnostic::compute_error(
                                "C0202",
                                format!("Cannot index vector with `{}`", other.type_name()),
                            )),
                        }
                    }
                    IndexSpec::Range { start, end, inclusive } => {
                        let s = if let Some(st) = start {
                            let val = self.eval_expr_ctx(st, col_ctx)?;
                            val.as_i64().ok_or_else(|| {
                                Diagnostic::compute_error("C0201", "Range start must be an integer")
                            })?
                        } else {
                            0
                        };
                        let e = if let Some(ed) = end {
                            let val = self.eval_expr_ctx(ed, col_ctx)?;
                            let ed_i = val.as_i64().ok_or_else(|| {
                                Diagnostic::compute_error("C0201", "Range end must be an integer")
                            })?;
                            if *inclusive { ed_i + 1 } else { ed_i }
                        } else {
                            vd.len() as i64
                        };
                        if s < 0 || (s as usize) > vd.len() || e < 0 || (e as usize) > vd.len() || s > e {
                            return Err(Diagnostic::compute_error(
                                "C0203",
                                format!("Range slice [{s}..{e}] out of bounds for vector of length {}", vd.len()),
                            ));
                        }
                        let start_u = s as usize;
                        let len_u = (e - s) as usize;
                        Ok(Value::Vector(vd.slice(start_u, len_u)))
                    }
                }
            }

            Value::Matrix { rows, cols, data } => {
                if indices.len() != 2 {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("Matrix indexing requires exactly 2 indices [row, col], got {}", indices.len()),
                    ));
                }

                let resolve_dim = |interp: &mut Self, spec: &IndexSpec, dim_len: usize, dim_name: &str| -> Result<(bool, usize, usize), Diagnostic> {
                    match spec {
                        IndexSpec::All => Ok((false, 0, dim_len)),
                        IndexSpec::Expr(e) => {
                            let val = interp.eval_expr_ctx(e, col_ctx)?;
                            let i = val.as_i64().ok_or_else(|| {
                                Diagnostic::compute_error("C0201", format!("Matrix {dim_name} index must be an integer"))
                            })?;
                            if i < 0 || (i as usize) >= dim_len {
                                return Err(Diagnostic::compute_error(
                                    "C0203",
                                    format!("Matrix {dim_name} index {i} out of bounds for dimension of size {dim_len}"),
                                ));
                            }
                            Ok((true, i as usize, (i as usize) + 1))
                        }
                        IndexSpec::Range { start, end, inclusive } => {
                            let s = if let Some(st) = start {
                                let val = interp.eval_expr_ctx(st, col_ctx)?;
                                val.as_i64().ok_or_else(|| {
                                    Diagnostic::compute_error("C0201", format!("Matrix {dim_name} range start must be an integer"))
                                })?
                            } else {
                                0
                            };
                            let e = if let Some(ed) = end {
                                let val = interp.eval_expr_ctx(ed, col_ctx)?;
                                let ed_i = val.as_i64().ok_or_else(|| {
                                    Diagnostic::compute_error("C0201", format!("Matrix {dim_name} range end must be an integer"))
                                })?;
                                if *inclusive { ed_i + 1 } else { ed_i }
                            } else {
                                dim_len as i64
                            };
                            if s < 0 || (s as usize) > dim_len || e < 0 || (e as usize) > dim_len || s > e {
                                return Err(Diagnostic::compute_error(
                                    "C0203",
                                    format!("Matrix {dim_name} slice [{s}..{e}] out of bounds for dimension of size {dim_len}"),
                                ));
                            }
                            Ok((false, s as usize, e as usize))
                        }
                    }
                };

                let (r_scalar, r_start, r_end) = resolve_dim(self, &indices[0], rows, "row")?;
                let (c_scalar, c_start, c_end) = resolve_dim(self, &indices[1], cols, "col")?;

                if r_scalar && c_scalar {
                    Ok(Value::F64(data[r_start * cols + c_start]))
                } else if r_scalar {
                    // Row vector
                    let row_data = data[r_start * cols + c_start .. r_start * cols + c_end].to_vec();
                    Ok(Value::Vector(VectorData::from_f64(row_data)))
                } else if c_scalar {
                    // Column vector
                    let mut col_data = Vec::with_capacity(r_end - r_start);
                    for r in r_start..r_end {
                        col_data.push(data[r * cols + c_start]);
                    }
                    Ok(Value::Vector(VectorData::from_f64(col_data)))
                } else {
                    // Sub-matrix
                    let new_rows = r_end - r_start;
                    let new_cols = c_end - c_start;
                    let mut sub_data = Vec::with_capacity(new_rows * new_cols);
                    for r in r_start..r_end {
                        sub_data.extend_from_slice(&data[r * cols + c_start .. r * cols + c_end]);
                    }
                    Ok(Value::Matrix {
                        rows: new_rows,
                        cols: new_cols,
                        data: std::sync::Arc::new(sub_data),
                    })
                }
            }

            Value::String(s) => {
                if indices.len() != 1 {
                    return Err(Diagnostic::compute_error(
                        "C0201",
                        format!("String indexing requires exactly 1 index, got {}", indices.len()),
                    ));
                }
                let chars: Vec<char> = s.chars().collect();
                match &indices[0] {
                    IndexSpec::All => Ok(Value::String(s)),
                    IndexSpec::Expr(e) => {
                        let val = self.eval_expr_ctx(e, col_ctx)?;
                        let i = val.as_i64().ok_or_else(|| {
                            Diagnostic::compute_error("C0201", "String index must be an integer")
                        })?;
                        if i < 0 || (i as usize) >= chars.len() {
                            return Err(Diagnostic::compute_error(
                                "C0203",
                                format!("Index out of bounds: index {i} for string of length {}", chars.len()),
                            ));
                        }
                        Ok(Value::String(chars[i as usize].to_string()))
                    }
                    IndexSpec::Range { start, end, inclusive } => {
                        let st = if let Some(s_expr) = start {
                            let val = self.eval_expr_ctx(s_expr, col_ctx)?;
                            val.as_i64().ok_or_else(|| {
                                Diagnostic::compute_error("C0201", "Range start must be an integer")
                            })?
                        } else {
                            0
                        };
                        let ed = if let Some(e_expr) = end {
                            let val = self.eval_expr_ctx(e_expr, col_ctx)?;
                            let ed_i = val.as_i64().ok_or_else(|| {
                                Diagnostic::compute_error("C0201", "Range end must be an integer")
                            })?;
                            if *inclusive { ed_i + 1 } else { ed_i }
                        } else {
                            chars.len() as i64
                        };
                        if st < 0 || (st as usize) > chars.len() || ed < 0 || (ed as usize) > chars.len() || st > ed {
                            return Err(Diagnostic::compute_error(
                                "C0203",
                                format!("Range slice [{st}..{ed}] out of bounds for string of length {}", chars.len()),
                            ));
                        }
                        let substr: String = chars[(st as usize)..(ed as usize)].iter().collect();
                        Ok(Value::String(substr))
                    }
                }
            }

            other => Err(Diagnostic::compute_error(
                "C0202",
                format!("Cannot index value of type `{}` with `[...]`", other.type_name()),
            )),
        }
    }

    fn eval_binary_op(&mut self, op: BinaryOp, left: Value, right: Value) -> Result<Value, Diagnostic> {
        // DataFrame column expressions: col("id") > 1
        if let Value::ColRef(col) = left {
            return Ok(Value::ColPredicate {
                col,
                op,
                rhs: Box::new(right),
            });
        }
        if let Value::ColRef(col) = right {
            let flipped_op = match op {
                BinaryOp::Lt => BinaryOp::Gt,
                BinaryOp::LtEq => BinaryOp::GtEq,
                BinaryOp::Gt => BinaryOp::Lt,
                BinaryOp::GtEq => BinaryOp::LtEq,
                BinaryOp::Eq => BinaryOp::Eq,
                BinaryOp::NotEq => BinaryOp::NotEq,
                _ => op,
            };
            return Ok(Value::ColPredicate {
                col,
                op: flipped_op,
                rhs: Box::new(left),
            });
        }

        // `score > 75.0 && !is_na(category)`: combining two deferred predicates (or one
        // predicate with anything else) stays deferred rather than going through the
        // Kleene Bool `&&`/`||` below, which would just error on a non-Bool operand.
        // Two real Bools/NAs never hit this branch (`is_predicate` is false for both),
        // so ordinary `&&`/`||` semantics are unaffected.
        if matches!(op, BinaryOp::And | BinaryOp::Or) && (is_predicate(&left) || is_predicate(&right)) {
            return Ok(match op {
                BinaryOp::And => Value::AndPredicate(Box::new(left), Box::new(right)),
                BinaryOp::Or => Value::OrPredicate(Box::new(left), Box::new(right)),
                _ => unreachable!(),
            });
        }

        match op {
            // Matrix solve: A \ b
            BinaryOp::MatSolve => {
                match (left, right) {
                    (Value::Matrix { rows, cols: _, data }, Value::Vector(v)) => {
                        let mut b_floats = Vec::with_capacity(v.len());
                        for item in v.iter() {
                            b_floats.push(item.as_f64().unwrap_or(0.0));
                        }
                        let x = MatrixOps::solve(rows, &data, &b_floats)?;
                        Ok(Value::Vector(VectorData::from_f64(x)))
                    }
                    (l, r) => Err(Diagnostic::statistical_error(
                        "S0412",
                        format!("Operator `\\` expects Matrix LHS and Vector RHS, found `{}` and `{}`", l.type_name(), r.type_name()),
                    )),
                }
            }

            // Element-wise Dot operations
            BinaryOp::DotMul | BinaryOp::DotAdd | BinaryOp::DotSub | BinaryOp::DotDiv => {
                let op_fn: fn(f64, f64) -> f64 = match op {
                    BinaryOp::DotMul => |a, b| a * b,
                    BinaryOp::DotAdd => |a, b| a + b,
                    BinaryOp::DotSub => |a, b| a - b,
                    BinaryOp::DotDiv => |a, b| a / b,
                    _ => unreachable!(),
                };

                match (left, right) {
                    (
                        Value::Matrix { rows: r1, cols: c1, data: d1 },
                        Value::Matrix { rows: r2, cols: c2, data: d2 },
                    ) => {
                        let (r, c, d) = MatrixOps::elementwise(r1, c1, &d1, r2, c2, &d2, op_fn, "matrix op")?;
                        Ok(Value::Matrix { rows: r, cols: c, data: std::sync::Arc::new(d) })
                    }
                    (Value::Vector(v1), Value::Vector(v2)) => vector_elementwise_op(&v1, &v2, op_fn),
                    (l, r) => Err(Diagnostic::compute_error(
                        "C0202",
                        format!("Element-wise op requires Vectors or Matrices, found `{}` and `{}`", l.type_name(), r.type_name()),
                    )),
                }
            }

            // Standard Arithmetic (+, -, *, /, %, ^)
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                // Kleene NA propagation: any operation with NA yields NA (preserving reason)
                if let Value::NA(r) = left {
                    return Ok(Value::NA(r));
                }
                if let Value::NA(r) = right {
                    return Ok(Value::NA(r));
                }

                // String concatenation
                if op == BinaryOp::Add {
                    if let (Value::String(s1), Value::String(s2)) = (&left, &right) {
                        return Ok(Value::String(format!("{}{}", s1, s2)));
                    }
                }

                // True matrix product `A * B` (Caso 1.2, Suite 01) -- distinct from the
                // element-wise `.+`/`.-`/`.*`/`./` handled separately below under
                // `BinaryOp::DotMul` etc. `MatrixOps::mul` dispatches to faer's `*`
                // operator, which uses its own blocked multithreaded GEMM kernel.
                if op == BinaryOp::Mul {
                    if let (
                        Value::Matrix { rows: r1, cols: c1, data: d1 },
                        Value::Matrix { rows: r2, cols: c2, data: d2 },
                    ) = (&left, &right)
                    {
                        let (r, c, d) = MatrixOps::mul(*r1, *c1, d1, *r2, *c2, d2)?;
                        return Ok(Value::Matrix { rows: r, cols: c, data: std::sync::Arc::new(d) });
                    }
                }

                // Integer arithmetic
                if let (Value::I64(a), Value::I64(b)) = (&left, &right) {
                    return match op {
                        BinaryOp::Add => Ok(Value::I64(a + b)),
                        BinaryOp::Sub => Ok(Value::I64(a - b)),
                        BinaryOp::Mul => Ok(Value::I64(a * b)),
                        BinaryOp::Div => Ok(Value::F64(*a as f64 / *b as f64)),
                        BinaryOp::Mod => Ok(Value::I64(a % b)),
                        BinaryOp::Pow => Ok(Value::I64(a.pow(*b as u32))),
                        _ => unreachable!(),
                    };
                }

                // Floating point arithmetic
                if let (Some(a), Some(b)) = (left.as_f64(), right.as_f64()) {
                    return match op {
                        BinaryOp::Add => Ok(Value::F64(a + b)),
                        BinaryOp::Sub => Ok(Value::F64(a - b)),
                        BinaryOp::Mul => Ok(Value::F64(a * b)),
                        BinaryOp::Div => Ok(Value::F64(a / b)),
                        BinaryOp::Mod => Ok(Value::F64(a % b)),
                        BinaryOp::Pow => Ok(Value::F64(a.powf(b))),
                        _ => unreachable!(),
                    };
                }

                // Vector op Vector (same length): `+`/`-`/`*`/`/` behave like their
                // elementwise `.+`/`.-`/`.*`/`./` counterparts -- see `vector_elementwise_op`
                // for why this doesn't collide with Matrix's `*` = real product design.
                // `%`/`^` between two Vectors are intentionally left alone (no `.%`/`.^`
                // operator exists either), so they still fall through to the final error.
                if let (Value::Vector(v1), Value::Vector(v2)) = (&left, &right) {
                    let op_fn: Option<fn(f64, f64) -> f64> = match op {
                        BinaryOp::Add => Some(|a, b| a + b),
                        BinaryOp::Sub => Some(|a, b| a - b),
                        BinaryOp::Mul => Some(|a, b| a * b),
                        BinaryOp::Div => Some(|a, b| a / b),
                        _ => None,
                    };
                    if let Some(op_fn) = op_fn {
                        return vector_elementwise_op(v1, v2, op_fn);
                    }
                }

                // Vector with scalar broadcasting
                if let (Value::Vector(v), scalar) = (&left, &right) {
                    if let Some(s) = scalar.as_f64() {
                        let mut res = Vec::with_capacity(v.len());
                        for item in v.iter() {
                            if item.is_na() {
                                res.push(item.clone());
                            } else if let Some(x) = item.as_f64() {
                                let calculated = match op {
                                    BinaryOp::Add => x + s,
                                    BinaryOp::Sub => x - s,
                                    BinaryOp::Mul => x * s,
                                    BinaryOp::Div => x / s,
                                    _ => x,
                                };
                                res.push(Value::F64(calculated));
                            }
                        }
                        return Ok(Value::Vector(VectorData::from_values(res)));
                    }
                }

                // Scalar with Vector broadcasting (reversed order) -- symmetric with the
                // branch above, but the computation direction must flip for non-commutative
                // ops: `5.0 - v` must give `[5.0-v[0], ...]`, not `[v[0]-5.0, ...]`.
                if let (scalar, Value::Vector(v)) = (&left, &right) {
                    if let Some(s) = scalar.as_f64() {
                        let mut res = Vec::with_capacity(v.len());
                        for item in v.iter() {
                            if item.is_na() {
                                res.push(item.clone());
                            } else if let Some(x) = item.as_f64() {
                                let calculated = match op {
                                    BinaryOp::Add => s + x,
                                    BinaryOp::Sub => s - x,
                                    BinaryOp::Mul => s * x,
                                    BinaryOp::Div => s / x,
                                    _ => x,
                                };
                                res.push(Value::F64(calculated));
                            }
                        }
                        return Ok(Value::Vector(VectorData::from_values(res)));
                    }
                }

                Err(Diagnostic::compute_error(
                    "C0202",
                    format!("Cannot apply `{:?}` to `{}` and `{}`", op, left.type_name(), right.type_name()),
                ))
            }

            // Kleene 3-Valued Logic: && and ||
            BinaryOp::And => {
                match (&left, &right) {
                    // false && anything is false!
                    (Value::Bool(false), _) | (_, Value::Bool(false)) => Ok(Value::Bool(false)),
                    // true && NA is NA!
                    (Value::Bool(true), Value::NA(r)) | (Value::NA(r), Value::Bool(true)) => Ok(Value::NA(r.clone())),
                    // NA && NA is NA!
                    (Value::NA(r), Value::NA(_)) => Ok(Value::NA(r.clone())),
                    // true && true is true!
                    (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
                    _ => Err(Diagnostic::compute_error("C0202", "`&&` requires boolean operands")),
                }
            }

            BinaryOp::Or => {
                match (&left, &right) {
                    // true || anything is true!
                    (Value::Bool(true), _) | (_, Value::Bool(true)) => Ok(Value::Bool(true)),
                    // false || NA is NA!
                    (Value::Bool(false), Value::NA(r)) | (Value::NA(r), Value::Bool(false)) => Ok(Value::NA(r.clone())),
                    // NA || NA is NA!
                    (Value::NA(r), Value::NA(_)) => Ok(Value::NA(r.clone())),
                    // false || false is false!
                    (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
                    _ => Err(Diagnostic::compute_error("C0202", "`||` requires boolean operands")),
                }
            }

            // Comparisons (<, <=, >, >=, ==, !=)
            BinaryOp::Eq | BinaryOp::NotEq | BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                if matches!(&left, Value::Vector(_)) || matches!(&right, Value::Vector(_)) {
                    return eval_vector_comparison(op, left, right);
                }
                match op {
                    BinaryOp::Eq => {
                        // Kleene logic: equality against NA is NA (unknown)
                        if left.is_na() {
                            return Ok(left);
                        }
                        if right.is_na() {
                            return Ok(right);
                        }
                        Ok(Value::Bool(left == right))
                    }
                    BinaryOp::NotEq => {
                        if left.is_na() {
                            return Ok(left);
                        }
                        if right.is_na() {
                            return Ok(right);
                        }
                        Ok(Value::Bool(left != right))
                    }
                    BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                        if left.is_na() {
                            return Ok(left);
                        }
                        if right.is_na() {
                            return Ok(right);
                        }
                        if let (Some(a), Some(b)) = (left.as_f64(), right.as_f64()) {
                            let cmp = match op {
                                BinaryOp::Lt => a < b,
                                BinaryOp::LtEq => a <= b,
                                BinaryOp::Gt => a > b,
                                BinaryOp::GtEq => a >= b,
                                _ => unreachable!(),
                            };
                            Ok(Value::Bool(cmp))
                        } else {
                            Err(Diagnostic::compute_error("C0202", "Inequality comparison requires numeric operands"))
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

use ghl_syntax::ast::is_column_context_verb;

fn callee_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.as_str()),
        ExprKind::FieldAccess { field, .. } => Some(field.as_str()),
        ExprKind::Path(segments) => segments.last().map(|s| s.as_str()),
        _ => None,
    }
}

/// Below this many elements, `vector_elementwise_op`'s numeric fast path stays on a single
/// thread -- rayon's work-stealing dispatch has a real, measured cost that dominates at
/// small sizes. Measured with `examples/spike_vector_elementwise_parallel_latency.rs`
/// (TODO.md Fase 4, punto (a)): at N=50,000 the parallel path is still roughly on par with
/// (very slightly behind) the sequential one (~0.91x), and by N=75,000 it's already ahead
/// (~1.23x) and only grows from there (~2.9x at N=5,000,000). 50,000 is the crossover
/// picked from that data, not a guess.
pub(crate) const PARALLEL_THRESHOLD: usize = 50_000;

/// Crossover for `sort_vector`'s (`sort_asc`/`sort_desc`) parallel path -- measured
/// separately from `PARALLEL_THRESHOLD` (see `examples/spike_sort_threshold_latency.rs`),
/// not reused blindly: a sort is O(n log n) with real per-element comparison/swap work,
/// a different cost shape than a flat O(n) elementwise op, and its real crossover lands
/// much lower -- between N=4,000 (parallel still ~0.76x, i.e. slower) and N=5,000
/// (~1.06x, i.e. break-even and climbing: ~1.46x at 10,000, ~3.29x at 1,000,000).
pub(crate) const PARALLEL_THRESHOLD_SORT: usize = 5_000;

/// Element-wise `op_fn` over two same-length `Vector`s, NA-propagating per element (an NA
/// on either side at a position makes that position's result NA, unaffected positions
/// stay unaffected). Shared by the explicit `.+`/`.-`/`.*`/`./` operators and, since
/// TODO.md Fase 3 Track 2's Vector-arithmetic follow-up, plain `+`/`-`/`*`/`/` between two
/// same-length Vectors too -- there's no other sensible meaning for vector addition/
/// subtraction (it's elementwise by definition), and unlike `Matrix` (where `*` means a
/// real matrix product on purpose, `.* ` is the elementwise escape hatch), `*` between two
/// `Vector`s follows R/NumPy/Julia's own convention of being elementwise too -- `dot()`
/// (Punto 2) is the dedicated way to ask for the dot product in all of those languages,
/// not overloading `*`.
///
/// TODO.md Fase 4, punto (a): when both sides are already numeric with zero NAs (the
/// common case for the kind of bulk arithmetic Suite 01 benchmarks), this skips the
/// `Vec<Value>` boxing entirely via `as_f64_view()` (same fast-path mechanism Punto 2's
/// `dot()` uses) and, above `PARALLEL_THRESHOLD`, spreads the work across rayon's
/// work-stealing pool. Mixed-NA input falls back to the original per-element boxed loop
/// unchanged -- that's the only place the Kleene "NA wins" logic below is needed, and it's
/// deliberately not parallelized in this pass (not the case Suite 01 benchmarks, and not
/// worth the risk without a measured need); `native_map`'s closure loop is not touched
/// here either, since it captures a mutable `RuntimeEnv` it swaps per call -- not a
/// `Send`/`Sync`-safe loop to hand to rayon without redesigning that mechanism first.
fn vector_elementwise_op(v1: &VectorData, v2: &VectorData, op_fn: fn(f64, f64) -> f64) -> Result<Value, Diagnostic> {
    if v1.len() != v2.len() {
        return Err(Diagnostic::statistical_error(
            "S0412",
            format!("Vector length mismatch in element-wise op: {} vs {}", v1.len(), v2.len()),
        ));
    }

    if v1.null_count() == 0 && v2.null_count() == 0 {
        let view1 = v1.as_f64_view()?;
        let view2 = v2.as_f64_view()?;
        let (a, b) = (view1.as_slice(), view2.as_slice());
        let data: Vec<f64> = if a.len() >= PARALLEL_THRESHOLD {
            a.par_iter().zip(b.par_iter()).map(|(&x, &y)| op_fn(x, y)).collect()
        } else {
            a.iter().zip(b.iter()).map(|(&x, &y)| op_fn(x, y)).collect()
        };
        return Ok(Value::Vector(VectorData::from_f64(data)));
    }

    let mut res = Vec::with_capacity(v1.len());
    for (a, b) in v1.iter().zip(v2.iter()) {
        if a.is_na() {
            res.push(a.clone());
        } else if b.is_na() {
            res.push(b.clone());
        } else {
            let fa = a.as_f64().unwrap_or(0.0);
            let fb = b.as_f64().unwrap_or(0.0);
            res.push(Value::F64(op_fn(fa, fb)));
        }
    }
    Ok(Value::Vector(VectorData::from_values(res)))
}

fn eval_vector_comparison(op: BinaryOp, left: Value, right: Value) -> Result<Value, Diagnostic> {
    match (left, right) {
        (Value::Vector(v1), Value::Vector(v2)) => {
            if v1.len() != v2.len() {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("Vector length mismatch in element-wise comparison: {} vs {}", v1.len(), v2.len()),
                ));
            }
            if v1.null_count() == 0 && v2.null_count() == 0 {
                if let (Ok(view1), Ok(view2)) = (v1.as_f64_view(), v2.as_f64_view()) {
                    let (a, b) = (view1.as_slice(), view2.as_slice());
                    let cmp_fn: fn(f64, f64) -> bool = match op {
                        BinaryOp::Eq => |x, y| x == y,
                        BinaryOp::NotEq => |x, y| x != y,
                        BinaryOp::Lt => |x, y| x < y,
                        BinaryOp::LtEq => |x, y| x <= y,
                        BinaryOp::Gt => |x, y| x > y,
                        BinaryOp::GtEq => |x, y| x >= y,
                        _ => unreachable!(),
                    };
                    let bools: Vec<bool> = if a.len() >= PARALLEL_THRESHOLD {
                        a.par_iter().zip(b.par_iter()).map(|(&x, &y)| cmp_fn(x, y)).collect()
                    } else {
                        a.iter().zip(b.iter()).map(|(&x, &y)| cmp_fn(x, y)).collect()
                    };
                    return Ok(Value::Vector(VectorData::from_bool(bools)));
                }
            }
            let mut out = Vec::with_capacity(v1.len());
            for (i1, i2) in v1.iter().zip(v2.iter()) {
                if i1.is_na() {
                    out.push(i1.clone());
                } else if i2.is_na() {
                    out.push(i2.clone());
                } else {
                    let res = match op {
                        BinaryOp::Eq => i1 == i2,
                        BinaryOp::NotEq => i1 != i2,
                        BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                            if let (Some(x), Some(y)) = (i1.as_f64(), i2.as_f64()) {
                                match op {
                                    BinaryOp::Lt => x < y,
                                    BinaryOp::LtEq => x <= y,
                                    BinaryOp::Gt => x > y,
                                    BinaryOp::GtEq => x >= y,
                                    _ => unreachable!(),
                                }
                            } else {
                                return Err(Diagnostic::compute_error("C0202", "Inequality comparison requires numeric operands"));
                            }
                        }
                        _ => unreachable!(),
                    };
                    out.push(Value::Bool(res));
                }
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        (Value::Vector(v), scalar) => {
            if scalar.is_na() {
                let out = vec![scalar; v.len()];
                return Ok(Value::Vector(VectorData::from_values(out)));
            }
            if v.null_count() == 0 {
                if let (Some(s), Ok(view)) = (scalar.as_f64(), v.as_f64_view()) {
                    let a = view.as_slice();
                    let cmp_fn: fn(f64, f64) -> bool = match op {
                        BinaryOp::Eq => |x, y| x == y,
                        BinaryOp::NotEq => |x, y| x != y,
                        BinaryOp::Lt => |x, y| x < y,
                        BinaryOp::LtEq => |x, y| x <= y,
                        BinaryOp::Gt => |x, y| x > y,
                        BinaryOp::GtEq => |x, y| x >= y,
                        _ => unreachable!(),
                    };
                    let bools: Vec<bool> = if a.len() >= PARALLEL_THRESHOLD {
                        a.par_iter().map(|&x| cmp_fn(x, s)).collect()
                    } else {
                        a.iter().map(|&x| cmp_fn(x, s)).collect()
                    };
                    return Ok(Value::Vector(VectorData::from_bool(bools)));
                }
            }
            let mut out = Vec::with_capacity(v.len());
            for item in v.iter() {
                if item.is_na() {
                    out.push(item.clone());
                } else {
                    let res = match op {
                        BinaryOp::Eq => item == &scalar,
                        BinaryOp::NotEq => item != &scalar,
                        BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                            if let (Some(x), Some(s)) = (item.as_f64(), scalar.as_f64()) {
                                match op {
                                    BinaryOp::Lt => x < s,
                                    BinaryOp::LtEq => x <= s,
                                    BinaryOp::Gt => x > s,
                                    BinaryOp::GtEq => x >= s,
                                    _ => unreachable!(),
                                }
                            } else {
                                return Err(Diagnostic::compute_error("C0202", "Inequality comparison requires numeric operands"));
                            }
                        }
                        _ => unreachable!(),
                    };
                    out.push(Value::Bool(res));
                }
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        (scalar, Value::Vector(v)) => {
            if scalar.is_na() {
                let out = vec![scalar; v.len()];
                return Ok(Value::Vector(VectorData::from_values(out)));
            }
            if v.null_count() == 0 {
                if let (Some(s), Ok(view)) = (scalar.as_f64(), v.as_f64_view()) {
                    let b = view.as_slice();
                    let cmp_fn: fn(f64, f64) -> bool = match op {
                        BinaryOp::Eq => |x, y| x == y,
                        BinaryOp::NotEq => |x, y| x != y,
                        BinaryOp::Lt => |x, y| x < y,
                        BinaryOp::LtEq => |x, y| x <= y,
                        BinaryOp::Gt => |x, y| x > y,
                        BinaryOp::GtEq => |x, y| x >= y,
                        _ => unreachable!(),
                    };
                    let bools: Vec<bool> = if b.len() >= PARALLEL_THRESHOLD {
                        b.par_iter().map(|&y| cmp_fn(s, y)).collect()
                    } else {
                        b.iter().map(|&y| cmp_fn(s, y)).collect()
                    };
                    return Ok(Value::Vector(VectorData::from_bool(bools)));
                }
            }
            let mut out = Vec::with_capacity(v.len());
            for item in v.iter() {
                if item.is_na() {
                    out.push(item.clone());
                } else {
                    let res = match op {
                        BinaryOp::Eq => &scalar == item,
                        BinaryOp::NotEq => &scalar != item,
                        BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                            if let (Some(s), Some(y)) = (scalar.as_f64(), item.as_f64()) {
                                match op {
                                    BinaryOp::Lt => s < y,
                                    BinaryOp::LtEq => s <= y,
                                    BinaryOp::Gt => s > y,
                                    BinaryOp::GtEq => s >= y,
                                    _ => unreachable!(),
                                }
                            } else {
                                return Err(Diagnostic::compute_error("C0202", "Inequality comparison requires numeric operands"));
                            }
                        }
                        _ => unreachable!(),
                    };
                    out.push(Value::Bool(res));
                }
            }
            Ok(Value::Vector(VectorData::from_values(out)))
        }
        _ => unreachable!(),
    }
}


/// True for any `Value` that represents a deferred `filter()` predicate rather than a
/// real computed value — the leaf `ColPredicate`/`IsNaPredicate` and the combinators
/// (`Not`/`And`/`Or`) built on top of them.
pub(crate) fn is_predicate(v: &Value) -> bool {
    matches!(
        v,
        Value::ColPredicate { .. }
            | Value::IsNaPredicate(_)
            | Value::NotPredicate(_)
            | Value::AndPredicate(..)
            | Value::OrPredicate(..)
    )
}

fn match_pattern(pattern: &Pattern, target: &Value, env: &mut RuntimeEnv) -> bool {
    match pattern {
        Pattern::Wildcard => true,
        Pattern::NA => target.is_na(),
        Pattern::NAReason(reason) => target.na_reason() == Some(reason.as_str()),
        Pattern::Lit(Literal::Int(n)) => target.as_i64() == Some(*n),
        Pattern::Lit(Literal::Float(x)) => target.as_f64() == Some(*x),
        Pattern::Lit(Literal::String(s)) => target.as_str() == Some(s.as_str()),
        Pattern::Lit(Literal::Bool(b)) => target.as_bool() == Some(*b),
        Pattern::Lit(Literal::NA(None)) => target.is_na(),
        Pattern::Lit(Literal::NA(Some(r))) => target.na_reason() == Some(r.as_str()),
        Pattern::Ident(id) => {
            env.set(id.clone(), target.clone());
            true
        }
    }
}

fn extract_formula_term(expr: &Expr, acc: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Ident(s) => acc.push(s.clone()),
        ExprKind::Binary { op: BinaryOp::Add, lhs, rhs } => {
            extract_formula_term(lhs, acc);
            extract_formula_term(rhs, acc);
        }
        ExprKind::Binary { op: BinaryOp::Mul, lhs, rhs } => {
            let mut left = Vec::new();
            let mut right = Vec::new();
            extract_formula_term(lhs, &mut left);
            extract_formula_term(rhs, &mut right);
            acc.push(format!("{}:{}", left.join(":"), right.join(":")));
        }
        _ => acc.push(format!("{:?}", expr.kind)),
    }
}

