using LinearAlgebra
using Random

function rand_gamma(shape::Float64, scale::Float64)::Float64
    if shape < 1.0
        return rand_gamma(shape + 1.0, scale) * (rand()^(1.0 / shape))
    end
    d = shape - 1.0 / 3.0
    c = 1.0 / sqrt(9.0 * d)
    while true
        z = randn()
        v = 1.0 + c * z
        if v <= 0.0
            continue
        end
        v = v * v * v
        u = rand()
        if u < 1.0 - 0.0331 * z * z * z * z
            return d * v * scale
        end
        if log(u) < 0.5 * z * z + d * (1.0 - v + log(v))
            return d * v * scale
        end
    end
end

function run_gibbs(y::Vector{Float64}, groups::Vector{Int64}, num_iterations::Int64)::Matrix{Float64}
    num_groups = maximum(groups) + 1
    trace = zeros(Float64, num_iterations, num_groups)
    tau = 1.0
    mu_vec = zeros(Float64, num_groups)
    n_total = length(y)

    @inbounds for iter in 1:num_iterations
        for j in 0:(num_groups - 1)
            mask = groups .== j
            y_j = y[mask]
            n_j = length(y_j)
            post_mean = (sum(y_j) * tau) / (n_j * tau + 1.0)
            post_sd = 1.0 / sqrt(n_j * tau + 1.0)
            mu_vec[j + 1] = post_mean + post_sd * randn()
        end

        diff = y .- mu_vec[groups .+ 1]
        ssq = dot(diff, diff)
        alpha_post = 1.0 + n_total / 2.0
        beta_post = 1.0 + ssq / 2.0
        tau = rand_gamma(alpha_post, 1.0 / beta_post)

        trace[iter, :] = mu_vec
    end

    trace
end

lines = readlines("benchmarks/scripts/suite_03_modeling/gibbs_data.csv")
y = Float64[]
groups = Int64[]
for line in lines[2:end]
    p = split(line, ',')
    push!(y, parse(Float64, p[1]))
    push!(groups, parse(Int64, p[2]))
end

trace = run_gibbs(y, groups, 100)
println(sum(trace[:, 1]) / 100)
println(sum(trace[:, 2]) / 100)
println(sum(trace[:, 3]) / 100)
