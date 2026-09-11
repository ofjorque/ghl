//! Cranelift IR compiler for GHL HIR functions.

use std::collections::HashMap;
use cranelift::prelude::*;
use cranelift_module::{FuncId, Module};
use ghl_diagnostics::Diagnostic;
use ghl_ir::{HirBinaryOp, HirExpr, HirFunction, HirLiteral, HirStatement, HirType, HirUnaryOp};

pub struct FunctionCompiler<'a, M: Module> {
    pub builder: FunctionBuilder<'a>,
    pub module: &'a mut M,
    pub func_ids: &'a HashMap<String, FuncId>,
    pub var_map: HashMap<String, Variable>,
}

impl<'a, M: Module> FunctionCompiler<'a, M> {
    pub fn new(
        builder: FunctionBuilder<'a>,
        module: &'a mut M,
        func_ids: &'a HashMap<String, FuncId>,
    ) -> Self {
        Self {
            builder,
            module,
            func_ids,
            var_map: HashMap::new(),
        }
    }

    pub fn to_clif_type(ty: HirType) -> Type {
        match ty {
            HirType::I64 => types::I64,
            HirType::F64 => types::F64,
            HirType::Bool => types::I8,
            HirType::Unit => types::I64, // Represent unit as 0 i64 in register
        }
    }

    fn is_current_block_terminated(&self) -> bool {
        if let Some(block) = self.builder.current_block() {
            if let Some(inst) = self.builder.func.layout.last_inst(block) {
                return self.builder.func.dfg.insts[inst].opcode().is_terminator();
            }
        }
        false
    }

    pub fn compile_function(mut self, func: &HirFunction) -> Result<(), Diagnostic> {
        let entry_block = self.builder.create_block();
        self.builder.append_block_params_for_function_params(entry_block);
        self.builder.switch_to_block(entry_block);
        self.builder.seal_block(entry_block);

        // Declare parameters as local variables
        for (i, param) in func.params.iter().enumerate() {
            let clif_ty = Self::to_clif_type(param.ty);
            let var = self.builder.declare_var(clif_ty);

            let param_val = self.builder.block_params(entry_block)[i];
            self.builder.def_var(var, param_val);
            self.var_map.insert(param.name.clone(), var);
        }

        let ret_val = self.compile_expr(&func.body)?;
        if !self.is_current_block_terminated() {
            self.builder.ins().return_(&[ret_val]);
        }
        let target_config = self.module.target_config();
        self.builder.finalize(target_config);

        Ok(())
    }

    pub fn compile_expr(&mut self, expr: &HirExpr) -> Result<Value, Diagnostic> {
        match expr {
            HirExpr::Literal(lit, _ty) => match lit {
                HirLiteral::I64(n) => Ok(self.builder.ins().iconst(types::I64, *n)),
                HirLiteral::F64(f) => Ok(self.builder.ins().f64const(*f)),
                HirLiteral::Bool(b) => {
                    let n = if *b { 1 } else { 0 };
                    Ok(self.builder.ins().iconst(types::I8, n))
                }
            },
            HirExpr::Var(name, _) => {
                if let Some(&var) = self.var_map.get(name) {
                    Ok(self.builder.use_var(var))
                } else {
                    Err(Diagnostic::compute_error(
                        "C0401",
                        format!("Unknown variable `{name}` in JIT compilation"),
                    ))
                }
            }
            HirExpr::Binary { op, lhs, rhs, .. } => {
                let lhs_val = self.compile_expr(lhs)?;
                let rhs_val = self.compile_expr(rhs)?;

                if lhs.ty().is_float() || rhs.ty().is_float() {
                    self.compile_float_binary(*op, lhs_val, rhs_val)
                } else {
                    self.compile_int_binary(*op, lhs_val, rhs_val)
                }
            }
            HirExpr::Unary { op, operand, .. } => {
                let val = self.compile_expr(operand)?;
                match op {
                    HirUnaryOp::Neg => {
                        if operand.ty().is_float() {
                            Ok(self.builder.ins().fneg(val))
                        } else {
                            Ok(self.builder.ins().ineg(val))
                        }
                    }
                    HirUnaryOp::Not => {
                        let one = self.builder.ins().iconst(types::I8, 1);
                        Ok(self.builder.ins().bxor(val, one))
                    }
                }
            }
            HirExpr::Call { func, args, .. } => {
                let func_id = self.func_ids.get(func).ok_or_else(|| {
                    Diagnostic::compute_error(
                        "C0402",
                        format!("JIT call to undefined function `{func}`"),
                    )
                })?;

                let func_ref = self.module.declare_func_in_func(*func_id, self.builder.func);
                let mut arg_vals = Vec::with_capacity(args.len());
                for arg in args {
                    arg_vals.push(self.compile_expr(arg)?);
                }

                let call_inst = self.builder.ins().call(func_ref, &arg_vals);
                let results = self.builder.inst_results(call_inst);
                if results.is_empty() {
                    Ok(self.builder.ins().iconst(types::I64, 0))
                } else {
                    Ok(results[0])
                }
            }
            HirExpr::IfElse { cond, then_branch, else_branch, ty } => {
                let cond_val = self.compile_expr(cond)?;

                let then_block = self.builder.create_block();
                let else_block = self.builder.create_block();
                let merge_block = self.builder.create_block();

                let clif_ty = Self::to_clif_type(*ty);
                let res_var = self.builder.declare_var(clif_ty);

                // Branch condition: if cond != 0 goto then_block else goto else_block
                self.builder.ins().brif(cond_val, then_block, &[], else_block, &[]);

                // 1. Then branch
                self.builder.switch_to_block(then_block);
                self.builder.seal_block(then_block);
                let then_res = self.compile_expr(then_branch)?;
                let then_filled = self.is_current_block_terminated();
                if !then_filled {
                    self.builder.def_var(res_var, then_res);
                    self.builder.ins().jump(merge_block, &[]);
                }

                // 2. Else branch
                self.builder.switch_to_block(else_block);
                self.builder.seal_block(else_block);
                let else_res = self.compile_expr(else_branch)?;
                let else_filled = self.is_current_block_terminated();
                if !else_filled {
                    self.builder.def_var(res_var, else_res);
                    self.builder.ins().jump(merge_block, &[]);
                }

                // 3. Merge block
                self.builder.switch_to_block(merge_block);
                self.builder.seal_block(merge_block);

                if then_filled && else_filled {
                    let dummy = match clif_ty {
                        types::F64 => self.builder.ins().f64const(0.0),
                        types::I8 => self.builder.ins().iconst(types::I8, 0),
                        _ => self.builder.ins().iconst(types::I64, 0),
                    };
                    self.builder.def_var(res_var, dummy);
                    Ok(dummy)
                } else {
                    Ok(self.builder.use_var(res_var))
                }
            }
            HirExpr::Block { statements, result, .. } => {
                for stmt in statements {
                    self.compile_stmt(stmt)?;
                }
                if let Some(res_expr) = result {
                    self.compile_expr(res_expr)
                } else {
                    Ok(self.builder.ins().iconst(types::I64, 0))
                }
            }
        }
    }

    pub fn compile_stmt(&mut self, stmt: &HirStatement) -> Result<(), Diagnostic> {
        match stmt {
            HirStatement::Let { name, ty, value } => {
                let clif_ty = Self::to_clif_type(*ty);
                let var = self.builder.declare_var(clif_ty);

                let val = self.compile_expr(value)?;
                self.builder.def_var(var, val);
                self.var_map.insert(name.clone(), var);
                Ok(())
            }
            HirStatement::Assign { name, value } => {
                let val = self.compile_expr(value)?;
                if let Some(&var) = self.var_map.get(name) {
                    self.builder.def_var(var, val);
                    Ok(())
                } else {
                    Err(Diagnostic::compute_error(
                        "C0403",
                        format!("Cannot assign to undeclared variable `{name}` in JIT"),
                    ))
                }
            }
            HirStatement::Expr(expr) => {
                self.compile_expr(expr)?;
                Ok(())
            }
            HirStatement::Return(expr_opt) => {
                if self.is_current_block_terminated() {
                    return Ok(());
                }
                if let Some(e) = expr_opt {
                    let val = self.compile_expr(e)?;
                    if !self.is_current_block_terminated() {
                        self.builder.ins().return_(&[val]);
                    }
                } else {
                    let zero = self.builder.ins().iconst(types::I64, 0);
                    self.builder.ins().return_(&[zero]);
                }
                // Switch to a fresh dead block to safely absorb any dead statements
                let dead_block = self.builder.create_block();
                self.builder.switch_to_block(dead_block);
                self.builder.seal_block(dead_block);
                Ok(())
            }
        }
    }

    fn compile_int_binary(&mut self, op: HirBinaryOp, lhs: Value, rhs: Value) -> Result<Value, Diagnostic> {
        match op {
            HirBinaryOp::Add => Ok(self.builder.ins().iadd(lhs, rhs)),
            HirBinaryOp::Sub => Ok(self.builder.ins().isub(lhs, rhs)),
            HirBinaryOp::Mul => Ok(self.builder.ins().imul(lhs, rhs)),
            HirBinaryOp::Div => Ok(self.builder.ins().sdiv(lhs, rhs)),
            HirBinaryOp::Mod => Ok(self.builder.ins().srem(lhs, rhs)),

            HirBinaryOp::Eq => Ok(self.builder.ins().icmp(IntCC::Equal, lhs, rhs)),
            HirBinaryOp::Ne => Ok(self.builder.ins().icmp(IntCC::NotEqual, lhs, rhs)),
            HirBinaryOp::Lt => Ok(self.builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs)),
            HirBinaryOp::Le => Ok(self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs)),
            HirBinaryOp::Gt => Ok(self.builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs)),
            HirBinaryOp::Ge => Ok(self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs)),

            HirBinaryOp::And => Ok(self.builder.ins().band(lhs, rhs)),
            HirBinaryOp::Or => Ok(self.builder.ins().bor(lhs, rhs)),
        }
    }

    fn compile_float_binary(&mut self, op: HirBinaryOp, lhs: Value, rhs: Value) -> Result<Value, Diagnostic> {
        match op {
            HirBinaryOp::Add => Ok(self.builder.ins().fadd(lhs, rhs)),
            HirBinaryOp::Sub => Ok(self.builder.ins().fsub(lhs, rhs)),
            HirBinaryOp::Mul => Ok(self.builder.ins().fmul(lhs, rhs)),
            HirBinaryOp::Div => Ok(self.builder.ins().fdiv(lhs, rhs)),
            HirBinaryOp::Mod => Err(Diagnostic::compute_error("C0404", "Modulo not supported on float values in JIT")),

            HirBinaryOp::Eq => Ok(self.builder.ins().fcmp(FloatCC::Equal, lhs, rhs)),
            HirBinaryOp::Ne => Ok(self.builder.ins().fcmp(FloatCC::NotEqual, lhs, rhs)),
            HirBinaryOp::Lt => Ok(self.builder.ins().fcmp(FloatCC::LessThan, lhs, rhs)),
            HirBinaryOp::Le => Ok(self.builder.ins().fcmp(FloatCC::LessThanOrEqual, lhs, rhs)),
            HirBinaryOp::Gt => Ok(self.builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs)),
            HirBinaryOp::Ge => Ok(self.builder.ins().fcmp(FloatCC::GreaterThanOrEqual, lhs, rhs)),

            _ => Err(Diagnostic::compute_error("C0405", "Unsupported operator for floats in JIT")),
        }
    }
}
