# benchmarks/scripts/suite_08_impute/bench_impute.py
# Cross-Language Benchmark: Missing Data Imputation in Python (scikit-learn 1.9.1)

import os
import sys
import time
import numpy as np
import pandas as pd

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

from sklearn.experimental import enable_iterative_imputer
from sklearn.impute import IterativeImputer, KNNImputer

csv_path = "benchmarks/data/impute_benchmark_data.csv"
if not os.path.exists(csv_path):
    csv_path = "../../data/impute_benchmark_data.csv"

df = pd.read_csv(csv_path)
n_rows, n_cols = df.shape

# -----------------------------------------------------------------------------
# Task 1: Missing Pattern Aggregation
# -----------------------------------------------------------------------------
t0 = time.perf_counter()
n_missing_cells = int(df.isna().sum().sum())
pct_missing_cells = (n_missing_cells / (n_rows * n_cols)) * 100.0
# Count unique missingness masks
patterns = df.isna().drop_duplicates()
t_pattern_ms = (time.perf_counter() - t0) * 1000.0

print(f"TASK_PATTERN_TIME_MS: {t_pattern_ms:.2f}")
print(f"TASK_MISSING_CELLS: {n_missing_cells}")
print(f"TASK_MISSING_PCT: {pct_missing_cells:.2f}")

# -----------------------------------------------------------------------------
# Task 2: IterativeImputer (MICE / Chained Equations, max_iter=5)
# -----------------------------------------------------------------------------
t0 = time.perf_counter()
imputer = IterativeImputer(max_iter=5, random_state=42, sample_posterior=True)
arr_imp = imputer.fit_transform(df)
t_mice_ms = (time.perf_counter() - t0) * 1000.0

print(f"TASK_MICE_PMM_TIME_MS: {t_mice_ms:.2f}")

# Imputed column for cholesterol (index 3: age=0, bmi=1, hyp=2, chl=3, income=4)
chl_imp = arr_imp[:, 3]
print(f"TASK_PMM_IMP_MEAN: {np.mean(chl_imp):.4f}")
print(f"TASK_PMM_IMP_SD: {np.std(chl_imp, ddof=1):.4f}")

# -----------------------------------------------------------------------------
# Task 3: Rubin's Rules Pooling across M=5 Imputations
# -----------------------------------------------------------------------------
t0 = time.perf_counter()
m_datasets = []
for m in range(5):
    imp_m = IterativeImputer(max_iter=5, random_state=42 + m, sample_posterior=True)
    m_datasets.append(imp_m.fit_transform(df))

# Fit OLS: chl ~ 1 + age + bmi + hyp
estimates = []
variances = []
for d in m_datasets:
    # d[:, 0]=age, d[:, 1]=bmi, d[:, 2]=hyp, d[:, 3]=chl
    X = np.column_stack([np.ones(n_rows), d[:, 0], d[:, 1], d[:, 2]])
    y = d[:, 3]
    # OLS beta = (X'X)^-1 X'y
    xtx_inv = np.linalg.inv(X.T @ X)
    beta = xtx_inv @ X.T @ y
    res = y - X @ beta
    sigma2 = np.sum(res**2) / (n_rows - 4)
    cov_beta = sigma2 * xtx_inv
    # index 1 is age
    estimates.append(beta[1])
    variances.append(cov_beta[1, 1])

# Rubin's Rules
m_count = 5.0
qbar = np.mean(estimates)
ubar = np.mean(variances)
b = np.sum((np.array(estimates) - qbar)**2) / (m_count - 1.0)
t_var = ubar + (1.0 + 1.0 / m_count) * b
se = np.sqrt(t_var)
tstat = qbar / se

r_val = (1.0 + 1.0 / m_count) * b / ubar
lambda_val = (r_val + 2.0 / 499.0) / (r_val + 1.0)
nu_old = (m_count - 1.0) * (1.0 + 1.0 / r_val)**2
nu_com = n_rows - 4.0
nu_obs = ((nu_com + 1.0) / (nu_com + 3.0)) * nu_com * (1.0 - lambda_val)
nu_adj = (nu_old * nu_obs) / (nu_old + nu_obs)
t_pool_ms = (time.perf_counter() - t0) * 1000.0

print(f"TASK_POOL_TIME_MS: {t_pool_ms:.2f}")
print(f"TASK_RUBIN_QBAR: {qbar:.4f}")
print(f"TASK_RUBIN_SE: {se:.4f}")
print(f"TASK_RUBIN_TSTAT: {tstat:.4f}")
print(f"TASK_RUBIN_DF: {nu_adj:.4f}")
print(f"TASK_RUBIN_UBAR: {ubar:.6f}")
print(f"TASK_RUBIN_B: {b:.6f}")
print(f"TASK_RUBIN_T: {t_var:.6f}")
print(f"TASK_RUBIN_RIV: {r_val:.4f}")
print(f"TASK_RUBIN_FMI: {lambda_val:.4f}")

# -----------------------------------------------------------------------------
# Task 4: k-NN Imputation (k=5)
# -----------------------------------------------------------------------------
t0 = time.perf_counter()
knn_imp = KNNImputer(n_neighbors=5)
arr_knn = knn_imp.fit_transform(df)
t_knn_ms = (time.perf_counter() - t0) * 1000.0
print(f"TASK_KNN_TIME_MS: {t_knn_ms:.2f}")
