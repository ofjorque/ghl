library(mirt)
dir.create("benchmarks/data", showWarnings = FALSE, recursive = TRUE)

# 1. LSAT7 (1000 examinees x 5 dichotomous items)
data(LSAT7)
write.csv(LSAT7, "benchmarks/data/lsat7.csv", row.names = FALSE)
cat("Exported lsat7.csv:", nrow(LSAT7), "x", ncol(LSAT7), "\n")

# 2. LSAT6 (1000 examinees x 5 dichotomous items)
data(LSAT6)
write.csv(LSAT6, "benchmarks/data/lsat6.csv", row.names = FALSE)
cat("Exported lsat6.csv:", nrow(LSAT6), "x", ncol(LSAT6), "\n")

# 3. Science (392 examinees x 4 polytomous items)
data(Science)
write.csv(Science, "benchmarks/data/science.csv", row.names = FALSE)
cat("Exported science.csv:", nrow(Science), "x", ncol(Science), "\n")

# 4. Bock1997 (Nominal data)
data(Bock1997)
write.csv(Bock1997, "benchmarks/data/bock1997.csv", row.names = FALSE)
cat("Exported bock1997.csv:", nrow(Bock1997), "x", ncol(Bock1997), "\n")

# 5. deAyala (Polytomous partial credit)
data(deAyala)
write.csv(deAyala, "benchmarks/data/deayala.csv", row.names = FALSE)
cat("Exported deayala.csv:", nrow(deAyala), "x", ncol(deAyala), "\n")
