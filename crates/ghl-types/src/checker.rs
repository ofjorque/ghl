use ghl_syntax::ast::*;
use ghl_diagnostics::Diagnostic;
use crate::types::Type;
use crate::env::TypeEnv;

fn callee_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.as_str()),
        _ => None,
    }
}

pub struct TypeChecker {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
    source_file: String,
}

impl TypeChecker {
    pub fn new(source_file: String) -> Self {
        Self {
            env: TypeEnv::with_prelude(),
            diagnostics: Vec::new(),
            source_file,
        }
    }

    pub fn check_program(&mut self, program: &Program) -> Result<(), Vec<Diagnostic>> {
        for stmt in &program.statements {
            self.check_stmt(stmt);
        }

        if self.diagnostics.iter().any(|d| d.is_error()) {
            Err(self.diagnostics.clone())
        } else {
            Ok(())
        }
    }

    pub fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let {
                name,
                is_mut,
                ty,
                init,
            } => {
                let init_ty = self.check_expr(init);

                if let Some(ann) = ty {
                    let declared_ty = Type::from_annotation(ann);
                    if init_ty.unify(&declared_ty).is_none() {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0102",
                                format!(
                                    "Type mismatch in let binding `{}`: expected `{}`, found `{}`",
                                    name, declared_ty, init_ty
                                ),
                            )
                            .with_location(&self.source_file, stmt.span.start, stmt.span.end)
                            .with_help(format!(
                                "Ensure the initialization expression matches the declared type `{}`.",
                                declared_ty
                            )),
                        );
                    }
                    self.env.insert(name.clone(), declared_ty, *is_mut);
                } else {
                    self.env.insert(name.clone(), init_ty, *is_mut);
                }
            }

            StmtKind::Fn {
                name,
                params,
                ret_ty,
                body,
            } => {
                let expected_ret = ret_ty
                    .as_ref()
                    .map(Type::from_annotation)
                    .unwrap_or(Type::Any);

                let mut param_types = Vec::new();
                for p in params {
                    let p_ty = p
                        .ty
                        .as_ref()
                        .map(Type::from_annotation)
                        .unwrap_or(Type::Any);
                    param_types.push(p_ty);
                }

                // Register function signature before checking body to support recursive calls
                self.env.insert(
                    name.clone(),
                    Type::Function {
                        params: param_types.clone(),
                        ret: Box::new(expected_ret.clone()),
                    },
                    false,
                );

                self.env.push_scope();
                for (p, p_ty) in params.iter().zip(&param_types) {
                    self.env.insert(p.name.clone(), p_ty.clone(), false);
                }

                let body_ty = self.check_expr(body);
                self.env.pop_scope();

                if body_ty.unify(&expected_ret).is_none() && expected_ret != Type::Unit && expected_ret != Type::Any {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0102",
                            format!(
                                "Function `{}` return type mismatch: declared `{}`, but body evaluates to `{}`",
                                name, expected_ret, body_ty
                            ),
                        )
                        .with_location(&self.source_file, stmt.span.start, stmt.span.end)
                        .with_help("Adjust the function body or declared return type so they agree."),
                    );
                }
            }

            StmtKind::Expr(expr) => {
                self.check_expr(expr);
            }

            StmtKind::Return(opt_expr) => {
                if let Some(e) = opt_expr {
                    self.check_expr(e);
                }
            }

            StmtKind::Assign { name, value } => {
                let value_ty = self.check_expr(value);
                match self.env.lookup(name) {
                    None => {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0101",
                                format!("Cannot assign to undefined variable `{}`", name),
                            )
                            .with_location(&self.source_file, stmt.span.start, stmt.span.end)
                            .with_help("Declare it first with `let mut`."),
                        );
                    }
                    Some(info) if !info.is_mut => {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0104",
                                format!("Cannot assign to `{}`: not declared as `mut`", name),
                            )
                            .with_location(&self.source_file, stmt.span.start, stmt.span.end)
                            .with_help(format!("Declare it as `let mut {} = ...;` to allow reassignment.", name)),
                        );
                    }
                    Some(info) => {
                        let declared_ty = info.ty.clone();
                        if value_ty.unify(&declared_ty).is_none() {
                            self.diagnostics.push(
                                Diagnostic::compute_error(
                                    "C0102",
                                    format!(
                                        "Type mismatch assigning to `{}`: expected `{}`, found `{}`",
                                        name, declared_ty, value_ty
                                    ),
                                )
                                .with_location(&self.source_file, stmt.span.start, stmt.span.end),
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn check_expr(&mut self, expr: &Expr) -> Type {
        self.check_expr_ctx(expr, false)
    }

    /// Mirrors `Interpreter::eval_expr_ctx` in `ghl-runtime`: when `col_ctx` is true, an
    /// undefined identifier is treated as a bare column reference (`Type::Any`, no
    /// diagnostic) instead of an "undefined variable" error — see `ghl_syntax::ast::COLUMN_CONTEXT_VERBS`.
    fn check_expr_ctx(&mut self, expr: &Expr, col_ctx: bool) -> Type {
        match &expr.kind {
            ExprKind::Lit(Literal::Int(_)) => Type::I64,
            ExprKind::Lit(Literal::Float(_)) => Type::F64,
            ExprKind::Lit(Literal::String(_)) => Type::String,
            ExprKind::Lit(Literal::Bool(_)) => Type::Bool,
            ExprKind::Lit(Literal::NA(_)) => Type::NA,

            ExprKind::Ident(name) => {
                if let Some(info) = self.env.lookup(name) {
                    info.ty.clone()
                } else if col_ctx {
                    Type::Any
                } else {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0101",
                            format!("Undefined variable or function `{}`", name),
                        )
                        .with_location(&self.source_file, expr.span.start, expr.span.end)
                        .with_help("Check spelling or declare the variable before use."),
                    );
                    Type::Any
                }
            }

            // Anti-R & Anti-Python Vector Guarantee:
            // 1. Homogeneous: [1, "foo"] fails with C0103 instead of silent coercion.
            // 2. NA does NOT convert integers into float64: [1, 2, NA] stays Vector[i64].
            ExprKind::VectorLit(items) => {
                if items.is_empty() {
                    return Type::Vector(Box::new(Type::Any));
                }

                let mut unified_elem = Type::NA;

                for item in items {
                    let item_ty = self.check_expr_ctx(item, col_ctx);
                    if let Some(next_elem) = unified_elem.unify(&item_ty) {
                        unified_elem = next_elem;
                    } else {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0103",
                                format!(
                                    "Heterogeneous vector element: cannot unify `{}` with `{}`",
                                    unified_elem, item_ty
                                ),
                            )
                            .with_location(&self.source_file, item.span.start, item.span.end)
                            .with_help("GHL vectors are strictly homogeneous and do not coerce silently like R or Python."),
                        );
                    }
                }

                Type::Vector(Box::new(unified_elem))
            }

            ExprKind::DataFrameLit(cols) => {
                let mut col_types = Vec::new();
                for (name, col_expr) in cols {
                    let col_ty = self.check_expr(col_expr);
                    col_types.push((name.clone(), col_ty));
                }
                Type::DataFrame(col_types)
            }

            ExprKind::MatrixLit { rows } => {
                for row in rows {
                    for cell in row {
                        let cell_ty = self.check_expr(cell);
                        if !cell_ty.is_numeric() && cell_ty != Type::NA && cell_ty != Type::Any {
                            self.diagnostics.push(
                                Diagnostic::statistical_error(
                                    "S0412",
                                    format!("Matrix elements must be numeric, found `{}`", cell_ty),
                                )
                                .with_location(&self.source_file, cell.span.start, cell.span.end)
                                .with_help("Ensure all cells in `mat [...]` evaluate to numbers."),
                            );
                        }
                    }
                }
                Type::Matrix(Box::new(Type::F64))
            }

            ExprKind::Binary { op, lhs, rhs } => {
                let t_lhs = self.check_expr_ctx(lhs, col_ctx);
                let t_rhs = self.check_expr_ctx(rhs, col_ctx);

                match op {
                    // Arithmetic operators
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                        // Anti-R string coercion check
                        if (t_lhs == Type::String || t_rhs == Type::String) && *op != BinaryOp::Add {
                            self.diagnostics.push(
                                Diagnostic::compute_error(
                                    "C0102",
                                    format!("Arithmetic operation `{:?}` cannot be applied to strings (`{}` and `{}`)", op, t_lhs, t_rhs),
                                )
                                .with_location(&self.source_file, expr.span.start, expr.span.end)
                                .with_help("No silent type coercion: convert explicitly if needed."),
                            );
                            return Type::Any;
                        }

                        if *op == BinaryOp::Add && t_lhs == Type::String && t_rhs == Type::String {
                            return Type::String;
                        }

                        if (t_lhs == Type::String && t_rhs.is_numeric()) || (t_rhs == Type::String && t_lhs.is_numeric()) {
                            self.diagnostics.push(
                                Diagnostic::compute_error(
                                    "C0102",
                                    format!("Cannot add `{}` and `{}`: no implicit type coercion in GHL", t_lhs, t_rhs),
                                )
                                .with_location(&self.source_file, expr.span.start, expr.span.end)
                                .with_help("Explicitly convert with `.to_string()` or `.parse::<f64>()`."),
                            );
                            return Type::Any;
                        }

                        // Division produces F64
                        if *op == BinaryOp::Div {
                            return Type::F64;
                        }

                        // Numeric combination
                        if t_lhs == Type::F64 || t_rhs == Type::F64 {
                            Type::F64
                        } else if t_lhs == Type::I64 && t_rhs == Type::I64 {
                            Type::I64
                        } else if t_lhs.is_vector() || t_rhs.is_vector() {
                            t_lhs.unify(&t_rhs).unwrap_or(Type::Vector(Box::new(Type::F64)))
                        } else {
                            Type::F64
                        }
                    }

                    // Matrix solve: A \ b
                    BinaryOp::MatSolve => {
                        if !t_lhs.is_matrix() && t_lhs != Type::Any {
                            self.diagnostics.push(
                                Diagnostic::statistical_error(
                                    "S0412",
                                    format!("Left-hand side of matrix solve `\\` must be a Matrix, found `{}`", t_lhs),
                                )
                                .with_location(&self.source_file, lhs.span.start, lhs.span.end)
                                .with_help("Matrix solve `A \\ b` computes x solving A*x = b."),
                            );
                        }
                        if t_rhs.is_vector() {
                            Type::Vector(Box::new(Type::F64))
                        } else {
                            Type::Matrix(Box::new(Type::F64))
                        }
                    }

                    // Element-wise matrix operations
                    BinaryOp::DotMul | BinaryOp::DotAdd | BinaryOp::DotSub | BinaryOp::DotDiv => {
                        if t_lhs.is_matrix() || t_rhs.is_matrix() {
                            Type::Matrix(Box::new(Type::F64))
                        } else if t_lhs.is_vector() || t_rhs.is_vector() {
                            Type::Vector(Box::new(Type::F64))
                        } else {
                            Type::F64
                        }
                    }

                    // Comparisons
                    BinaryOp::Eq | BinaryOp::NotEq => {
                        // Statistical warning on direct comparison with NA
                        if t_lhs == Type::NA || t_rhs == Type::NA {
                            self.diagnostics.push(
                                Diagnostic::statistical_warning(
                                    "SW0201",
                                    "Direct comparison with `NA` detected. Under Kleene logic, `x == NA` evaluates to `NA`, not boolean true/false.",
                                )
                                .with_location(&self.source_file, expr.span.start, expr.span.end)
                                .with_help("Use `is_na(x)` or pattern matching with `match` instead."),
                            );
                        }
                        if t_lhs.is_vector() || t_rhs.is_vector() {
                            Type::Vector(Box::new(Type::Bool))
                        } else {
                            Type::Bool
                        }
                    }

                    BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                        if t_lhs.is_vector() || t_rhs.is_vector() {
                            Type::Vector(Box::new(Type::Bool))
                        } else {
                            Type::Bool
                        }
                    }
                    BinaryOp::And | BinaryOp::Or => Type::Bool,
                }
            }

            ExprKind::UnaryNeg(inner) => self.check_expr_ctx(inner, col_ctx),
            ExprKind::UnaryNot(inner) => {
                self.check_expr_ctx(inner, col_ctx);
                Type::Bool
            }

            ExprKind::Pipe { expr, target } => {
                let src_ty = self.check_expr(expr);
                match &target.kind {
                    ExprKind::Call { callee, args } => {
                        let arg_ctx = col_ctx || callee_name(callee).is_some_and(is_column_context_verb);
                        let mut call_args = vec![src_ty];
                        for arg in args {
                            call_args.push(self.check_expr_ctx(arg, arg_ctx));
                        }
                        self.check_call_type(callee, &call_args, target.span.clone())
                    }
                    ExprKind::Ident(_) => {
                        self.check_call_type(target, &[src_ty], target.span.clone())
                    }
                    _ => {
                        self.check_expr(target);
                        Type::Any
                    }
                }
            }

            ExprKind::Call { callee, args } => {
                let arg_ctx = col_ctx || callee_name(callee).is_some_and(is_column_context_verb);
                let arg_types: Vec<Type> = args.iter().map(|a| self.check_expr_ctx(a, arg_ctx)).collect();
                self.check_call_type(callee, &arg_types, expr.span.clone())
            }

            ExprKind::Block { stmts, expr: opt_expr } => {
                self.env.push_scope();
                for s in stmts {
                    self.check_stmt(s);
                }
                let block_val_ty = if let Some(e) = opt_expr {
                    self.check_expr(e)
                } else {
                    Type::Unit
                };
                self.env.pop_scope();
                block_val_ty
            }

            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond_ty = self.check_expr(cond);
                if cond_ty != Type::Bool && cond_ty != Type::Any {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0102",
                            format!("`if` condition must evaluate to `bool`, found `{}`", cond_ty),
                        )
                        .with_location(&self.source_file, cond.span.start, cond.span.end),
                    );
                }

                let then_ty = self.check_expr(then_branch);
                if let Some(el) = else_branch {
                    let else_ty = self.check_expr(el);
                    if let Some(unified) = then_ty.unify(&else_ty) {
                        unified
                    } else {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0102",
                                format!(
                                    "`if` and `else` branches have incompatible types: `{}` vs `{}`",
                                    then_ty, else_ty
                                ),
                            )
                            .with_location(&self.source_file, expr.span.start, expr.span.end)
                            .with_help("Both branches of an `if` expression must return the same type."),
                        );
                        Type::Any
                    }
                } else {
                    Type::Unit
                }
            }

            ExprKind::While { cond, body } => {
                let cond_ty = self.check_expr(cond);
                if cond_ty != Type::Bool && cond_ty != Type::Any {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0102",
                            format!("`while` condition must evaluate to `bool`, found `{}`", cond_ty),
                        )
                        .with_location(&self.source_file, cond.span.start, cond.span.end),
                    );
                }
                self.check_expr(body);
                Type::Unit
            }

            ExprKind::Match { expr: matched, arms } => {
                let _matched_ty = self.check_expr(matched);
                let mut unified_result = Type::NA;

                for arm in arms {
                    let body_ty = self.check_expr(&arm.body);
                    if let Some(u) = unified_result.unify(&body_ty) {
                        unified_result = u;
                    } else {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0102",
                                format!(
                                    "Incompatible return type in match arm: `{}` vs `{}`",
                                    unified_result, body_ty
                                ),
                            )
                            .with_location(&self.source_file, arm.span.start, arm.span.end)
                            .with_help("All arms in a `match` expression must yield compatible types."),
                        );
                    }
                }

                unified_result
            }

            ExprKind::Formula { .. } => Type::Formula,

            ExprKind::Lambda { params, body } => {
                self.env.push_scope();
                for p in params {
                    self.env.insert(p.clone(), Type::Any, false);
                }
                let body_ty = self.check_expr(body);
                self.env.pop_scope();
                Type::Function {
                    params: vec![Type::Any; params.len()],
                    ret: Box::new(body_ty),
                }
            }

            ExprKind::Placeholder => Type::Any,

            ExprKind::NamedArg { value, .. } => self.check_expr_ctx(value, col_ctx),
        }
    }

    fn check_call_type(&mut self, callee: &Expr, arg_types: &[Type], span: Span) -> Type {
        let callee_ty = self.check_expr(callee);
        match callee_ty {
            Type::Function { params, ret } => {
                if params.len() != arg_types.len() && !params.iter().any(|p| *p == Type::Any) {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0102",
                            format!(
                                "Incorrect argument count: expected {}, got {}",
                                params.len(),
                                arg_types.len()
                            ),
                        )
                        .with_location(&self.source_file, span.start, span.end),
                    );
                }
                *ret
            }
            Type::Any => Type::Any,
            other => {
                self.diagnostics.push(
                    Diagnostic::compute_error(
                        "C0102",
                        format!("Type `{}` is not callable as a function", other),
                    )
                    .with_location(&self.source_file, span.start, span.end),
                );
                Type::Any
            }
        }
    }
}
