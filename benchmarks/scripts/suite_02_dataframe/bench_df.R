df <- read.csv("target/synthetic_1m.csv", stringsAsFactors = FALSE)
filtered <- df[df$status == "OK", ]
agg_mean_b <- aggregate(value_b ~ category, data = filtered, FUN = mean)
agg_mean_c <- aggregate(value_c ~ category, data = filtered, FUN = mean)
agg_sum_a  <- aggregate(value_a ~ category, data = filtered, FUN = sum)
agg_n      <- aggregate(id ~ category, data = filtered, FUN = length)
res <- cbind(agg_mean_b, mean_c = agg_mean_c$value_c, total_a = agg_sum_a$value_a, n = agg_n$id)
print(res)
