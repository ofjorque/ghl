# benchmarks/scripts/suite_08_impute/bench_impute.R
# Cross-Language Benchmark: Missing Data Imputation in R (mice 3.19.0)

suppressPackageStartupMessages({
    library(mice)
})

csv_path <- "benchmarks/data/impute_benchmark_data.csv"
if (!file.exists(csv_path)) {
    csv_path <- "../../data/impute_benchmark_data.csv"
}

df <- read.csv(csv_path)
n_rows <- nrow(df)
n_cols <- ncol(df)

# -----------------------------------------------------------------------------
# Task 1: Missing Pattern Aggregation (md.pattern)
# -----------------------------------------------------------------------------
t0 <- proc.time()
pat <- md.pattern(df, plot = FALSE)
n_missing_cells <- sum(is.na(df))
pct_missing_cells <- (n_missing_cells / (n_rows * n_cols)) * 100.0
t_pattern_ms <- (proc.time() - t0)[3] * 1000.0

cat(sprintf("TASK_PATTERN_TIME_MS: %.2f\n", t_pattern_ms))
cat(sprintf("TASK_MISSING_CELLS: %d\n", n_missing_cells))
cat(sprintf("TASK_MISSING_PCT: %.2f\n", pct_missing_cells))

# -----------------------------------------------------------------------------
# Task 2: MICE with Predictive Mean Matching (PMM, m=5, maxit=5)
# -----------------------------------------------------------------------------
t0 <- proc.time()
imp_mice <- mice(df, m = 5, maxit = 5, method = "pmm", seed = 42, printFlag = FALSE)
t_mice_ms <- (proc.time() - t0)[3] * 1000.0

cat(sprintf("TASK_MICE_PMM_TIME_MS: %.2f\n", t_mice_ms))

# Imputed mean and sd of cholesterol in complete dataset 1
df_imp1 <- complete(imp_mice, 1)
mean_chl_imp1 <- mean(df_imp1$chl)
sd_chl_imp1 <- sd(df_imp1$chl)
cat(sprintf("TASK_PMM_IMP_MEAN: %.4f\n", mean_chl_imp1))
cat(sprintf("TASK_PMM_IMP_SD: %.4f\n", sd_chl_imp1))

# -----------------------------------------------------------------------------
# Task 3: Rubin's Rules Pooling with Barnard-Rubin (1999) df
# -----------------------------------------------------------------------------
t0 <- proc.time()
fit_models <- with(imp_mice, lm(chl ~ age + bmi + hyp))
pooled <- pool(fit_models)
t_pool_ms <- (proc.time() - t0)[3] * 1000.0

pooled_summary <- summary(pooled)
# Extract slope for age
idx_age <- which(pooled_summary$term == "age")
qbar_age <- pooled_summary$estimate[idx_age]
se_age <- pooled_summary$std.error[idx_age]
tstat_age <- pooled_summary$statistic[idx_age]
df_age <- pooled_summary$df[idx_age]
riv_age <- pooled$pooled$riv[idx_age]
fmi_age <- pooled$pooled$fmi[idx_age]
b_age <- pooled$pooled$b[idx_age]
ubar_age <- pooled$pooled$ubar[idx_age]
t_age <- pooled$pooled$t[idx_age]

cat(sprintf("TASK_POOL_TIME_MS: %.2f\n", t_pool_ms))
cat(sprintf("TASK_RUBIN_QBAR: %.4f\n", qbar_age))
cat(sprintf("TASK_RUBIN_SE: %.4f\n", se_age))
cat(sprintf("TASK_RUBIN_TSTAT: %.4f\n", tstat_age))
cat(sprintf("TASK_RUBIN_DF: %.4f\n", df_age))
cat(sprintf("TASK_RUBIN_UBAR: %.6f\n", ubar_age))
cat(sprintf("TASK_RUBIN_B: %.6f\n", b_age))
cat(sprintf("TASK_RUBIN_T: %.6f\n", t_age))
cat(sprintf("TASK_RUBIN_RIV: %.4f\n", riv_age))
cat(sprintf("TASK_RUBIN_FMI: %.4f\n", fmi_age))

# -----------------------------------------------------------------------------
# Task 4: Hotdeck / Donor Sampling Imputation
# -----------------------------------------------------------------------------
t0 <- proc.time()
# R base sample hotdeck
df_hot <- df
for (col in names(df_hot)) {
    na_idx <- which(is.na(df_hot[[col]]))
    if (length(na_idx) > 0) {
        obs_vals <- df_hot[[col]][-na_idx]
        df_hot[[col]][na_idx] <- sample(obs_vals, length(na_idx), replace = TRUE)
    }
}
t_hot_ms <- (proc.time() - t0)[3] * 1000.0
cat(sprintf("TASK_HOTDECK_TIME_MS: %.2f\n", t_hot_ms))
