//! High-level and Mid-level Intermediate Representation for GHL.

pub mod hir;
pub mod lower;

pub use hir::*;
pub use lower::{LoweringContext, lower_ast};
