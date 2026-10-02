//! AST Lowering to High-Level Intermediate Representation (HIR).

use std::collections::HashMap;
use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::{BinaryOp, Expr, ExprKind, Literal, Program, Stmt, StmtKind, TypeAnnotation};
use crate::hir::*;

pub struct LoweringContext {
    pub scopes: Vec<HashMap<String, HirType>>,
    pub function_signatures: HashMap<String, (Vec<HirType>, HirType)>,
}

impl Default for LoweringContext {
    fn default() -> Self {
        Self::new()
    }
}

impl LoweringContext {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            function_signatures: HashMap::new(),
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn insert_var(&mut self, name: String, ty: HirType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    pub fn lookup_var(&self, name: &str) -> Option<HirType> {
        for scope in self.scopes.iter().rev() {
            if let Some(&ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    pub fn lower_type_annotation(ty_ann: &Option<TypeAnnotation>) -> HirType {
        match ty_ann {
            Some(TypeAnnotation::Simple(name)) => match name.as_str() {
                "i64" | "int" | "Int" => HirType::I64,
                "f64" | "float" | "Float" => HirType::F64,
                "bool" | "Bool" => HirType::Bool,
                _ => HirType::I64,
            },
            _ => HirType::I64,
        }
    }

    pub fn lower_program(&mut self, program: &Program) -> Result<HirModule, Diagnostic> {
        let mut module = HirModule::new("main");

        // 1. First pass: register function signatures
        for stmt in &program.statements {
            if let StmtKind::Fn { name, params, ret_ty, .. } = &stmt.kind {
                let param_types: Vec<HirType> = params
                    .iter()
                    .map(|p| Self::lower_type_annotation(&p.ty))
                    .collect();
                let return_type = Self::lower_type_annotation(ret_ty);
                self.function_signatures.insert(name.clone(), (param_types, return_type));
            }
        }

        // 2. Second pass: lower function bodies
        for stmt in &program.statements {
            if let StmtKind::Fn { name, params, ret_ty, body, .. } = &stmt.kind {
                self.push_scope();

                let mut hir_params = Vec::new();
                for param in params {
                    let ty = Self::lower_type_annotation(&param.ty);
                    self.insert_var(param.name.clone(), ty);
                    hir_params.push(HirParam {
                        name: param.name.clone(),
                        ty,
                    });
                }

                let return_ty = Self::lower_type_annotation(ret_ty);
                let lowered_body = self.lower_expr(body);
                self.pop_scope();

                match lowered_body {
                    Ok(hir_body) => {
                        module.add_function(HirFunction {
                            name: name.clone(),
                            params: hir_params,
                            return_ty,
                            body: hir_body,
                        });
                    }
                    Err(diag) => {
                        // Isolate the failure to this one function rather than aborting
                        // lowering for the whole program: everything else here is still
                        // eligible for JIT/AOT (see `HirModule::skipped`).
                        module.skipped.push((name.clone(), diag));
                    }
                }
            }
        }

        // 3. Third pass: lower top-level non-Fn statements into a synthesized `__ghl_main` entry point function
        let top_level_stmts: Vec<&Stmt> = program
            .statements
            .iter()
            .filter(|s| !matches!(&s.kind, StmtKind::Fn { .. } | StmtKind::Use(_)))
            .collect();

        if !top_level_stmts.is_empty() {
            self.push_scope();
            let mut hir_stmts = Vec::with_capacity(top_level_stmts.len());
            let num_stmts = top_level_stmts.len();

            let (stmts_to_lower, trailing_expr) = if let StmtKind::Expr(e) = &top_level_stmts[num_stmts - 1].kind {
                (&top_level_stmts[..num_stmts - 1], Some(e))
            } else {
                (&top_level_stmts[..], None)
            };

            let mut lower_ok = true;
            for stmt in stmts_to_lower {
                match self.lower_stmt(stmt) {
                    Ok(s) => hir_stmts.push(s),
                    Err(_) => {
                        lower_ok = false;
                        break;
                    }
                }
            }

            if lower_ok {
                let result_pair = if let Some(e) = trailing_expr {
                    self.lower_expr(e).ok().map(|lowered| {
                        let ty = lowered.ty();
                        (Some(Box::new(lowered)), ty)
                    })
                } else {
                    Some((Some(Box::new(HirExpr::Literal(HirLiteral::I64(0), HirType::I64))), HirType::I64))
                };

                if let Some((result_expr, return_ty)) = result_pair {
                    let main_fn = HirFunction {
                        name: "__ghl_main".to_string(),
                        params: Vec::new(),
                        return_ty,
                        body: HirExpr::Block {
                            statements: hir_stmts,
                            result: result_expr,
                            ty: return_ty,
                        },
                    };
                    module.add_function(main_fn);
                }
            }

            self.pop_scope();
        }

        Ok(module)
    }

    pub fn lower_expr(&mut self, expr: &Expr) -> Result<HirExpr, Diagnostic> {
        match &expr.kind {
            ExprKind::Lit(lit) => match lit {
                Literal::Int(n) => Ok(HirExpr::Literal(HirLiteral::I64(*n), HirType::I64)),
                Literal::Float(f) => Ok(HirExpr::Literal(HirLiteral::F64(*f), HirType::F64)),
                Literal::Bool(b) => Ok(HirExpr::Literal(HirLiteral::Bool(*b), HirType::Bool)),
                _ => Err(Diagnostic::compute_error(
                    "C0301",
                    "Literal type not directly supported in scalar JIT",
                )),
            },
            ExprKind::Ident(name) => {
                let ty = self.lookup_var(name).unwrap_or(HirType::I64);
                Ok(HirExpr::Var(name.clone(), ty))
            }
            ExprKind::Path(segments) => {
                let name = segments.join("::");
                let ty = self.lookup_var(&name).unwrap_or(HirType::I64);
                Ok(HirExpr::Var(name, ty))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let lhs_hir = self.lower_expr(lhs)?;
                let rhs_hir = self.lower_expr(rhs)?;

                let (hir_op, result_ty) = match op {
                    BinaryOp::Add => (HirBinaryOp::Add, lhs_hir.ty()),
                    BinaryOp::Sub => (HirBinaryOp::Sub, lhs_hir.ty()),
                    BinaryOp::Mul => (HirBinaryOp::Mul, lhs_hir.ty()),
                    BinaryOp::Div => (HirBinaryOp::Div, lhs_hir.ty()),
                    BinaryOp::Mod => (HirBinaryOp::Mod, lhs_hir.ty()),

                    BinaryOp::Eq => (HirBinaryOp::Eq, HirType::Bool),
                    BinaryOp::NotEq => (HirBinaryOp::Ne, HirType::Bool),
                    BinaryOp::Lt => (HirBinaryOp::Lt, HirType::Bool),
                    BinaryOp::LtEq => (HirBinaryOp::Le, HirType::Bool),
                    BinaryOp::Gt => (HirBinaryOp::Gt, HirType::Bool),
                    BinaryOp::GtEq => (HirBinaryOp::Ge, HirType::Bool),

                    BinaryOp::And => (HirBinaryOp::And, HirType::Bool),
                    BinaryOp::Or | BinaryOp::BitOr => (HirBinaryOp::Or, HirType::Bool),
                    _ => {
                        return Err(Diagnostic::compute_error(
                            "C0302",
                            format!("Binary operator {:?} not supported in scalar JIT", op),
                        ))
                    }
                };

                Ok(HirExpr::Binary {
                    op: hir_op,
                    lhs: Box::new(lhs_hir),
                    rhs: Box::new(rhs_hir),
                    ty: result_ty,
                })
            }
            ExprKind::UnaryNeg(inner) => {
                let inner_hir = self.lower_expr(inner)?;
                let ty = inner_hir.ty();
                Ok(HirExpr::Unary {
                    op: HirUnaryOp::Neg,
                    operand: Box::new(inner_hir),
                    ty,
                })
            }
            ExprKind::UnaryNot(inner) => {
                let inner_hir = self.lower_expr(inner)?;
                Ok(HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    operand: Box::new(inner_hir),
                    ty: HirType::Bool,
                })
            }
            ExprKind::Call { callee, args } => {
                let func_name = match &callee.kind {
                    ExprKind::Ident(id) => id.clone(),
                    ExprKind::Path(segments) => segments.join("::"),
                    _ => {
                        return Err(Diagnostic::compute_error(
                            "C0303",
                            "Indirect function calls not yet supported in scalar JIT",
                        ))
                    }
                };

                let mut lowered_args = Vec::new();
                for arg in args {
                    lowered_args.push(self.lower_expr(arg)?);
                }

                let ret_ty = self
                    .function_signatures
                    .get(&func_name)
                    .map(|(_, ret)| *ret)
                    .unwrap_or(HirType::I64);

                Ok(HirExpr::Call {
                    func: func_name,
                    args: lowered_args,
                    ty: ret_ty,
                })
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                let cond_hir = self.lower_expr(cond)?;
                let then_hir = self.lower_expr(then_branch)?;
                let else_hir = if let Some(el) = else_branch {
                    self.lower_expr(el)?
                } else {
                    HirExpr::Literal(HirLiteral::I64(0), HirType::Unit)
                };

                let ty = then_hir.ty();
                Ok(HirExpr::IfElse {
                    cond: Box::new(cond_hir),
                    then_branch: Box::new(then_hir),
                    else_branch: Box::new(else_hir),
                    ty,
                })
            }
            ExprKind::Block { stmts, expr } => {
                self.push_scope();
                let mut hir_stmts = Vec::new();
                for stmt in stmts {
                    if matches!(&stmt.kind, StmtKind::Use(_)) {
                        continue;
                    }
                    hir_stmts.push(self.lower_stmt(stmt)?);
                }

                let result = if let Some(res_expr) = expr {
                    let lowered = self.lower_expr(res_expr)?;
                    Some(Box::new(lowered))
                } else {
                    None
                };

                let ty = result.as_ref().map(|e| e.ty()).unwrap_or(HirType::Unit);
                self.pop_scope();

                Ok(HirExpr::Block {
                    statements: hir_stmts,
                    result,
                    ty,
                })
            }
            ExprKind::While { cond, body } => {
                let cond_hir = self.lower_expr(cond)?;
                let body_hir = self.lower_expr(body)?;
                Ok(HirExpr::While {
                    cond: Box::new(cond_hir),
                    body: Box::new(body_hir),
                    ty: HirType::Unit,
                })
            }
            ExprKind::For { var, start, end, body } => {
                let start_hir = self.lower_expr(start)?;
                let end_hir = self.lower_expr(end)?;
                self.push_scope();
                self.insert_var(var.clone(), HirType::I64);
                let body_hir = self.lower_expr(body)?;
                self.pop_scope();

                Ok(HirExpr::For {
                    var: var.clone(),
                    start: Box::new(start_hir),
                    end: Box::new(end_hir),
                    body: Box::new(body_hir),
                    ty: HirType::Unit,
                })
            }
            _ => Err(Diagnostic::compute_error(
                "C0304",
                "Expression not currently eligible for scalar JIT lowering",
            )),
        }
    }

    pub fn lower_stmt(&mut self, stmt: &Stmt) -> Result<HirStatement, Diagnostic> {
        match &stmt.kind {
            StmtKind::Let { name, ty, init, .. } => {
                let init_hir = self.lower_expr(init)?;
                let var_ty = ty
                    .as_ref()
                    .map(|t| Self::lower_type_annotation(&Some(t.clone())))
                    .unwrap_or_else(|| init_hir.ty());
                self.insert_var(name.clone(), var_ty);

                Ok(HirStatement::Let {
                    name: name.clone(),
                    ty: var_ty,
                    value: init_hir,
                })
            }
            StmtKind::Expr(expr) => {
                let expr_hir = self.lower_expr(expr)?;
                Ok(HirStatement::Expr(expr_hir))
            }
            StmtKind::Return(ret_opt) => {
                let val = if let Some(e) = ret_opt {
                    Some(self.lower_expr(e)?)
                } else {
                    None
                };
                Ok(HirStatement::Return(val))
            }
            StmtKind::Break => Ok(HirStatement::Break),
            StmtKind::Continue => Ok(HirStatement::Continue),
            StmtKind::Assign { name, op, value } => {
                let value_hir = if let Some(bin_op) = op.to_binary_op() {
                    let lhs_hir = self.lower_expr(&Expr::new(ExprKind::Ident(name.clone()), value.span.clone()))?;
                    let rhs_hir = self.lower_expr(value)?;
                    let hir_op = match bin_op {
                        BinaryOp::Add => HirBinaryOp::Add,
                        BinaryOp::Sub => HirBinaryOp::Sub,
                        BinaryOp::Mul => HirBinaryOp::Mul,
                        BinaryOp::Div => HirBinaryOp::Div,
                        _ => return Err(Diagnostic::compute_error("C0305", "Unsupported compound operator in scalar JIT")),
                    };
                    let ty = lhs_hir.ty().clone();
                    HirExpr::Binary {
                        op: hir_op,
                        lhs: Box::new(lhs_hir),
                        rhs: Box::new(rhs_hir),
                        ty,
                    }
                } else {
                    self.lower_expr(value)?
                };
                Ok(HirStatement::Assign {
                    name: name.clone(),
                    value: value_hir,
                })
            }
            StmtKind::Use(_) => {
                // Static imports are handled during symbol resolution
                Ok(HirStatement::Expr(HirExpr::Literal(HirLiteral::I64(0), HirType::Unit)))
            }
            _ => Err(Diagnostic::compute_error(
                "C0305",
                "Statement not supported in scalar JIT function body",
            )),
        }
    }
}

pub fn lower_ast(program: &Program) -> Result<HirModule, Diagnostic> {
    let mut ctx = LoweringContext::new();
    ctx.lower_program(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lower_simple_add_fn() {
        let code = r#"
            fn add_nums(a, b) {
                let sum = a + b;
                sum
            }
        "#;
        let program = ghl_syntax::parse(code).expect("syntax ok");
        let module = lower_ast(&program).expect("lowering ok");

        assert!(module.functions.contains_key("add_nums"));
        let func = &module.functions["add_nums"];
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.return_ty, HirType::I64);
    }

    #[test]
    fn test_lower_fibonacci_recursion() {
        let code = r#"
            fn fib(n) {
                if n <= 1 {
                    n
                } else {
                    fib(n - 1) + fib(n - 2)
                }
            }
        "#;
        let program = ghl_syntax::parse(code).expect("syntax ok");
        let module = lower_ast(&program).expect("lowering ok");

        assert!(module.functions.contains_key("fib"));
    }

    #[test]
    fn test_lower_top_level_script_to_ghl_main() {
        let code = r#"
            let a = 10;
            let b = 25;
            a + b;
        "#;
        let program = ghl_syntax::parse(code).expect("syntax ok");
        let module = lower_ast(&program).expect("lowering ok");

        assert!(module.functions.contains_key("__ghl_main"));
        let main_fn = &module.functions["__ghl_main"];
        assert_eq!(main_fn.params.len(), 0);
        assert_eq!(main_fn.return_ty, HirType::I64);
    }

    /// One function using an expression outside the scalar JIT subset (here, a
    /// string literal) must not prevent an unrelated, fully scalar function in
    /// the same file from lowering — only the offending function is excluded
    /// (and reported via `HirModule::skipped`), not the whole module.
    #[test]
    fn test_lower_isolates_failure_to_one_function() {
        let code = r#"
            fn add_nums(a: int, b: int) -> int {
                a + b
            }

            fn greet() -> str {
                "hello"
            }
        "#;
        let program = ghl_syntax::parse(code).expect("syntax ok");
        let module = lower_ast(&program).expect("lowering ok");

        assert!(module.functions.contains_key("add_nums"), "unaffected function must still lower");
        assert!(!module.functions.contains_key("greet"), "incompatible function must not be in the module");
        assert_eq!(module.skipped.len(), 1);
        assert_eq!(module.skipped[0].0, "greet");
    }
}

