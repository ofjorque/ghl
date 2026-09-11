import pandas as pd

df = pd.read_csv("target/synthetic_1m.csv")
filtered = df[df["status"] == "OK"]
summary = filtered.groupby("category").agg(
    mean_b=("value_b", "mean"),
    mean_c=("value_c", "mean"),
    total_a=("value_a", "sum"),
    n=("id", "count")
).reset_index()
print(summary)
