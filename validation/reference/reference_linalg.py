# Python / NumPy / SciPy Reference script for GHL Linear Algebra and Covariances
import numpy as np

# 1. SVD Reference Matrix
A = np.array([
    [1.0, 2.0, 3.0],
    [4.0, 5.0, 6.0],
    [7.0, 8.0, 10.0]
], dtype=np.float64)

U, S, Vt = np.linalg.svd(A)
print("=== SVD Singular Values ===")
print(S)

# 2. QR Reference
Q, R = np.linalg.qr(A)
print("=== QR R Matrix ===")
print(R)

# 3. Symmetric Eigenvalues Reference
Sym = np.array([
    [4.0, 1.0, -2.0],
    [1.0, 2.0, 0.0],
    [-2.0, 0.0, 3.0]
], dtype=np.float64)

eigvals, eigvecs = np.linalg.eigh(Sym)
print("=== Symmetric Eigenvalues ===")
print(eigvals)
