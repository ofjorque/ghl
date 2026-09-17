//! Regional memory arena native functions (RFC 03 sect2.2).
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use ghl_diagnostics::Diagnostic;
use crate::eval::Interpreter;
use crate::value::Value;

// =========================================================================
// Regional Memory Arenas (RFC 03 §2.2)
// =========================================================================

pub(crate) fn native_arena_scope(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    let callable = args.first().cloned().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`arena::scope()` requires a callback function `\\a -> ...`")
    })?;
    let arena_state = std::sync::Arc::new(std::sync::Mutex::new(crate::arena::ArenaState::new()));
    let arena_val = Value::Arena(arena_state.clone());
    let res = interp.call_value(callable, vec![arena_val]);
    if let Ok(mut st) = arena_state.lock() {
        st.reset();
    }
    res
}

pub(crate) fn native_alloc_vector(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error("C0201", "`alloc_vector(arena, len, [default])` requires at least an arena and length"));
    }
    let arena_val = &args[0];
    let len = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`alloc_vector(arena, len)` requires an integer length")
    })?;
    if len < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`alloc_vector()` length must be non-negative, found {len}")));
    }
    let default_val = args.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
    match arena_val {
        Value::Arena(st_arc) => {
            let mut st = st_arc.lock().map_err(|e| Diagnostic::compute_error("C0210", format!("Arena lock error: {e}")))?;
            Ok(st.alloc_vector(len as usize, default_val))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`alloc_vector()` requires an Arena as first argument, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_alloc_matrix(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error("C0201", "`alloc_matrix(arena, rows, cols, [default])` requires arena, rows, and cols"));
    }
    let arena_val = &args[0];
    let r = args[1].as_i64().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`alloc_matrix()` requires integer row count")
    })?;
    let c = args[2].as_i64().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`alloc_matrix()` requires integer col count")
    })?;
    if r < 0 || c < 0 {
        return Err(Diagnostic::compute_error("C0201", format!("`alloc_matrix()` dimensions must be non-negative, found ({r}, {c})")));
    }
    let default_val = args.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
    match arena_val {
        Value::Arena(st_arc) => {
            let mut st = st_arc.lock().map_err(|e| Diagnostic::compute_error("C0210", format!("Arena lock error: {e}")))?;
            Ok(st.alloc_matrix(r as usize, c as usize, default_val))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`alloc_matrix()` requires an Arena as first argument, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_arena_reset(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let arena_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`reset(arena)` requires an Arena argument")
    })?;
    match arena_val {
        Value::Arena(st_arc) => {
            let mut st = st_arc.lock().map_err(|e| Diagnostic::compute_error("C0210", format!("Arena lock error: {e}")))?;
            st.reset();
            Ok(Value::Unit)
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`reset()` requires an Arena, found `{}`", other.type_name()))),
    }
}

pub(crate) fn native_arena_allocated_bytes(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let arena_val = args.first().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`allocated_bytes(arena)` requires an Arena argument")
    })?;
    match arena_val {
        Value::Arena(st_arc) => {
            let st = st_arc.lock().map_err(|e| Diagnostic::compute_error("C0210", format!("Arena lock error: {e}")))?;
            Ok(Value::I64(st.allocated_bytes() as i64))
        }
        other => Err(Diagnostic::compute_error("C0202", format!("`allocated_bytes()` requires an Arena, found `{}`", other.type_name()))),
    }
}

