run_gibbs <- function(y, groups, num_iterations) {
  num_groups <- max(groups) + 1
  trace <- matrix(0, nrow = num_iterations, ncol = num_groups)
  tau <- 1.0
  mu_vec <- numeric(num_groups)
  n_total <- length(y)

  for (iter in 1:num_iterations) {
    for (j in 0:(num_groups - 1)) {
      y_j <- y[groups == j]
      n_j <- length(y_j)
      post_mean <- (sum(y_j) * tau) / (n_j * tau + 1.0)
      post_sd <- 1.0 / sqrt(n_j * tau + 1.0)
      mu_vec[j + 1] <- rnorm(1, mean = post_mean, sd = post_sd)
    }

    diff <- y - mu_vec[groups + 1]
    ssq <- sum(diff * diff)
    alpha_post <- 1.0 + n_total / 2.0
    beta_post <- 1.0 + ssq / 2.0
    tau <- rgamma(1, shape = alpha_post, rate = beta_post)

    trace[iter, ] <- mu_vec
  }

  trace
}

df <- read.csv("benchmarks/scripts/suite_03_modeling/gibbs_data.csv")
y <- df$y
groups <- as.integer(df$group)
trace <- run_gibbs(y, groups, 100)
cat(mean(trace[, 1]), "\n")
cat(mean(trace[, 2]), "\n")
cat(mean(trace[, 3]), "\n")
