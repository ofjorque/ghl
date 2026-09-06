//! High-performance JIT execution engine backed by Cranelift.

use std::collections::HashMap;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use ghl_diagnostics::Diagnostic;
use ghl_ir::HirModule;
use crate::compiler::FunctionCompiler;

pub struct JitEngine {
    pub module: JITModule,
    pub ctx: codegen::Context,
    pub fn_builder_ctx: FunctionBuilderContext,
    pub compiled_ptrs: HashMap<String, *const u8>,
}

impl Default for JitEngine {
    fn default() -> Self {
        Self::new().expect("Failed to initialize native Cranelift JIT engine")
    }
}

impl JitEngine {
    /// Initialize a new Cranelift JIT engine targeting the native host ISA.
    pub fn new() -> Result<Self, Diagnostic> {
        let mut flag_builder = settings::builder();
        flag_builder
            .set("use_colocated_libcalls", "false")
            .map_err(|e| Diagnostic::compute_error("C0410", format!("Cranelift flag error: {e}")))?;
        flag_builder
            .set("is_pic", "false")
            .map_err(|e| Diagnostic::compute_error("C0411", format!("Cranelift flag error: {e}")))?;

        let isa_builder = cranelift_native::builder().map_err(|e| {
            Diagnostic::compute_error("C0412", format!("Host ISA unsupported by Cranelift: {e}"))
        })?;

        let isa = isa_builder.finish(settings::Flags::new(flag_builder)).map_err(|e| {
            Diagnostic::compute_error("C0413", format!("Failed to configure native ISA: {e}"))
        })?;

        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let module = JITModule::new(builder);
        let ctx = module.make_context();
        let fn_builder_ctx = FunctionBuilderContext::new();

        Ok(Self {
            module,
            ctx,
            fn_builder_ctx,
            compiled_ptrs: HashMap::new(),
        })
    }

    /// Compile all functions in a `HirModule` to executable native machine code.
    pub fn compile_module(&mut self, hir: &HirModule) -> Result<&HashMap<String, *const u8>, Diagnostic> {
        let mut func_ids = HashMap::new();
        let mut signatures = HashMap::new();

        // 1. First pass: declare all function signatures in the module
        for (name, func) in &hir.functions {
            let mut sig = self.module.make_signature();
            for param in &func.params {
                sig.params
                    .push(AbiParam::new(FunctionCompiler::<JITModule>::to_clif_type(param.ty)));
            }
            sig.returns
                .push(AbiParam::new(FunctionCompiler::<JITModule>::to_clif_type(func.return_ty)));

            let func_id = self
                .module
                .declare_function(name, Linkage::Export, &sig)
                .map_err(|e| {
                    Diagnostic::compute_error("C0414", format!("Failed to declare JIT function `{name}`: {e}"))
                })?;

            func_ids.insert(name.clone(), func_id);
            signatures.insert(name.clone(), sig);
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
                Diagnostic::compute_error("C0415", format!("Failed to define JIT function `{name}`: {e}"))
            })?;

            self.module.clear_context(&mut self.ctx);
        }

        // 3. Finalize definitions and commit executable memory pages
        self.module.finalize_definitions().map_err(|e| {
            Diagnostic::compute_error("C0416", format!("Failed to finalize JIT module definitions: {e}"))
        })?;

        // 4. Retrieve native code pointers
        for (name, func_id) in func_ids {
            let code_ptr = self.module.get_finalized_function(func_id);
            self.compiled_ptrs.insert(name, code_ptr);
        }

        Ok(&self.compiled_ptrs)
    }

    /// Retrieve raw function pointer for a compiled function.
    pub fn get_fn_ptr(&self, name: &str) -> Option<*const u8> {
        self.compiled_ptrs.get(name).copied()
    }

    /// Retrieve a callable native function taking 1 i64 and returning i64.
    pub fn get_fn_i64_1(&self, name: &str) -> Option<extern "C" fn(i64) -> i64> {
        self.get_fn_ptr(name).map(|ptr| unsafe { std::mem::transmute(ptr) })
    }

    /// Retrieve a callable native function taking 2 i64 and returning i64.
    pub fn get_fn_i64_2(&self, name: &str) -> Option<extern "C" fn(i64, i64) -> i64> {
        self.get_fn_ptr(name).map(|ptr| unsafe { std::mem::transmute(ptr) })
    }

    /// Retrieve a callable native function taking 1 f64 and returning f64.
    pub fn get_fn_f64_1(&self, name: &str) -> Option<extern "C" fn(f64) -> f64> {
        self.get_fn_ptr(name).map(|ptr| unsafe { std::mem::transmute(ptr) })
    }

    /// Retrieve a callable native function taking 2 f64 and returning f64.
    pub fn get_fn_f64_2(&self, name: &str) -> Option<extern "C" fn(f64, f64) -> f64> {
        self.get_fn_ptr(name).map(|ptr| unsafe { std::mem::transmute(ptr) })
    }
}

