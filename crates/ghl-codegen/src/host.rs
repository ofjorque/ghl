//! Single source of truth for host (Rust-implemented) library functions callable
//! from GHL as C-ABI imports — shared by both the JIT and AOT Cranelift backends.
//!
//! Adding, removing, or renaming a host function only requires editing
//! [`HOST_FUNCTIONS`] here; both [`crate::jit::JitEngine`] and
//! [`crate::aot::AotEngine`] derive their import declarations from this table.

use std::collections::HashMap;
use cranelift::prelude::*;
use cranelift_module::{FuncId, Linkage, Module};
use ghl_diagnostics::Diagnostic;

#[derive(Clone, Copy)]
pub enum HostArity {
    Zero,
    Unary,
    Binary,
}

pub struct HostFn {
    pub name: &'static str,
    pub arity: HostArity,
    pub ptr: *const u8,
    /// Rust expression body (e.g. `"x.sqrt()"`) mirroring `ptr`'s implementation,
    /// used by [`runner_source_stub`] to give AOT-linked binaries a native
    /// definition of this symbol without linking against `ghl-codegen` itself.
    pub rust_body: &'static str,
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

pub const HOST_FUNCTIONS: &[HostFn] = &[
    HostFn {
        name: "clock_now", arity: HostArity::Zero, ptr: ghl_host_clock_now as *const u8,
        rust_body: "std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)",
    },
    HostFn { name: "sqrt", arity: HostArity::Unary, ptr: ghl_host_sqrt as *const u8, rust_body: "x.sqrt()" },
    HostFn { name: "sin", arity: HostArity::Unary, ptr: ghl_host_sin as *const u8, rust_body: "x.sin()" },
    HostFn { name: "cos", arity: HostArity::Unary, ptr: ghl_host_cos as *const u8, rust_body: "x.cos()" },
    HostFn { name: "exp", arity: HostArity::Unary, ptr: ghl_host_exp as *const u8, rust_body: "x.exp()" },
    HostFn { name: "ln", arity: HostArity::Unary, ptr: ghl_host_ln as *const u8, rust_body: "x.ln()" },
    HostFn { name: "abs", arity: HostArity::Unary, ptr: ghl_host_abs as *const u8, rust_body: "x.abs()" },
    HostFn { name: "floor", arity: HostArity::Unary, ptr: ghl_host_floor as *const u8, rust_body: "x.floor()" },
    HostFn { name: "ceil", arity: HostArity::Unary, ptr: ghl_host_ceil as *const u8, rust_body: "x.ceil()" },
    HostFn { name: "pow", arity: HostArity::Binary, ptr: ghl_host_pow as *const u8, rust_body: "x.powf(y)" },
];

fn signature_for<M: Module>(module: &M, arity: HostArity) -> Signature {
    let mut sig = module.make_signature();
    match arity {
        HostArity::Zero => {}
        HostArity::Unary => sig.params.push(AbiParam::new(types::F64)),
        HostArity::Binary => {
            sig.params.push(AbiParam::new(types::F64));
            sig.params.push(AbiParam::new(types::F64));
        }
    }
    sig.returns.push(AbiParam::new(types::F64));
    sig
}

/// Declare every entry in [`HOST_FUNCTIONS`] as a C-ABI import on `module`,
/// inserting the resulting `FuncId`s into `func_ids` so `FunctionCompiler`
/// can resolve GHL calls to them, regardless of backend (JIT or AOT).
pub fn declare_host_imports<M: Module>(
    module: &mut M,
    func_ids: &mut HashMap<String, FuncId>,
) -> Result<(), Diagnostic> {
    for host_fn in HOST_FUNCTIONS {
        let sig = signature_for(module, host_fn.arity);
        let id = module.declare_function(host_fn.name, Linkage::Import, &sig).map_err(|e| {
            Diagnostic::compute_error(
                "C0417",
                format!("Failed to declare host import `{}`: {e}", host_fn.name),
            )
        })?;
        func_ids.insert(host_fn.name.to_string(), id);
    }
    Ok(())
}

/// Register native symbol addresses for every entry in [`HOST_FUNCTIONS`] on a
/// JIT builder, so the JIT linker can resolve the imports declared by
/// [`declare_host_imports`] to their actual Rust implementations.
pub fn register_jit_symbols(builder: &mut cranelift_jit::JITBuilder) {
    for host_fn in HOST_FUNCTIONS {
        builder.symbol(host_fn.name, host_fn.ptr);
    }
}

/// Generate Rust source defining every entry in [`HOST_FUNCTIONS`] as a
/// `#[no_mangle] extern "C"` function. `AotEngine::compile_module` declares all
/// of them as C-ABI imports regardless of whether the source script calls them
/// (see [`declare_host_imports`]), so the linker driver (`ghl-cli`'s `build`
/// command) must splice this stub into every generated runner it compiles with
/// `rustc`, or linking will fail on unresolved symbols even for scripts that
/// never use host math.
pub fn runner_source_stub() -> String {
    let mut out = String::new();
    for host_fn in HOST_FUNCTIONS {
        let params = match host_fn.arity {
            HostArity::Zero => String::new(),
            HostArity::Unary => "x: f64".to_string(),
            HostArity::Binary => "x: f64, y: f64".to_string(),
        };
        out.push_str(&format!(
            "#[no_mangle]\npub extern \"C\" fn {name}({params}) -> f64 {{ {body} }}\n",
            name = host_fn.name,
            body = host_fn.rust_body,
        ));
    }
    out
}
