//! High-performance Ahead-Of-Time (AOT) native object generation engine.
//!
//! Emits raw machine-code object files (`.obj` on Windows, `.o` on Unix)
//! using `cranelift-object` and the shared generic [`FunctionCompiler`].

use std::collections::HashMap;
use cranelift::prelude::*;
use cranelift_module::{Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use ghl_diagnostics::Diagnostic;
use ghl_ir::HirModule;
use crate::compiler::FunctionCompiler;

pub struct AotEngine {
    pub module: ObjectModule,
    pub ctx: codegen::Context,
    pub fn_builder_ctx: FunctionBuilderContext,
}

impl AotEngine {
    /// Initialize a new Cranelift AOT object engine targeting the host ISA.
    pub fn new(module_name: &str) -> Result<Self, Diagnostic> {
        let mut flag_builder = settings::builder();
        flag_builder
            .set("use_colocated_libcalls", "false")
            .map_err(|e| Diagnostic::compute_error("C0420", format!("Cranelift flag error: {e}")))?;

        let isa_builder = cranelift_native::builder().map_err(|e| {
            Diagnostic::compute_error("C0421", format!("Host ISA unsupported by Cranelift: {e}"))
        })?;

        let isa = isa_builder.finish(settings::Flags::new(flag_builder)).map_err(|e| {
            Diagnostic::compute_error("C0422", format!("Failed to configure native ISA: {e}"))
        })?;

        let builder = ObjectBuilder::new(isa, module_name, cranelift_module::default_libcall_names())
            .map_err(|e| Diagnostic::compute_error("C0423", format!("Failed to create ObjectBuilder: {e}")))?;

        let module = ObjectModule::new(builder);
        let ctx = module.make_context();
        let fn_builder_ctx = FunctionBuilderContext::new();

        Ok(Self {
            module,
            ctx,
            fn_builder_ctx,
        })
    }

    /// Compile all functions in a `HirModule` and emit the raw object file bytes (`.obj` / `.o`).
    pub fn compile_module(mut self, hir: &HirModule) -> Result<Vec<u8>, Diagnostic> {
        let mut func_ids = HashMap::new();
        let mut signatures = HashMap::new();

        // 1. First pass: declare all function signatures with exported linkage
        for (name, func) in &hir.functions {
            let mut sig = self.module.make_signature();
            for param in &func.params {
                sig.params.push(AbiParam::new(FunctionCompiler::<ObjectModule>::to_clif_type(param.ty)));
            }

            sig.returns
                .push(AbiParam::new(FunctionCompiler::<ObjectModule>::to_clif_type(func.return_ty)));

            let func_id = self.module.declare_function(name, Linkage::Export, &sig).map_err(|e| {
                Diagnostic::compute_error("C0424", format!("Failed to declare AOT function `{name}`: {e}"))
            })?;

            signatures.insert(name.clone(), sig);
            func_ids.insert(name.clone(), func_id);
        }

        // 2. Second pass: compile and define function bodies
        for (name, func) in &hir.functions {
            let func_id = func_ids[name];
            let sig = signatures[name].clone();

            self.ctx.func.signature = sig;
            {
                let builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.fn_builder_ctx);
                let compiler = FunctionCompiler::new(builder, &mut self.module, &func_ids);
                compiler.compile_function(func)?;
            }

            self.module.define_function(func_id, &mut self.ctx).map_err(|e| {
                Diagnostic::compute_error("C0425", format!("Failed to define AOT function `{name}`: {e}"))
            })?;

            self.module.clear_context(&mut self.ctx);
        }

        // 3. Finalize and produce object file product
        let product = self.module.finish();
        product.emit().map_err(|e| {
            Diagnostic::compute_error("C0426", format!("Failed to emit object bytes: {e}"))
        })
    }
}
