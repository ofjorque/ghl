import sys
import polars as pl

sys.stdout.reconfigure(encoding="utf-8")

df = pl.read_csv("target/synthetic_1m.csv")
summary = (
    df.filter(pl.col("status") == "OK")
    .group_by("category")
    .agg(
        pl.col("value_b").mean().alias("mean_b"),
        pl.col("value_c").mean().alias("mean_c"),
        pl.col("value_a").sum().alias("total_a"),
        pl.len().alias("n"),
    )
    .sort("category")
)
print(summary)
