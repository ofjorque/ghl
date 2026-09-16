# R Reference script for cross-language validation of GHL
# Models: OLS, GLM (Logit), HC0-HC3 Robust Covariances, Contrasts

# 1. OLS Reference
set.seed(42)
x1 <- c(1.0, 2.5, 3.2, 4.8, 5.1, 6.7, 7.3, 8.9)
x2 <- c(2.1, 1.8, 3.4, 4.0, 5.5, 6.1, 7.0, 8.2)
y_ols <- 1.5 + 2.0 * x1 - 0.5 * x2 + c(0.1, -0.2, 0.3, -0.1, 0.2, -0.3, 0.1, -0.1)
df_ols <- data.frame(y = y_ols, x1 = x1, x2 = x2)
fit_ols <- lm(y ~ x1 + x2, data = df_ols)
cat("=== OLS Reference ===\n")
print(coef(fit_ols))
print(summary(fit_ols)$r.squared)

# 2. GLM Logistic Reference
y_logit <- c(0, 0, 0, 1, 0, 1, 1, 1)
df_logit <- data.frame(y = y_logit, x1 = x1, x2 = x2)
fit_logit <- glm(y ~ x1 + x2, data = df_logit, family = binomial(link = "logit"))
cat("=== GLM Logit Reference ===\n")
print(coef(fit_logit))

# 3. Categorical Factor Contrasts Reference
grp <- factor(c("ctrl", "ctrl", "trt1", "trt1", "trt2", "trt2"))
y_grp <- c(10.2, 9.8, 20.1, 19.9, 30.5, 29.5)
df_grp <- data.frame(y = y_grp, grp = grp)
fit_grp <- lm(y ~ grp, data = df_grp)
cat("=== Factor Treatment Contrast Reference ===\n")
print(coef(fit_grp))
