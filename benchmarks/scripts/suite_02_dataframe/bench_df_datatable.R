suppressPackageStartupMessages(library(data.table))

dt <- fread("target/synthetic_1m.csv", showProgress = FALSE)
summary_dt <- dt[status == "OK", .(
  mean_b = mean(value_b),
  mean_c = mean(value_c),
  total_a = sum(value_a),
  n = .N
), by = category][order(category)]
print(summary_dt)
