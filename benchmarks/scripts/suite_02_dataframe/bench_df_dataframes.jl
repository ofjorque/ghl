using CSV
using DataFrames
using Statistics

df = CSV.read("target/synthetic_1m.csv", DataFrame)
filtered = filter(:status => ==("OK"), df)
summary_df = combine(
    groupby(filtered, :category),
    :value_b => mean => :mean_b,
    :value_c => mean => :mean_c,
    :value_a => sum => :total_a,
    nrow => :n,
)
sort!(summary_df, :category)
println(summary_df)
