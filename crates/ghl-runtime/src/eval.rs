use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::*;
use crate::value::Value;
use crate::env::RuntimeEnv;
use crate::matrix::MatrixOps;

pub struct Interpreter {
    pub env: RuntimeEnv,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            env: RuntimeEnv::with_prelude(),
        }
    }

    pub fn eval_program(&mut self, program: &Program) -> Result<Value, Diagnostic> {
        let mut last_val = Value::Unit;
        for stmt in &program.statements {
            last_val = self.eval_stmt(stmt)?;
        }
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
            StmtKind::Expr(expr) => self.eval_expr(expr),
            StmtKind::Return(opt_expr) => {
                if let Some(e) = opt_expr {
                    self.eval_expr(e)
                } else {
                    Ok(Value::Unit)
                }
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
                Ok(Value::Vector(evaluated))
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
                        Value::Vector(vec_data) => vec_data,
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
                    data,
                })
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
                    Value::Vector(v) => {
                        let mut res = Vec::new();
                        for item in v {
                            match item {
                                Value::I64(n) => res.push(Value::I64(-n)),
                                Value::F64(x) => res.push(Value::F64(-x)),
                                other => res.push(other),
                            }
                        }
                        Ok(Value::Vector(res))
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
                    _ => self.eval_expr(target),
                }
            }

            ExprKind::Call { callee, args } => {
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
        }
    }

    fn call_value(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, Diagnostic> {
        match callee {
            Value::NativeFn(func) => func(args),
            Value::Closure { params, body, mut env } => {
                // Inherit any newly defined globals into the closure environment
                if let Some(global_scope) = self.env.scopes.first() {
                    for (k, v) in global_scope {
                        if env.get(k).is_none() {
                            env.set(k.clone(), v.clone());
                        }
                    }
                }

                env.push_scope();
                for (p, a) in params.iter().zip(args.into_iter()) {
                    env.set(p.clone(), a);
                }
                let old_env = std::mem::replace(&mut self.env, env);
                let res = self.eval_expr(&body);
                self.env = old_env;
                res
            }
            other => Err(Diagnostic::compute_error(
                "C0203",
                format!("Value `{}` is not callable as a function", other.type_name()),
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
                        for item in v {
                            b_floats.push(item.as_f64().unwrap_or(0.0));
                        }
                        let x = MatrixOps::solve(rows, &data, &b_floats)?;
                        let res_vals = x.into_iter().map(Value::F64).collect();
                        Ok(Value::Vector(res_vals))
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
                        Ok(Value::Matrix { rows: r, cols: c, data: d })
                    }
                    (Value::Vector(v1), Value::Vector(v2)) => {
                        if v1.len() != v2.len() {
                            return Err(Diagnostic::statistical_error(
                                "S0412",
                                format!("Vector length mismatch in element-wise op: {} vs {}", v1.len(), v2.len()),
                            ));
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
                        Ok(Value::Vector(res))
                    }
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

                // Vector with scalar broadcasting
                if let (Value::Vector(v), scalar) = (&left, &right) {
                    if let Some(s) = scalar.as_f64() {
                        let mut res = Vec::with_capacity(v.len());
                        for item in v {
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
                        return Ok(Value::Vector(res));
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
        }
    }
}

use ghl_syntax::ast::is_column_context_verb;

fn callee_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.as_str()),
        _ => None,
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

