//! Bridge connecting Cranelift JIT compiled native code to GHL Runtime (RFC 13).
//!
//! Provides typed C-ABI invocation trampolines that unpack `ghl_runtime::Value`
//! arguments into native CPU registers, invoke the JIT machine code, and wrap
//! results back into `ghl_runtime::Value`.

use std::sync::Arc;
use ghl_codegen::JitEngine;
use ghl_diagnostics::Diagnostic;
use ghl_ir::{HirFunction, HirType};
use ghl_runtime::{JitFunction, Value};

fn val_to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::I64(n) => Some(*n),
        Value::F64(f) if f.fract() == 0.0 => Some(*f as i64),
        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
        _ => None,
    }
}

fn val_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::F64(f) => Some(*f),
        Value::I64(n) => Some(*n as f64),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Create a typed JIT trampoline for an exported HIR function.
/// Captures `jit_arc` to ensure executable memory pages remain committed.
pub fn create_jit_trampoline(
    hir_fn: &HirFunction,
    code_ptr: *const u8,
    jit_arc: Arc<JitEngine>,
) -> Option<JitFunction> {
    let params: Vec<HirType> = hir_fn.params.iter().map(|p| p.ty).collect();
    let ret = hir_fn.return_ty;
    let ptr_val = code_ptr as usize;

    match (params.as_slice(), ret) {
        // --- 0 Parameters ---
        ([], HirType::I64 | HirType::Bool) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if !args.is_empty() {
                    return Err(Diagnostic::compute_error("C0420", "Expected 0 arguments in native JIT call"));
                }
                let f: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr_val) };
                let res = f();
                Ok(if ret == HirType::Bool { Value::Bool(res != 0) } else { Value::I64(res) })
            }))
        }
        ([], HirType::F64) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if !args.is_empty() {
                    return Err(Diagnostic::compute_error("C0420", "Expected 0 arguments in native JIT call"));
                }
                let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(ptr_val) };
                Ok(Value::F64(f()))
            }))
        }

        // --- 1 Parameter ---
        ([HirType::I64 | HirType::Bool], HirType::I64 | HirType::Bool) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 1 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 1 argument in native JIT call"));
                }
                let a0 = val_to_i64(&args[0]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 1 must be an integer, found {:?}", args[0]))
                })?;
                let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(ptr_val) };
                let res = f(a0);
                Ok(if ret == HirType::Bool { Value::Bool(res != 0) } else { Value::I64(res) })
            }))
        }
        ([HirType::F64], HirType::F64) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 1 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 1 argument in native JIT call"));
                }
                let a0 = val_to_f64(&args[0]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 1 must be numeric, found {:?}", args[0]))
                })?;
                let f: extern "C" fn(f64) -> f64 = unsafe { std::mem::transmute(ptr_val) };
                Ok(Value::F64(f(a0)))
            }))
        }
        ([HirType::F64], HirType::I64 | HirType::Bool) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 1 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 1 argument in native JIT call"));
                }
                let a0 = val_to_f64(&args[0]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 1 must be numeric, found {:?}", args[0]))
                })?;
                let f: extern "C" fn(f64) -> i64 = unsafe { std::mem::transmute(ptr_val) };
                let res = f(a0);
                Ok(if ret == HirType::Bool { Value::Bool(res != 0) } else { Value::I64(res) })
            }))
        }

        // --- 2 Parameters ---
        ([HirType::I64 | HirType::Bool, HirType::I64 | HirType::Bool], HirType::I64 | HirType::Bool) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 2 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 2 arguments in native JIT call"));
                }
                let a0 = val_to_i64(&args[0]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 1 must be an integer, found {:?}", args[0]))
                })?;
                let a1 = val_to_i64(&args[1]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 2 must be an integer, found {:?}", args[1]))
                })?;
                let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(ptr_val) };
                let res = f(a0, a1);
                Ok(if ret == HirType::Bool { Value::Bool(res != 0) } else { Value::I64(res) })
            }))
        }
        ([HirType::F64, HirType::F64], HirType::F64) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 2 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 2 arguments in native JIT call"));
                }
                let a0 = val_to_f64(&args[0]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 1 must be numeric, found {:?}", args[0]))
                })?;
                let a1 = val_to_f64(&args[1]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 2 must be numeric, found {:?}", args[1]))
                })?;
                let f: extern "C" fn(f64, f64) -> f64 = unsafe { std::mem::transmute(ptr_val) };
                Ok(Value::F64(f(a0, a1)))
            }))
        }
        ([HirType::F64, HirType::F64], HirType::I64 | HirType::Bool) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 2 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 2 arguments in native JIT call"));
                }
                let a0 = val_to_f64(&args[0]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 1 must be numeric, found {:?}", args[0]))
                })?;
                let a1 = val_to_f64(&args[1]).ok_or_else(|| {
                    Diagnostic::compute_error("C0421", format!("Argument 2 must be numeric, found {:?}", args[1]))
                })?;
                let f: extern "C" fn(f64, f64) -> i64 = unsafe { std::mem::transmute(ptr_val) };
                let res = f(a0, a1);
                Ok(if ret == HirType::Bool { Value::Bool(res != 0) } else { Value::I64(res) })
            }))
        }

        // --- 3 Parameters ---
        ([HirType::I64 | HirType::Bool, HirType::I64 | HirType::Bool, HirType::I64 | HirType::Bool], HirType::I64 | HirType::Bool) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 3 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 3 arguments in native JIT call"));
                }
                let a0 = val_to_i64(&args[0]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 1 must be int"))?;
                let a1 = val_to_i64(&args[1]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 2 must be int"))?;
                let a2 = val_to_i64(&args[2]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 3 must be int"))?;
                let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(ptr_val) };
                let res = f(a0, a1, a2);
                Ok(if ret == HirType::Bool { Value::Bool(res != 0) } else { Value::I64(res) })
            }))
        }
        ([HirType::F64, HirType::F64, HirType::F64], HirType::F64) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 3 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 3 arguments in native JIT call"));
                }
                let a0 = val_to_f64(&args[0]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 1 must be float"))?;
                let a1 = val_to_f64(&args[1]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 2 must be float"))?;
                let a2 = val_to_f64(&args[2]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 3 must be float"))?;
                let f: extern "C" fn(f64, f64, f64) -> f64 = unsafe { std::mem::transmute(ptr_val) };
                Ok(Value::F64(f(a0, a1, a2)))
            }))
        }

        // --- 4 Parameters ---
        ([HirType::F64, HirType::F64, HirType::F64, HirType::F64], HirType::F64) => {
            let _jit = jit_arc;
            Some(JitFunction::new(move |args: Vec<Value>| {
                if args.len() != 4 {
                    return Err(Diagnostic::compute_error("C0420", "Expected 4 arguments in native JIT call"));
                }
                let a0 = val_to_f64(&args[0]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 1 must be float"))?;
                let a1 = val_to_f64(&args[1]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 2 must be float"))?;
                let a2 = val_to_f64(&args[2]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 3 must be float"))?;
                let a3 = val_to_f64(&args[3]).ok_or_else(|| Diagnostic::compute_error("C0421", "Arg 4 must be float"))?;
                let f: extern "C" fn(f64, f64, f64, f64) -> f64 = unsafe { std::mem::transmute(ptr_val) };
                Ok(Value::F64(f(a0, a1, a2, a3)))
            }))
        }

        _ => None,
    }
}
