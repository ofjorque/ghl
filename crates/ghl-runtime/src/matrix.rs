//! Dense linear algebra backed by `faer` (TODO.md, Fase 3) instead of hand-rolled loops.
//!
//! `faer::Mat<f64>` is the conversion boundary: GHL's own `Value::Matrix` stores a flat
//! row-major `Vec<f64>` (see `value.rs`), so every entry point here converts in via
//! `Mat::from_fn` and back out via indexing -- the same "convert at the edges, use the
//! real library's types for the actual work" pattern the DataFrame/polars migration
//! (Fase 0/1) already established. `solve()`'s LU-based path and `mul()`'s `*` operator
//! both use faer's own multithreaded, blocked kernels (the `gemm`/`gemm-f64` crates
//! already pulled in transitively) -- Caso 1.2 ("multiplicación matricial densa
//! multinúcleo por bloques") falls out of this for free, it isn't hand-implemented here.

use faer::prelude::*;
use faer::Mat;
use ghl_diagnostics::Diagnostic;

pub struct MatrixOps;

/// Builds a `faer::Mat<f64>` from GHL's flat row-major `Vec<f64>` representation.
fn to_faer_mat(rows: usize, cols: usize, data: &[f64]) -> Mat<f64> {
    Mat::from_fn(rows, cols, |i, j| data[i * cols + j])
}

/// The inverse of `to_faer_mat`: flattens a `faer::MatRef<f64>` back to row-major.
fn from_faer_mat(m: MatRef<'_, f64>) -> Vec<f64> {
    let (rows, cols) = (m.nrows(), m.ncols());
    let mut out = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            out.push(m[(i, j)]);
        }
    }
    out
}

impl MatrixOps {
    /// Multiplies two matrices: C = A * B. Emits S0412 if inner dimensions do not match.
    /// Delegates to faer's `Mat * Mat` operator, which dispatches to its own blocked,
    /// multithreaded GEMM kernel rather than a naive triple loop.
    pub fn mul(
        a_rows: usize,
        a_cols: usize,
        a: &[f64],
        b_rows: usize,
        b_cols: usize,
        b: &[f64],
    ) -> Result<(usize, usize, Vec<f64>), Diagnostic> {
        if a_cols != b_rows {
            return Err(Diagnostic::statistical_error(
                "S0412",
                format!(
                    "Non-conformable matrix multiplication: ({}x{}) * ({}x{})",
                    a_rows, a_cols, b_rows, b_cols
                ),
            )
            .with_help("Inner matrix dimensions must agree: cols(A) == rows(B)."));
        }

        let ma = to_faer_mat(a_rows, a_cols, a);
        let mb = to_faer_mat(b_rows, b_cols, b);
        let mc = &ma * &mb;
        Ok((a_rows, b_cols, from_faer_mat(mc.as_ref())))
    }

    /// Solves linear system A * x = b for square matrix A via faer's partial-pivoting LU
    /// decomposition. Emits S0101 if matrix A is singular (a near-zero pivot on U's
    /// diagonal after decomposition -- the same signal the old hand-rolled Gaussian
    /// elimination checked during elimination, just read off the finished factorization
    /// instead of during it).
    pub fn solve(n: usize, a: &[f64], b: &[f64]) -> Result<Vec<f64>, Diagnostic> {
        if a.len() != n * n || b.len() != n {
            return Err(Diagnostic::statistical_error(
                "S0412",
                format!(
                    "Dimension mismatch in matrix solve `A \\ b`: A is ({}x{}), but b has {} elements",
                    n, n, b.len()
                ),
            ));
        }

        let ma = to_faer_mat(n, n, a);
        let lu = ma.partial_piv_lu();

        let u = lu.U();
        let min_pivot = (0..n).map(|i| u[(i, i)].abs()).fold(f64::INFINITY, f64::min);
        if min_pivot < 1e-12 {
            return Err(Diagnostic::statistical_error(
                "S0101",
                "Singular matrix detected in `A \\ b`: determinant is approximately zero, system has no unique solution",
            )
            .with_help("Verify that predictor variables are not collinear (VIF test) or regularize with ridge/Lasso."));
        }

        let mb = to_faer_mat(n, 1, b);
        let x = lu.solve(&mb);
        Ok((0..n).map(|i| x[(i, 0)]).collect())
    }

    /// Element-wise matrix operation. Already a tight, LLVM-auto-vectorizable loop over a
    /// flat `Vec<f64>` -- not routed through faer, which wouldn't buy anything here (no
    /// decomposition or blocked kernel involved, just `a[i] op b[i]` for every cell).
    pub fn elementwise(
        rows1: usize,
        cols1: usize,
        a: &[f64],
        rows2: usize,
        cols2: usize,
        b: &[f64],
        op: fn(f64, f64) -> f64,
        op_name: &str,
    ) -> Result<(usize, usize, Vec<f64>), Diagnostic> {
        if rows1 != rows2 || cols1 != cols2 {
            return Err(Diagnostic::statistical_error(
                "S0412",
                format!(
                    "Dimension mismatch in element-wise `{}`: ({}x{}) vs ({}x{})",
                    op_name, rows1, cols1, rows2, cols2
                ),
            ));
        }

        let mut res = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            res.push(op(a[i], b[i]));
        }
        Ok((rows1, cols1, res))
    }

    /// Thin QR decomposition: A (m×n, m ≥ n) = Q (m×n, orthonormal columns) × R (n×n,
    /// upper triangular). Returns `(q_data, r_data)`, both flattened row-major like every
    /// other `Value::Matrix`.
    pub fn qr(rows: usize, cols: usize, a: &[f64]) -> Result<(Vec<f64>, Vec<f64>), Diagnostic> {
        if a.len() != rows * cols {
            return Err(Diagnostic::statistical_error(
                "S0412",
                format!("`qr()`: matrix is ({rows}x{cols}) but data has {} entries", a.len()),
            ));
        }
        let ma = to_faer_mat(rows, cols, a);
        let qr = ma.qr();
        let q = qr.compute_thin_Q();
        let r = qr.thin_R();
        Ok((from_faer_mat(q.as_ref()), from_faer_mat(r)))
    }

    /// Cholesky decomposition of a symmetric positive-definite matrix: A = L Lᵀ. Returns
    /// just `L` (flattened row-major) -- `Lᵀ` is trivially `L`'s transpose, not worth a
    /// second return value. Emits S0412 if `A` isn't symmetric (checked explicitly: faer's
    /// `llt()` only ever reads one triangle and assumes the other mirrors it, so an
    /// asymmetric input would otherwise silently produce a factorization of a DIFFERENT,
    /// silently-symmetrized matrix instead of failing) and S0101 if `A` isn't positive
    /// definite (a real, meaningful failure mode for Cholesky, not a bug to hide).
    pub fn cholesky(n: usize, a: &[f64]) -> Result<Vec<f64>, Diagnostic> {
        if a.len() != n * n {
            return Err(Diagnostic::statistical_error(
                "S0412",
                format!("`cholesky()`: matrix is ({n}x{n}) but data has {} entries", a.len()),
            ));
        }
        check_symmetric(n, a, "cholesky()")?;

        let ma = to_faer_mat(n, n, a);
        let llt = ma.llt(faer::Side::Lower).map_err(|_| {
            Diagnostic::statistical_error(
                "S0101",
                "`cholesky()`: matrix is not positive-definite",
            )
            .with_help("Cholesky requires a symmetric positive-definite matrix -- check for collinear columns or a covariance matrix built from too few observations.")
        })?;
        Ok(from_faer_mat(llt.L()))
    }

    /// Thin SVD: A (m×n) = U (m×k) × diag(S) (k×k) × Vᵀ (k×n), k = min(m,n). Returns
    /// `(u_data, s_values, v_data, k)` -- `s_values` as a plain `Vec<f64>` (the singular
    /// values, nonincreasing) rather than a diagonal matrix, since a `Vector` is what
    /// GHL's own `Value` model already has for "one value per component".
    pub fn svd(rows: usize, cols: usize, a: &[f64]) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>, usize), Diagnostic> {
        if a.len() != rows * cols {
            return Err(Diagnostic::statistical_error(
                "S0412",
                format!("`svd()`: matrix is ({rows}x{cols}) but data has {} entries", a.len()),
            ));
        }
        let ma = to_faer_mat(rows, cols, a);
        let svd = ma.thin_svd().map_err(|e| {
            Diagnostic::compute_error("C0210", format!("`svd()` failed to converge: {e:?}"))
        })?;
        let k = rows.min(cols);
        let s_values: Vec<f64> = svd.S().column_vector().iter().copied().collect();
        Ok((from_faer_mat(svd.U()), s_values, from_faer_mat(svd.V()), k))
    }

    /// Eigendecomposition of a symmetric matrix: A = V × diag(values) × Vᵀ. Returns
    /// `(values, vectors_data)`, eigenvalues in nondecreasing order, eigenvectors as
    /// columns of the returned n×n matrix. Only defined for symmetric input (faer's
    /// `self_adjoint_eigen` assumes it and only reads one triangle without checking) --
    /// checked explicitly for the same "would otherwise silently decompose a different
    /// matrix" reason as `cholesky()`. A general (non-symmetric) eigendecomposition would
    /// need complex eigenvalues, which GHL's `Value` has no representation for yet.
    pub fn eigen_symmetric(n: usize, a: &[f64]) -> Result<(Vec<f64>, Vec<f64>), Diagnostic> {
        if a.len() != n * n {
            return Err(Diagnostic::statistical_error(
                "S0412",
                format!("`eigen()`: matrix is ({n}x{n}) but data has {} entries", a.len()),
            ));
        }
        check_symmetric(n, a, "eigen()")?;

        let ma = to_faer_mat(n, n, a);
        let eig = ma.self_adjoint_eigen(faer::Side::Lower).map_err(|e| {
            Diagnostic::compute_error("C0210", format!("`eigen()` failed to converge: {e:?}"))
        })?;
        let values: Vec<f64> = eig.S().column_vector().iter().copied().collect();
        Ok((values, from_faer_mat(eig.U())))
    }
}

/// Shared symmetry check for `cholesky()`/`eigen()` -- both require a symmetric input and
/// both back onto a faer routine that silently reads only one triangle instead of
/// validating the other, so this is where that validation actually happens.
fn check_symmetric(n: usize, a: &[f64], ctx: &str) -> Result<(), Diagnostic> {
    for i in 0..n {
        for j in (i + 1)..n {
            let (aij, aji) = (a[i * n + j], a[j * n + i]);
            if (aij - aji).abs() > 1e-9 * (1.0 + aij.abs().max(aji.abs())) {
                return Err(Diagnostic::statistical_error(
                    "S0412",
                    format!("`{ctx}` requires a symmetric matrix, but ({i},{j})={aij} and ({j},{i})={aji} differ"),
                ));
            }
        }
    }
    Ok(())
}
