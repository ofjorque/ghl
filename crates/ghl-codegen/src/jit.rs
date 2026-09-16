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

extern "C" fn ghl_host_clock_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

extern "C" fn ghl_host_sqrt(x: f64) -> f64 {
    x.sqrt()
}

extern "C" fn ghl_host_sin(x: f64) -> f64 {
    x.sin()
}

extern "C" fn ghl_host_cos(x: f64) -> f64 {
    x.cos()
}

extern "C" fn ghl_host_exp(x: f64) -> f64 {
    x.exp()
}

extern "C" fn ghl_host_ln(x: f64) -> f64 {
    x.ln()
}

extern "C" fn ghl_host_pow(x: f64, y: f64) -> f64 {
    x.powf(y)
}

extern "C" fn ghl_host_abs(x: f64) -> f64 {
    x.abs()
}

extern "C" fn ghl_host_floor(x: f64) -> f64 {
    x.floor()
}

extern "C" fn ghl_host_ceil(x: f64) -> f64 {
    x.ceil()
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

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        builder.symbol("clock_now", ghl_host_clock_now as *const u8);
        builder.symbol("sqrt", ghl_host_sqrt as *const u8);
        builder.symbol("sin", ghl_host_sin as *const u8);
        builder.symbol("cos", ghl_host_cos as *const u8);
        builder.symbol("exp", ghl_host_exp as *const u8);
        builder.symbol("ln", ghl_host_ln as *const u8);
        builder.symbol("pow", ghl_host_pow as *const u8);
        builder.symbol("abs", ghl_host_abs as *const u8);
        builder.symbol("floor", ghl_host_floor as *const u8);
        builder.symbol("ceil", ghl_host_ceil as *const u8);

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

        // Register host math functions in module
        let host_math_unary = ["sqrt", "sin", "cos", "exp", "ln", "abs", "floor", "ceil"];
        for name in host_math_unary {
            let mut sig = self.module.make_signature();
            sig.params.push(AbiParam::new(types::F64));
            sig.returns.push(AbiParam::new(types::F64));
            if let Ok(id) = self.module.declare_function(name, Linkage::Import, &sig) {
                func_ids.insert(name.to_string(), id);
            }
        }

        let mut pow_sig = self.module.make_signature();
        pow_sig.params.push(AbiParam::new(types::F64));
        pow_sig.params.push(AbiParam::new(types::F64));
        pow_sig.returns.push(AbiParam::new(types::F64));
        if let Ok(id) = self.module.declare_function("pow", Linkage::Import, &pow_sig) {
            func_ids.insert("pow".to_string(), id);
        }

        let mut clock_sig = self.module.make_signature();
        clock_sig.returns.push(AbiParam::new(types::F64));
        if let Ok(id) = self.module.declare_function("clock_now", Linkage::Import, &clock_sig) {
            func_ids.insert("clock_now".to_string(), id);
        }

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

        // 4. Retrieve native code pointers for defined module functions
        for name in hir.functions.keys() {
            if let Some(&func_id) = func_ids.get(name) {
                let code_ptr = self.module.get_finalized_function(func_id);
                self.compiled_ptrs.insert(name.clone(), code_ptr);
            }
        }

        Ok(&self.compiled_ptrs)
    }

    /// Retrieve raw function pointer for a compiled function.
    pub fn get_fn_ptr(&self, name: &str) -> Option<*const u8> {
        self.compiled_ptrs.get(name).copied()
    }

    /// Retrieve a callable native function taking 0 arguments and returning i64.
    pub fn get_fn_i64_0(&self, name: &str) -> Option<extern "C" fn() -> i64> {
        self.get_fn_ptr(name).map(|ptr| unsafe { std::mem::transmute(ptr) })
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

