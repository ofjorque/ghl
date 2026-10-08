//! High-performance runtime engine for GHL (Generalized Hypothesis Language).
//!
//! Provides memory evaluation, Kleene three-valued logic, linear algebra operators,
//! and native statistical functions.

pub mod env;
pub mod matrix;
pub mod value;
// `env`'s native function implementations, split out for maintainability -
// `RuntimeEnv::with_prelude()` in env.rs still refers to every `native_*` name
// unqualified, resolved via glob imports (env.rs and each of these modules
// import each other, which is why this file list has to precede `env`'s own
// re-exports being usable - Rust resolves this fine since it's all one crate).
pub mod arena;
pub mod autodiff;
pub mod cockpit;
pub mod concurrency;
pub mod doc;
pub mod eval;
pub mod feols;
pub mod glm;
pub mod gmm;
pub mod gpu;
pub mod io;
pub mod iv;
pub mod modules;
pub mod na_reasons;
pub mod native_arena_ops;
pub mod native_core;
pub mod native_dataframe_agg;
pub mod native_dataframe_ext;
pub mod native_db;
pub mod native_distributions;
pub mod native_fused;
pub mod native_io_ops;
pub mod native_irt;
pub mod native_linalg;
pub mod native_math;
pub mod native_neko_ops;
pub mod native_plot;
pub mod native_string_ops;
pub mod native_vector_ops;
pub mod neko;
pub mod net;
pub mod optim;
pub mod plot_stats;
pub mod polars_bridge;
pub mod regularized;
pub mod sem;
pub mod vector_data;

pub use doc::{FunctionDoc, lookup_doc};
pub use env::RuntimeEnv;
pub use eval::Interpreter;
pub use feols::{FeolsVcovSpec, fit_feols};
pub use glm::FittedGlm;
pub use gmm::FittedGmm;
pub use iv::{IvVcovSpec, fit_iv};
pub use neko::{Blueprint, FittedModel, RowDisposition, VcovKind};
pub use regularized::{RegularizationKind, RegularizedOptions, fit_regularized};
pub use value::{FormulaParts, JitFunction, Value};

use ghl_diagnostics::Diagnostic;
use ghl_syntax::ast::Program;

/// Evaluates a GHL program and returns the final expression value.
pub fn eval(program: &Program) -> Result<Value, Diagnostic> {
    let mut interpreter = Interpreter::new();
    interpreter.eval_program(program)
}
