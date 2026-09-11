using LinearAlgebra
using Random

Random.seed!(42)
a = rand(10_000_000)
Random.seed!(43)
b = rand(10_000_000)
res = dot(a, b)
println(res)
