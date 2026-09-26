library(mirt)
funcs <- ls("package:mirt")
res <- data.frame(Function = character(), Title = character(), stringsAsFactors = FALSE)

for (f in funcs) {
  h <- tryCatch({
    rd <- utils:::.getHelpFile(help(f, package = "mirt"))
    tools:::Rd_get_metadata(rd, "title")
  }, error = function(e) "")
  if (length(h) == 0) h <- ""
  res <- rbind(res, data.frame(Function = f, Title = h, stringsAsFactors = FALSE))
}

write.csv(res, "benchmarks/scripts/mirt_functions_catalog.csv", row.names = FALSE)
cat("Exported mirt_functions_catalog.csv with", nrow(res), "functions.\n")
