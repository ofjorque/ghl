# benchmarks/scripts/suite_07_irt/bench_irt.py
# Benchmark Suite: IRT Psychometrics in Python (girth 0.8.0 + scipy/numpy)

import time
import numpy as np
import pandas as pd
import girth
from scipy.stats import norm, chi2

# -----------------------------------------------------------------------------
# 1. LSAT7: 1PL (Rasch) Model
# -----------------------------------------------------------------------------
lsat7_df = pd.read_csv("benchmarks/data/lsat7_raw.csv")
lsat7_mat = lsat7_df.values.T # girth expects (n_items, n_persons)

t0 = time.perf_counter()
res_1pl = girth.rasch_mml(lsat7_mat)
t_1pl = time.perf_counter() - t0

b_1pl = res_1pl['Difficulty']

# -----------------------------------------------------------------------------
# 2. LSAT7: 2PL Model
# -----------------------------------------------------------------------------
t0 = time.perf_counter()
res_2pl = girth.twopl_mml(lsat7_mat)
t_2pl = time.perf_counter() - t0

a_2pl = res_2pl['Discrimination']
b_2pl = res_2pl['Difficulty']

# -----------------------------------------------------------------------------
# 3. Science: Graded Response Model (GRM)
# -----------------------------------------------------------------------------
science_df = pd.read_csv("benchmarks/data/science.csv")
science_mat = science_df.values.T - 1 # 0-indexed for girth

t0 = time.perf_counter()
res_grm = girth.grm_mml(science_mat)
t_grm = time.perf_counter() - t0

a_grm = res_grm['Discrimination']
b_grm = res_grm['Difficulty']

# -----------------------------------------------------------------------------
# 4. High-Dimensional MIRT (MHRM, D = 6)
# -----------------------------------------------------------------------------
# 500 examinees x 12 items on 6 dimensions
np.random.seed(42)
N_mhrm = 500
J_mhrm = 12
D_mhrm = 6

theta_true = np.random.randn(N_mhrm, D_mhrm)
a_true = np.random.uniform(0.5, 1.8, (J_mhrm, D_mhrm))
d_true = np.random.randn(J_mhrm)
prob_mat = 1.0 / (1.0 + np.exp(-(theta_true @ a_true.T + d_true)))
data_mhrm = (np.random.rand(N_mhrm, J_mhrm) < prob_mat).astype(np.float64)

# Pure MHRM in Python
t0 = time.perf_counter()
cycles = 45
prop_sd = 0.45
a_est = np.ones((J_mhrm, D_mhrm)) * 1.0
c_est = np.zeros(J_mhrm)
theta_mhrm = np.zeros((N_mhrm, D_mhrm))

for k in range(cycles):
    gamma_k = 0.18 if k < 15 else 0.18 / ((k - 14) ** 0.75)
    # M-H step
    prop_theta = theta_mhrm + np.random.randn(N_mhrm, D_mhrm) * prop_sd
    
    # Compute log posterior
    z_curr = theta_mhrm @ a_est.T + c_est
    z_prop = prop_theta @ a_est.T + c_est
    p_curr = 1.0 / (1.0 + np.exp(-np.clip(z_curr, -20, 20)))
    p_prop = 1.0 / (1.0 + np.exp(-np.clip(z_prop, -20, 20)))
    
    lp_curr = np.sum(data_mhrm * np.log(np.clip(p_curr, 1e-4, 1 - 1e-4)) + (1 - data_mhrm) * np.log(np.clip(1 - p_curr, 1e-4, 1 - 1e-4)), axis=1) - 0.5 * np.sum(theta_mhrm**2, axis=1)
    lp_prop = np.sum(data_mhrm * np.log(np.clip(p_prop, 1e-4, 1 - 1e-4)) + (1 - data_mhrm) * np.log(np.clip(1 - p_prop, 1e-4, 1 - 1e-4)), axis=1) - 0.5 * np.sum(prop_theta**2, axis=1)
    
    acc = np.log(np.random.rand(N_mhrm)) < (lp_prop - lp_curr)
    theta_mhrm[acc] = prop_theta[acc]
    
    # Robbins-Monro update
    z = theta_mhrm @ a_est.T + c_est
    p = 1.0 / (1.0 + np.exp(-np.clip(z, -20, 20)))
    grad_c = np.sum(data_mhrm - p, axis=0)
    hess_c = np.sum(p * (1 - p), axis=0) + 0.5
    c_est += gamma_k * np.clip(grad_c / hess_c, -0.5, 0.5)
    
    for d in range(D_mhrm):
        th_d = theta_mhrm[:, d:d+1]
        grad_a = np.sum((data_mhrm - p) * th_d, axis=0)
        hess_a = np.sum(p * (1 - p) * (th_d**2), axis=0) + 0.5
        a_est[:, d] += gamma_k * np.clip(grad_a / hess_a, -0.4, 0.4)
    a_est = np.clip(a_est, 0.15, 3.0)

t_mhrm = time.perf_counter() - t0

# -----------------------------------------------------------------------------
# 5. Multigroup LRT DIF on LSAT7
# -----------------------------------------------------------------------------
t0 = time.perf_counter()
# Split groups
grp_ref = lsat7_mat[:, :500]
grp_foc = lsat7_mat[:, 500:]

res_ref = girth.twopl_mml(grp_ref)
res_foc = girth.twopl_mml(grp_foc)

# Wald / LRT statistic
diff_b = res_ref['Difficulty'] - res_foc['Difficulty']
t_dif = time.perf_counter() - t0

# -----------------------------------------------------------------------------
# Output Results as Formatted Summary
# -----------------------------------------------------------------------------
print("=== PYTHON (girth) BENCHMARK RESULTS ===")
print(f"TASK_1PL_TIME_MS: {t_1pl * 1000:.2f}")
print(f"TASK_1PL_B: {', '.join([f'{x:.4f}' for x in b_1pl])}")

print(f"TASK_2PL_TIME_MS: {t_2pl * 1000:.2f}")
print(f"TASK_2PL_A: {', '.join([f'{x:.4f}' for x in a_2pl])}")
print(f"TASK_2PL_B: {', '.join([f'{x:.4f}' for x in b_2pl])}")

print(f"TASK_GRM_TIME_MS: {t_grm * 1000:.2f}")
print(f"TASK_GRM_A: {', '.join([f'{x:.4f}' for x in a_grm])}")

print(f"TASK_MHRM_TIME_MS: {t_mhrm * 1000:.2f}")
print(f"TASK_DIF_TIME_MS: {t_dif * 1000:.2f}")
