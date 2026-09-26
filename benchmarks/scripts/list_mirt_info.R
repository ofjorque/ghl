library(mirt)
cat("=== MIRT Version ===\n")
cat(as.character(packageVersion("mirt")), "\n\n")

cat("=== Datasets in mirt ===\n")
d <- data(package = "mirt")
for (i in 1:nrow(d$results)) {
  cat(sprintf("- %-15s : %s\n", d$results[i, "Item"], d$results[i, "Title"]))
}

cat("\n=== Exported Functions in mirt ===\n")
funcs <- ls("package:mirt")
cat("Total exported functions:", length(funcs), "\n")
cat(paste(head(funcs, 40), collapse = ", "), "\n...\n")
