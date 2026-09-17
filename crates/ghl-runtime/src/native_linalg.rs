//! Dense linear algebra native functions (faer-backed).
//!
//! Split out of `env.rs` for maintainability; native fn names are still
//! referenced unqualified from `RuntimeEnv::with_prelude()` via glob imports.

use ghl_diagnostics::Diagnostic;
use crate::value::Value;
use crate::vector_data::VectorData;


// =========================================================================
// Dense linear algebra (TODO.md Fase 3, faer-backed)
// =========================================================================

/// `qr(m)` — thin QR decomposition: `m` (rows×cols) = Q (rows×cols) × R (cols×cols).
pub(crate) fn native_qr(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Matrix { rows, cols, data }) => {
            let (q_data, r_data) = crate::matrix::MatrixOps::qr(*rows, *cols, data)?;
            Ok(Value::QrDecomp {
                q: Box::new(Value::Matrix { rows: *rows, cols: *cols, data: std::sync::Arc::new(q_data) }),
                r: Box::new(Value::Matrix { rows: *cols, cols: *cols, data: std::sync::Arc::new(r_data) }),
            })
        }
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`qr()` requires a Matrix, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`qr()` requires a Matrix")),
    }
}

pub(crate) fn native_qr_q(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::QrDecomp { q, .. }) => Ok((**q).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`qr_q()` requires a QrDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`qr_q()` requires a QrDecomp")),
    }
}

pub(crate) fn native_qr_r(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::QrDecomp { r, .. }) => Ok((**r).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`qr_r()` requires a QrDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`qr_r()` requires a QrDecomp")),
    }
}

/// `cholesky(m)` — L such that `m` = L × Lᵀ. Requires a symmetric positive-definite
/// square matrix (checked in `MatrixOps::cholesky`, not silently assumed).
pub(crate) fn native_cholesky(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Matrix { rows, cols, data }) => {
            if rows != cols {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("`cholesky()` requires a square matrix, found ({rows}x{cols})"),
                ));
            }
            let l_data = crate::matrix::MatrixOps::cholesky(*rows, data)?;
            Ok(Value::Matrix { rows: *rows, cols: *cols, data: std::sync::Arc::new(l_data) })
        }
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`cholesky()` requires a Matrix, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`cholesky()` requires a Matrix")),
    }
}

/// `svd(m)` — thin SVD: `m` (rows×cols) = U (rows×k) × diag(S) × Vᵀ (k×cols), k = min(rows,cols).
pub(crate) fn native_svd(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Matrix { rows, cols, data }) => {
            let (u_data, s_values, v_data, k) = crate::matrix::MatrixOps::svd(*rows, *cols, data)?;
            Ok(Value::SvdDecomp {
                u: Box::new(Value::Matrix { rows: *rows, cols: k, data: std::sync::Arc::new(u_data) }),
                s: Box::new(Value::Vector(VectorData::from_f64(s_values))),
                v: Box::new(Value::Matrix { rows: *cols, cols: k, data: std::sync::Arc::new(v_data) }),
            })
        }
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`svd()` requires a Matrix, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`svd()` requires a Matrix")),
    }
}

pub(crate) fn native_svd_u(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::SvdDecomp { u, .. }) => Ok((**u).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`svd_u()` requires an SvdDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`svd_u()` requires an SvdDecomp")),
    }
}

pub(crate) fn native_svd_s(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::SvdDecomp { s, .. }) => Ok((**s).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`svd_s()` requires an SvdDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`svd_s()` requires an SvdDecomp")),
    }
}

pub(crate) fn native_svd_v(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::SvdDecomp { v, .. }) => Ok((**v).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`svd_v()` requires an SvdDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`svd_v()` requires an SvdDecomp")),
    }
}

/// `eigen(m)` — eigendecomposition of a symmetric matrix (see `MatrixOps::eigen_symmetric`
/// for why non-symmetric input is rejected rather than silently misread).
pub(crate) fn native_eigen(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::Matrix { rows, cols, data }) => {
            if rows != cols {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("`eigen()` requires a square matrix, found ({rows}x{cols})"),
                ));
            }
            let (values, vectors_data) = crate::matrix::MatrixOps::eigen_symmetric(*rows, data)?;
            Ok(Value::EigenDecomp {
                values: Box::new(Value::Vector(VectorData::from_f64(values))),
                vectors: Box::new(Value::Matrix { rows: *rows, cols: *cols, data: std::sync::Arc::new(vectors_data) }),
            })
        }
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`eigen()` requires a Matrix, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`eigen()` requires a Matrix")),
    }
}

pub(crate) fn native_eigen_values(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::EigenDecomp { values, .. }) => Ok((**values).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`eigen_values()` requires an EigenDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`eigen_values()` requires an EigenDecomp")),
    }
}

pub(crate) fn native_eigen_vectors(args: Vec<Value>) -> Result<Value, Diagnostic> {
    match args.first() {
        Some(Value::EigenDecomp { vectors, .. }) => Ok((**vectors).clone()),
        Some(other) => Err(Diagnostic::statistical_error("S0200", format!("`eigen_vectors()` requires an EigenDecomp, found `{}`", other.type_name()))),
        None => Err(Diagnostic::compute_error("C0201", "`eigen_vectors()` requires an EigenDecomp")),
    }
}
