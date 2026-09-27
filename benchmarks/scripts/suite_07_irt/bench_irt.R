# benchmarks/scripts/suite_07_irt/bench_irt.R
# Benchmark Suite: IRT Psychometrics in R (mirt 1.47)

suppressPackageStartupMessages({
    library(mirt)
})

# -----------------------------------------------------------------------------
# 1. LSAT7: 1PL (Rasch) Model
# -----------------------------------------------------------------------------
lsat7_raw <- as.matrix(read.csv("benchmarks/data/lsat7_raw.csv"))

t0 <- proc.time()
mod_1pl <- mirt(lsat7_raw, 1, itemtype = 'Rasch', verbose = FALSE, technical = list(NCYCLES = 50))
t_1pl <- (proc.time() - t0)[3]

c_1pl <- coef(mod_1pl, IRTpars = TRUE, simplify = TRUE)$items
b_1pl <- c_1pl[, "b"]
loglik_1pl <- extract.mirt(mod_1pl, "logLik")

# -----------------------------------------------------------------------------
# 2. LSAT7: 2PL Model
# -----------------------------------------------------------------------------
t0 <- proc.time()
mod_2pl <- mirt(lsat7_raw, 1, itemtype = '2PL', verbose = FALSE, technical = list(NCYCLES = 50))
t_2pl <- (proc.time() - t0)[3]

c_2pl <- coef(mod_2pl, IRTpars = TRUE, simplify = TRUE)$items
a_2pl <- c_2pl[, "a"]
b_2pl <- c_2pl[, "b"]
loglik_2pl <- extract.mirt(mod_2pl, "logLik")

# -----------------------------------------------------------------------------
# 3. Science: Graded Response Model (GRM)
# -----------------------------------------------------------------------------
science_raw <- as.matrix(read.csv("benchmarks/data/science.csv"))

t0 <- proc.time()
mod_grm <- mirt(science_raw, 1, itemtype = 'graded', verbose = FALSE, technical = list(NCYCLES = 50))
t_grm <- (proc.time() - t0)[3]

c_grm <- coef(mod_grm, IRTpars = TRUE, simplify = TRUE)$items
a_grm <- c_grm[, "a"]
b_grm <- c_grm[, c("b1", "b2", "b3")]
loglik_grm <- extract.mirt(mod_grm, "logLik")

# -----------------------------------------------------------------------------
# 4. High-Dimensional MIRT (MHRM, D = 6)
# -----------------------------------------------------------------------------
# 500 examinees x 12 items on 6 dimensions
set.seed(42)
N_mhrm <- 500
J_mhrm <- 12
D_mhrm <- 6
theta_true <- matrix(rnorm(N_mhrm * D_mhrm), N_mhrm, D_mhrm)
a_true <- matrix(runif(J_mhrm * D_mhrm, 0.5, 1.8), J_mhrm, D_mhrm)
d_true <- rnorm(J_mhrm, 0, 1)

prob_mat <- 1 / (1 + exp(-(theta_true %*% t(a_true) + matrix(d_true, N_mhrm, J_mhrm, byrow = TRUE))))
data_mhrm <- (matrix(runif(N_mhrm * J_mhrm), N_mhrm, J_mhrm) < prob_mat) * 1
colnames(data_mhrm) <- paste0("Item_", 1:J_mhrm)

t0 <- proc.time()
mod_mhrm <- mirt(data_mhrm, D_mhrm, method = 'MHRM', verbose = FALSE, technical = list(NCYCLES = 45))
t_mhrm <- (proc.time() - t0)[3]
loglik_mhrm <- extract.mirt(mod_mhrm, "logLik")

# -----------------------------------------------------------------------------
# 5. Multigroup LRT DIF on LSAT7
# -----------------------------------------------------------------------------
group <- rep(c("Ref", "Foc"), each = 500)
t0 <- proc.time()
mod_mg <- multipleGroup(lsat7_raw, 1, group = group, invariance = colnames(lsat7_raw), verbose = FALSE)
dif_res <- DIF(mod_mg, which.par = c('d'), verbose = FALSE)
t_dif <- (proc.time() - t0)[3]

# -----------------------------------------------------------------------------
# Output Results as Formatted Summary
# -----------------------------------------------------------------------------
cat("=== R (mirt) BENCHMARK RESULTS ===\n")
cat(sprintf("TASK_1PL_TIME_MS: %.2f\n", t_1pl * 1000))
cat(sprintf("TASK_1PL_B: %s\n", paste(round(b_1pl, 4), collapse = ", ")))
cat(sprintf("TASK_1PL_LOGLIK: %.2f\n", loglik_1pl))

cat(sprintf("TASK_2PL_TIME_MS: %.2f\n", t_2pl * 1000))
cat(sprintf("TASK_2PL_A: %s\n", paste(round(a_2pl, 4), collapse = ", ")))
cat(sprintf("TASK_2PL_B: %s\n", paste(round(b_2pl, 4), collapse = ", ")))
cat(sprintf("TASK_2PL_LOGLIK: %.2f\n", loglik_2pl))

cat(sprintf("TASK_GRM_TIME_MS: %.2f\n", t_grm * 1000))
cat(sprintf("TASK_GRM_A: %s\n", paste(round(a_grm, 4), collapse = ", ")))
cat(sprintf("TASK_GRM_LOGLIK: %.2f\n", loglik_grm))

cat(sprintf("TASK_MHRM_TIME_MS: %.2f\n", t_mhrm * 1000))
cat(sprintf("TASK_MHRM_LOGLIK: %.2f\n", loglik_mhrm))

cat(sprintf("TASK_DIF_TIME_MS: %.2f\n", t_dif * 1000))
cat(sprintf("TASK_DIF_CHISQ: %s\n", paste(round(dif_res$X2, 2), collapse = ", ")))
