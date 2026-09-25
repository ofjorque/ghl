use std::collections::HashMap;
use ghl_syntax::ast::*;
use ghl_syntax::source::SourceIndex;
use ghl_diagnostics::Diagnostic;
use crate::types::{Type, Dim};
use crate::env::TypeEnv;

fn callee_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.as_str()),
        ExprKind::Path(segments) => segments.last().map(|s| s.as_str()),
        _ => None,
    }
}

/// Attaches a real, human-readable `file:line:column` (derived from a byte
/// `Span` via a `SourceIndex`) to a diagnostic, while also keeping the exact
/// byte `Span` on it (via `with_span`) for consumers like the LSP that want
/// byte precision rather than a rendered line/column.
trait DiagnosticLocateExt {
    fn locate(self, index: &SourceIndex, file: &str, span: &Span) -> Self;
}

impl DiagnosticLocateExt for Diagnostic {
    fn locate(self, index: &SourceIndex, file: &str, span: &Span) -> Self {
        let (line, col) = index.offset_to_position(span.start);
        self.with_location(file, line as usize + 1, col as usize + 1)
            .with_span(span.clone())
    }
}

pub struct TypeChecker {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
    source_file: String,
    source_index: SourceIndex,
}

impl TypeChecker {
    /// `source` is the exact text that `program` was parsed from - it's needed
    /// to translate AST byte spans into real line/column numbers for
    /// diagnostics (previously, diagnostics displayed raw byte offsets
    /// mislabeled as line:column, e.g. reporting an error on a 3-line file at
    /// "32:46").
    pub fn new(source_file: String, source: &str) -> Self {
        Self {
            env: TypeEnv::with_prelude(),
            diagnostics: Vec::new(),
            source_file,
            source_index: SourceIndex::new(source),
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
                            .locate(&self.source_index, &self.source_file, &stmt.span)
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
                ..
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
                        .locate(&self.source_index, &self.source_file, &stmt.span)
                        .with_help("Adjust the function body or declared return type so they agree."),
                    );
                }
            }

            StmtKind::Struct(decl) => {
                let fields = decl
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), Type::from_annotation(&f.ty)))
                    .collect::<Vec<_>>();
                self.env.insert_struct(decl.name.clone(), fields);
            }

            StmtKind::Trait(decl) => {
                let mut methods = HashMap::new();
                for item in &decl.items {
                    if let TraitItem::Method(sig) = item {
                        let param_types = sig
                            .params
                            .iter()
                            .map(|p| {
                                p.ty.as_ref()
                                    .map(Type::from_annotation)
                                    .unwrap_or(Type::Any)
                            })
                            .collect::<Vec<_>>();
                        let ret = sig
                            .ret_ty
                            .as_ref()
                            .map(Type::from_annotation)
                            .unwrap_or(Type::Unit);
                        methods.insert(sig.name.clone(), (param_types, ret));
                    }
                }
                self.env.insert_trait(decl.name.clone(), methods);
            }

            StmtKind::Impl(decl) => {
                let target_fields = self.env.lookup_struct(&decl.target_type).cloned();
                if target_fields.is_none() {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0101",
                            format!("Cannot implement for undefined struct `{}`", decl.target_type),
                        )
                        .locate(&self.source_index, &self.source_file, &stmt.span),
                    );
                }

                if let Some(trait_name) = &decl.trait_name {
                    if self.env.lookup_trait(trait_name).is_none() {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0101",
                                format!("Undefined trait `{}`", trait_name),
                            )
                            .locate(&self.source_index, &self.source_file, &stmt.span),
                        );
                    }
                }

                let mut assoc_types: HashMap<String, Type> = HashMap::new();
                for item in &decl.items {
                    if let ImplItem::AssociatedType { name, ty, .. } = item {
                        assoc_types.insert(name.clone(), Type::from_annotation(ty));
                    }
                }

                let target_struct_ty = Type::Struct {
                    name: decl.target_type.clone(),
                    fields: target_fields.unwrap_or_default(),
                };

                let resolve_self = |t: Type| -> Type {
                    match t {
                        Type::Custom(ref s) if s == "Self" || s == "&self" || s == "&mut self" => {
                            target_struct_ty.clone()
                        }
                        Type::Custom(ref s) if s.starts_with("Self::") => {
                            let assoc_name = &s[6..];
                            assoc_types.get(assoc_name).cloned().unwrap_or(Type::Any)
                        }
                        other => other,
                    }
                };

                for item in &decl.items {
                    if let ImplItem::Method { name, params, ret_ty, body, span } = item {
                        let expected_ret = ret_ty
                            .as_ref()
                            .map(|ann| resolve_self(Type::from_annotation(ann)))
                            .unwrap_or(Type::Unit);

                        let mut param_types = Vec::new();
                        for p in params {
                            let p_ty = if p.name == "&self" || p.name == "&mut self" || p.name == "self" {
                                target_struct_ty.clone()
                            } else {
                                p.ty.as_ref()
                                    .map(|ann| resolve_self(Type::from_annotation(ann)))
                                    .unwrap_or(Type::Any)
                            };
                            param_types.push(p_ty);
                        }

                        self.env.insert_impl_method(
                            decl.target_type.clone(),
                            name.clone(),
                            param_types.clone(),
                            expected_ret.clone(),
                        );

                        self.env.push_scope();
                        for (p, p_ty) in params.iter().zip(&param_types) {
                            let bind_name = if p.name == "&self" || p.name == "&mut self" {
                                "self".to_string()
                            } else {
                                p.name.clone()
                            };
                            self.env.insert(bind_name, p_ty.clone(), false);
                        }

                        let body_ty = self.check_expr(body);
                        self.env.pop_scope();

                        if body_ty.unify(&expected_ret).is_none() && expected_ret != Type::Unit && expected_ret != Type::Any {
                            self.diagnostics.push(
                                Diagnostic::compute_error(
                                    "C0102",
                                    format!(
                                        "Method `{}` in impl `{}` return type mismatch: declared `{}`, found `{}`",
                                        name, decl.target_type, expected_ret, body_ty
                                    ),
                                )
                                .locate(&self.source_index, &self.source_file, &span),
                            );
                        }
                    }
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
                            .locate(&self.source_index, &self.source_file, &stmt.span)
                            .with_help("Declare it first with `let mut`."),
                        );
                    }
                    Some(info) if !info.is_mut => {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0104",
                                format!("Cannot assign to `{}`: not declared as `mut`", name),
                            )
                            .locate(&self.source_index, &self.source_file, &stmt.span)
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
                                .locate(&self.source_index, &self.source_file, &stmt.span),
                            );
                        }
                    }
                }
            }
            StmtKind::Use(use_stmt) => {
                if !crate::modules::is_valid_module_path(&use_stmt.path) {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0105",
                            format!("Cannot find module `{}`", use_stmt.path.join("::")),
                        )
                        .locate(&self.source_index, &self.source_file, &stmt.span)
                        .with_help("Verify the module name and path in the standard library."),
                    );
                    return;
                }

                match &use_stmt.kind {
                    ghl_syntax::ast::UseKind::Glob => {
                        if let Some(items) = crate::modules::get_module_items(&use_stmt.path) {
                            for (name, ty) in items {
                                self.env.insert(name, ty, false);
                            }
                        } else {
                            self.diagnostics.push(
                                Diagnostic::compute_error(
                                    "C0105",
                                    format!("Module `{}` cannot be glob imported", use_stmt.path.join("::")),
                                )
                                .locate(&self.source_index, &self.source_file, &stmt.span),
                            );
                        }
                    }
                    ghl_syntax::ast::UseKind::Items(items) => {
                        for item in items {
                            let mut full_path = use_stmt.path.clone();
                            full_path.push(item.name.clone());
                            if let Some(ty) = crate::modules::lookup_module_item(&full_path) {
                                let bound_name = item.alias.as_deref().unwrap_or(&item.name);
                                self.env.insert(bound_name.to_string(), ty, false);
                            } else {
                                self.diagnostics.push(
                                    Diagnostic::compute_error(
                                        "C0106",
                                        format!(
                                            "Cannot find item `{}` in module `{}`",
                                            item.name,
                                            use_stmt.path.join("::")
                                        ),
                                    )
                                    .locate(&self.source_index, &self.source_file, &stmt.span)
                                    .with_help("Check spelling or see available items in the standard library documentation."),
                                );
                            }
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
                        .locate(&self.source_index, &self.source_file, &expr.span)
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
                            .locate(&self.source_index, &self.source_file, &item.span)
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
                                .locate(&self.source_index, &self.source_file, &cell.span)
                                .with_help("Ensure all cells in `mat [...]` evaluate to numbers."),
                            );
                        }
                    }
                }
                let r_count = rows.len();
                let c_count = rows.first().map(|r| r.len()).unwrap_or(0);
                Type::matrix(Type::F64, Dim::Known(r_count), Dim::Known(c_count))
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
                                .locate(&self.source_index, &self.source_file, &expr.span)
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
                                .locate(&self.source_index, &self.source_file, &expr.span)
                                .with_help("Explicitly convert with `.to_string()` or `.parse::<f64>()`."),
                            );
                            return Type::Any;
                        }

                        // Division produces F64
                        if *op == BinaryOp::Div {
                            return Type::F64;
                        }

                        // Matrix multiplication: A * B
                        if *op == BinaryOp::Mul {
                            if let (Type::Matrix { elem: e1, rows: r1, cols: c1 }, Type::Matrix { elem: e2, rows: r2, cols: c2 }) = (&t_lhs, &t_rhs) {
                                if let (Dim::Known(k1), Dim::Known(k2)) = (c1, r2) {
                                    if k1 != k2 {
                                        self.diagnostics.push(
                                            Diagnostic::compute_error(
                                                "C0102",
                                                format!(
                                                    "Matrix dimension mismatch in multiplication: ({}x{}) * ({}x{}) — inner dimensions {} and {} do not match",
                                                    r1, c1, r2, c2, k1, k2
                                                ),
                                            )
                                            .locate(&self.source_index, &self.source_file, &expr.span)
                                            .with_help("To multiply matrices A * B, the number of columns in A must equal the number of rows in B."),
                                        );
                                        return Type::Any;
                                    }
                                }
                                let elem = e1.unify(e2).unwrap_or(Type::F64);
                                return Type::Matrix {
                                    elem: Box::new(elem),
                                    rows: *r1,
                                    cols: *c2,
                                };
                            }

                            // Matrix * Vector or Vector * Matrix
                            if let (Type::Matrix { elem: e1, .. }, Type::Vector(e2)) = (&t_lhs, &t_rhs) {
                                let elem = e1.unify(e2).unwrap_or(Type::F64);
                                return Type::Vector(Box::new(elem));
                            }
                            if let (Type::Vector(e1), Type::Matrix { elem: e2, .. }) = (&t_lhs, &t_rhs) {
                                let elem = e1.unify(e2).unwrap_or(Type::F64);
                                return Type::Vector(Box::new(elem));
                            }

                            // Matrix * Scalar or Scalar * Matrix
                            if let Type::Matrix { elem, rows, cols } = &t_lhs {
                                if t_rhs.is_numeric() {
                                    return Type::Matrix { elem: elem.clone(), rows: *rows, cols: *cols };
                                }
                            }
                            if let Type::Matrix { elem, rows, cols } = &t_rhs {
                                if t_lhs.is_numeric() {
                                    return Type::Matrix { elem: elem.clone(), rows: *rows, cols: *cols };
                                }
                            }
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
                                .locate(&self.source_index, &self.source_file, &lhs.span)
                                .with_help("Matrix solve `A \\ b` computes x solving A*x = b."),
                            );
                        }
                        if t_rhs.is_vector() {
                            Type::Vector(Box::new(Type::F64))
                        } else if let (Type::Matrix { rows: r1, cols: c1, .. }, Type::Matrix { elem: e2, rows: r2, cols: c2 }) = (&t_lhs, &t_rhs) {
                            if let (Dim::Known(k1), Dim::Known(k2)) = (r1, r2) {
                                if k1 != k2 {
                                    self.diagnostics.push(
                                        Diagnostic::compute_error(
                                            "C0102",
                                            format!("Matrix dimension mismatch in solve `\\`: A is ({}x{}), B is ({}x{}) — row dimensions {} and {} must match", r1, c1, r2, c2, k1, k2),
                                        )
                                        .locate(&self.source_index, &self.source_file, &expr.span),
                                    );
                                    return Type::Any;
                                }
                            }
                            Type::Matrix { elem: e2.clone(), rows: *c1, cols: *c2 }
                        } else {
                            Type::matrix_dynamic(Type::F64)
                        }
                    }

                    // Element-wise matrix operations
                    BinaryOp::DotMul | BinaryOp::DotAdd | BinaryOp::DotSub | BinaryOp::DotDiv => {
                        if let (Type::Matrix { elem: e1, rows: r1, cols: c1 }, Type::Matrix { elem: e2, rows: r2, cols: c2 }) = (&t_lhs, &t_rhs) {
                            if !r1.is_compatible_with(r2) || !c1.is_compatible_with(c2) {
                                self.diagnostics.push(
                                    Diagnostic::compute_error(
                                        "C0102",
                                        format!("Matrix dimension mismatch in element-wise operation: ({}x{}) vs ({}x{})", r1, c1, r2, c2),
                                    )
                                    .locate(&self.source_index, &self.source_file, &expr.span)
                                    .with_help("Element-wise matrix operations require identical dimensions."),
                                );
                                return Type::Any;
                            }
                            let elem = e1.unify(e2).unwrap_or(Type::F64);
                            return Type::Matrix {
                                elem: Box::new(elem),
                                rows: if let Dim::Known(_) = r1 { *r1 } else { *r2 },
                                cols: if let Dim::Known(_) = c1 { *c1 } else { *c2 },
                            };
                        } else if t_lhs.is_matrix() {
                            t_lhs.clone()
                        } else if t_rhs.is_matrix() {
                            t_rhs.clone()
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
                                .locate(&self.source_index, &self.source_file, &expr.span)
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
                        .locate(&self.source_index, &self.source_file, &cond.span),
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
                            .locate(&self.source_index, &self.source_file, &expr.span)
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
                        .locate(&self.source_index, &self.source_file, &cond.span),
                    );
                }
                self.check_expr(body);
                Type::Unit
            }

            ExprKind::For { start, end, body, .. } => {
                let start_ty = self.check_expr(start);
                if start_ty != Type::I64 && start_ty != Type::Any {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0103",
                            format!("`for` range start must be `i64`, found `{}`", start_ty),
                        )
                        .locate(&self.source_index, &self.source_file, &start.span),
                    );
                }
                let end_ty = self.check_expr(end);
                if end_ty != Type::I64 && end_ty != Type::Any {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0103",
                            format!("`for` range end must be `i64`, found `{}`", end_ty),
                        )
                        .locate(&self.source_index, &self.source_file, &end.span),
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
                            .locate(&self.source_index, &self.source_file, &arm.span)
                            .with_help("All arms in a `match` expression must yield compatible types."),
                        );
                    }
                }

                unified_result
            }

            ExprKind::Formula { .. } => Type::Formula,

            ExprKind::SemSpec { equations } => {
                for eq in equations {
                    let eq_ty = self.check_expr(eq);
                    if !matches!(eq.kind, ExprKind::Formula { .. }) {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0615",
                                format!(
                                    "`sem_spec` equations must use `~`, `=~`, or `~~`, found `{}`",
                                    eq_ty
                                ),
                            )
                            .locate(&self.source_index, &self.source_file, &eq.span)
                            .with_help(
                                "Each line inside `sem_spec { ... }` must be a formula equation, e.g. `f1 =~ x1 + x2;`.",
                            ),
                        );
                    }
                }
                Type::SemSpec
            }

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

            ExprKind::RecordLit(fields) => {
                let checked_fields: Vec<(String, Type)> = fields
                    .iter()
                    .map(|(name, e)| (name.clone(), self.check_expr_ctx(e, col_ctx)))
                    .collect();
                Type::Record(checked_fields)
            }

            ExprKind::StructLit { name, fields } => {
                let defined_fields = match self.env.lookup_struct(name) {
                    Some(f) => f.clone(),
                    None => {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0101",
                                format!("Struct `{}` is not defined", name),
                            )
                            .locate(&self.source_index, &self.source_file, &expr.span),
                        );
                        return Type::Any;
                    }
                };

                let mut actual_fields = Vec::new();
                for (f_name, f_expr) in fields {
                    let f_ty = self.check_expr_ctx(f_expr, col_ctx);
                    if let Some((_, expected_ty)) = defined_fields.iter().find(|(n, _)| n == f_name) {
                        if f_ty.unify(expected_ty).is_none() {
                            self.diagnostics.push(
                                Diagnostic::compute_error(
                                    "C0102",
                                    format!(
                                        "Field `{}` in `{}` expected type `{}`, but found `{}`",
                                        f_name, name, expected_ty, f_ty
                                    ),
                                )
                                .locate(&self.source_index, &self.source_file, &f_expr.span),
                            );
                        }
                    } else {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0102",
                                format!("Unknown field `{}` for struct `{}`", f_name, name),
                            )
                            .locate(&self.source_index, &self.source_file, &f_expr.span),
                        );
                    }
                    actual_fields.push((f_name.clone(), f_ty));
                }

                Type::Struct {
                    name: name.clone(),
                    fields: actual_fields,
                }
            }

            ExprKind::FieldAccess { target, field } => {
                let target_ty = self.check_expr_ctx(target, col_ctx);
                match target_ty {
                    Type::Record(fields) => {
                        if let Some((_, ty)) = fields.iter().find(|(n, _)| n == field) {
                            ty.clone()
                        } else {
                            self.diagnostics.push(
                                Diagnostic::compute_error(
                                    "C0101",
                                    format!("Field `{}` not found in record", field),
                                )
                                .locate(&self.source_index, &self.source_file, &expr.span),
                            );
                            Type::Any
                        }
                    }
                    Type::Struct { ref name, ref fields } => {
                        if let Some((_, ty)) = fields.iter().find(|(n, _)| n == field) {
                            ty.clone()
                        } else if let Some((params, ret)) = self.env.lookup_method(name, field) {
                            Type::Function {
                                params: if params.is_empty() { Vec::new() } else { params[1..].to_vec() },
                                ret: Box::new(ret.clone()),
                            }
                        } else {
                            Type::Any
                        }
                    }
                    Type::Custom(ref name) => {
                        if let Some(fields) = self.env.lookup_struct(name) {
                            if let Some((_, ty)) = fields.iter().find(|(n, _)| n == field) {
                                return ty.clone();
                            }
                        }
                        if let Some((params, ret)) = self.env.lookup_method(name, field) {
                            Type::Function {
                                params: if params.is_empty() { Vec::new() } else { params[1..].to_vec() },
                                ret: Box::new(ret.clone()),
                            }
                        } else {
                            Type::Any
                        }
                    }
                    Type::DataFrame(cols) => {
                        if let Some((_, ty)) = cols.iter().find(|(n, _)| n == field) {
                            Type::Vector(Box::new(ty.clone()))
                        } else {
                            Type::Any
                        }
                    }
                    Type::Formula => match field.as_str() {
                        "response" => Type::String,
                        "terms" => Type::Vector(Box::new(Type::String)),
                        "parts" => Type::Vector(Box::new(Type::Vector(Box::new(Type::String)))),
                        "absorbed" => Type::Vector(Box::new(Type::String)),
                        "instruments" => Type::Vector(Box::new(Type::String)),
                        "parts_count" => Type::I64,
                        "has_fixed_effects" => Type::Bool,
                        "has_instruments" => Type::Bool,
                        _ => Type::Any,
                    },
                    _ => Type::Any,
                }
            }

            ExprKind::Index { target, indices } => {
                let target_ty = self.check_expr_ctx(target, col_ctx);
                match target_ty {
                    Type::Vector(elem_ty) => {
                        if indices.len() == 1 {
                            match &indices[0] {
                                IndexSpec::Expr(e) => {
                                    let idx_ty = self.check_expr_ctx(e, col_ctx);
                                    match idx_ty {
                                        Type::I64 => *elem_ty,
                                        _ => Type::Vector(elem_ty),
                                    }
                                }
                                IndexSpec::Range { .. } | IndexSpec::All => Type::Vector(elem_ty),
                            }
                        } else {
                            Type::Any
                        }
                    }
                    Type::Matrix { elem, rows: _, cols: _ } => {
                        if indices.len() == 2 {
                            let is_r_scalar = match &indices[0] {
                                IndexSpec::Expr(e) => self.check_expr_ctx(e, col_ctx) == Type::I64,
                                _ => false,
                            };
                            let is_c_scalar = match &indices[1] {
                                IndexSpec::Expr(e) => self.check_expr_ctx(e, col_ctx) == Type::I64,
                                _ => false,
                            };
                            if is_r_scalar && is_c_scalar {
                                *elem
                            } else if is_r_scalar || is_c_scalar {
                                Type::Vector(elem)
                            } else {
                                Type::matrix_dynamic(*elem)
                            }
                        } else {
                            Type::Any
                        }
                    }
                    Type::String => Type::String,
                    _ => Type::Any,
                }
            }

            ExprKind::Range { start, end, .. } => {
                let start_ty = self.check_expr_ctx(start, col_ctx);
                let end_ty = self.check_expr_ctx(end, col_ctx);
                if start_ty != Type::I64 && start_ty != Type::Any {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0103",
                            format!("Range start must be `i64`, found `{}`", start_ty),
                        )
                        .locate(&self.source_index, &self.source_file, &start.span),
                    );
                }
                if end_ty != Type::I64 && end_ty != Type::Any {
                    self.diagnostics.push(
                        Diagnostic::compute_error(
                            "C0103",
                            format!("Range end must be `i64`, found `{}`", end_ty),
                        )
                        .locate(&self.source_index, &self.source_file, &end.span),
                    );
                }
                Type::Custom("Range".into())
            }

            ExprKind::Path(segments) => {
                if segments.len() == 2 && (segments[0] == "NAReason" || segments[0] == "NAReasons") {
                    Type::NA
                } else if let Some(ty) = crate::modules::lookup_module_item(segments) {
                    ty
                } else {
                    let full_name = segments.join("::");
                    if let Some(info) = self.env.lookup(&full_name) {
                        info.ty.clone()
                    } else if col_ctx {
                        Type::Any
                    } else {
                        self.diagnostics.push(
                            Diagnostic::compute_error(
                                "C0101",
                                format!("Undefined path `{}`", full_name),
                            )
                            .locate(&self.source_index, &self.source_file, &expr.span)
                            .with_help("Verify the module and function name, or import it with `use`."),
                        );
                        Type::Any
                    }
                }
            }
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
                        .locate(&self.source_index, &self.source_file, &span),
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
                    .locate(&self.source_index, &self.source_file, &span),
                );
                Type::Any
            }
        }
    }
}
