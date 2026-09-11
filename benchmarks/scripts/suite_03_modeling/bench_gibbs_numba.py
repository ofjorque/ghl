import numpy as np
import pandas as pd
from numba import njit


@njit(cache=True)
def run_gibbs(y, groups, num_iterations, num_groups):
    trace = np.zeros((num_iterations, num_groups))
    tau = 1.0
    mu_vec = np.zeros(num_groups)
    n_total = len(y)

    for it in range(num_iterations):
        for j in range(num_groups):
            mask = groups == j
            y_j = y[mask]
            n_j = len(y_j)
            post_mean = (np.sum(y_j) * tau) / (n_j * tau + 1.0)
            post_sd = 1.0 / np.sqrt(n_j * tau + 1.0)
            mu_vec[j] = np.random.normal(post_mean, post_sd)

        diff = y - mu_vec[groups]
        ssq = np.dot(diff, diff)
        alpha_post = 1.0 + n_total / 2.0
        beta_post = 1.0 + ssq / 2.0
        tau = np.random.gamma(alpha_post, 1.0 / beta_post)

        trace[it, :] = mu_vec

    return trace


df = pd.read_csv("benchmarks/scripts/suite_03_modeling/gibbs_data.csv")
y = df["y"].values.astype(np.float64)
groups = df["group"].values.astype(np.int64)
num_groups = int(np.max(groups) + 1)

# Warm the JIT outside the timed region would defeat a cold-start TTFX test,
# so this variant intentionally pays JIT compilation cost inside the run,
# same as every other suite here (whole-process wall time).
trace = run_gibbs(y, groups, 100, num_groups)
print(np.mean(trace[:, 0]))
print(np.mean(trace[:, 1]))
print(np.mean(trace[:, 2]))
