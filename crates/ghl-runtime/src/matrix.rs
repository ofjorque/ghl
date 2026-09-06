use ghl_diagnostics::Diagnostic;

pub struct MatrixOps;

impl MatrixOps {
    /// Multiplies two matrices: C = A * B.
    /// Emits S0412 if inner dimensions do not match.
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

        let mut c = vec![0.0; a_rows * b_cols];

        for i in 0..a_rows {
            for k in 0..a_cols {
                let a_ik = a[i * a_cols + k];
                for j in 0..b_cols {
                    c[i * b_cols + j] += a_ik * b[k * b_cols + j];
                }
            }
        }

        Ok((a_rows, b_cols, c))
    }

    /// Solves linear system A * x = b for square matrix A via Gaussian elimination with partial pivoting.
    /// Emits S0101 if matrix A is singular (det ≈ 0).
    pub fn solve(
        n: usize,
        a: &[f64],
        b: &[f64],
    ) -> Result<Vec<f64>, Diagnostic> {
        if a.len() != n * n || b.len() != n {
            return Err(Diagnostic::statistical_error(
                "S0412",
                format!(
                    "Dimension mismatch in matrix solve `A \\ b`: A is ({}x{}), but b has {} elements",
                    n, n, b.len()
                ),
            ));
        }

        // Augmented matrix [A | b]
        let mut aug = vec![0.0; n * (n + 1)];
        for i in 0..n {
            for j in 0..n {
                aug[i * (n + 1) + j] = a[i * n + j];
            }
            aug[i * (n + 1) + n] = b[i];
        }

        // Forward elimination with partial pivoting
        for col in 0..n {
            // Find pivot
            let mut max_row = col;
            let mut max_val = aug[col * (n + 1) + col].abs();

            for row in (col + 1)..n {
                let val = aug[row * (n + 1) + col].abs();
                if val > max_val {
                    max_val = val;
                    max_row = row;
                }
            }

            // Check for singularity
            if max_val < 1e-12 {
                return Err(Diagnostic::statistical_error(
                    "S0101",
                    "Singular matrix detected in `A \\ b`: determinant is approximately zero, system has no unique solution",
                )
                .with_help("Verify that predictor variables are not collinear (VIF test) or regularize with ridge/Lasso."));
            }

            // Swap rows
            if max_row != col {
                for k in 0..=(n) {
                    aug.swap(col * (n + 1) + k, max_row * (n + 1) + k);
                }
            }

            // Eliminate lower rows
            for row in (col + 1)..n {
                let factor = aug[row * (n + 1) + col] / aug[col * (n + 1) + col];
                for k in col..=(n) {
                    aug[row * (n + 1) + k] -= factor * aug[col * (n + 1) + k];
                }
            }
        }

        // Back substitution
        let mut x = vec![0.0; n];
        for i in (0..n).rev() {
            let mut sum = aug[i * (n + 1) + n];
            for j in (i + 1)..n {
                sum -= aug[i * (n + 1) + j] * x[j];
            }
            x[i] = sum / aug[i * (n + 1) + i];
        }

        Ok(x)
    }

    /// Element-wise matrix operation
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
}
