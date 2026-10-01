# benchmarks/scripts/suite_07_irt/bench_irt.jl
# Benchmark Suite: IRT Psychometrics in Julia (Idiomatic, Type-Stable, Preallocated, Warm-up)

using LinearAlgebra
using Statistics
using Random

# 1. Read LSAT7 raw data
function read_csv_matrix(path::String)
    lines = readlines(path)
    n_cols = length(split(lines[1], ","))
    n_rows = length(lines) - 1
    mat = zeros(Float64, n_rows, n_cols)
    for i in 1:n_rows
        parts = split(lines[i+1], ",")
        for j in 1:n_cols
            mat[i, j] = parse(Float64, parts[j])
        end
    end
    return mat
end

# -----------------------------------------------------------------------------
# 1. LSAT7: 1PL (Rasch) Model
# -----------------------------------------------------------------------------
function fit_1pl(mat::Matrix{Float64}, q_nodes::Vector{Float64}, q_weights::Vector{Float64}; max_iter::Int=25)
    n_persons, n_items = size(mat)
    n_q = length(q_nodes)
    b = zeros(n_items)
    for j in 1:n_items
        p_bar = clamp((sum(@view(mat[:, j])) + 0.5) / (n_persons + 1.0), 0.05, 0.95)
        b[j] = -log(p_bar / (1.0 - p_bar))
    end
    r_tab = zeros(n_items, n_q)
    n_tab = zeros(n_q)
    log_p = zeros(n_q)
    w = zeros(n_q)
    post = zeros(n_q)

    for iter in 1:max_iter
        fill!(r_tab, 0.0)
        fill!(n_tab, 0.0)
        for i in 1:n_persons
            for q in 1:n_q
                th = q_nodes[q]
                lp = 0.0
                @inbounds for j in 1:n_items
                    p = 1.0 / (1.0 + exp(-(th - b[j])))
                    y = mat[i, j]
                    lp += y > 0.5 ? log(clamp(p, 1e-4, 1-1e-4)) : log(clamp(1.0 - p, 1e-4, 1-1e-4))
                end
                log_p[q] = lp
            end
            max_lp = maximum(log_p)
            sum_w = 0.0
            @inbounds for q in 1:n_q
                w[q] = exp(log_p[q] - max_lp) * q_weights[q]
                sum_w += w[q]
            end
            inv_sum = 1.0 / sum_w
            @inbounds for q in 1:n_q
                post[q] = w[q] * inv_sum
                n_tab[q] += post[q]
            end
            @inbounds for j in 1:n_items
                if mat[i, j] > 0.5
                    for q in 1:n_q
                        r_tab[j, q] += post[q]
                    end
                end
            end
        end
        @inbounds for j in 1:n_items
            g = 0.0
            h = 0.0
            for q in 1:n_q
                p = 1.0 / (1.0 + exp(-(q_nodes[q] - b[j])))
                diff = r_tab[j, q] - n_tab[q] * p
                g -= diff
                h += n_tab[q] * p * (1.0 - p)
            end
            b[j] -= clamp(g / (h + 0.1), -0.4, 0.4)
        end
    end
    return b
end

# -----------------------------------------------------------------------------
# 2. LSAT7: 2PL Model
# -----------------------------------------------------------------------------
function fit_2pl(mat::Matrix{Float64}, q_nodes::Vector{Float64}, q_weights::Vector{Float64}, init_b::Vector{Float64}; max_iter::Int=30)
    n_persons, n_items = size(mat)
    n_q = length(q_nodes)
    a = ones(n_items)
    b = copy(init_b)
    r_tab = zeros(n_items, n_q)
    n_tab = zeros(n_q)
    log_p = zeros(n_q)
    w = zeros(n_q)
    post = zeros(n_q)

    for iter in 1:max_iter
        fill!(r_tab, 0.0)
        fill!(n_tab, 0.0)
        for i in 1:n_persons
            for q in 1:n_q
                th = q_nodes[q]
                lp = 0.0
                @inbounds for j in 1:n_items
                    p = 1.0 / (1.0 + exp(-a[j] * (th - b[j])))
                    y = mat[i, j]
                    lp += y > 0.5 ? log(clamp(p, 1e-4, 1-1e-4)) : log(clamp(1.0 - p, 1e-4, 1-1e-4))
                end
                log_p[q] = lp
            end
            max_lp = maximum(log_p)
            sum_w = 0.0
            @inbounds for q in 1:n_q
                w[q] = exp(log_p[q] - max_lp) * q_weights[q]
                sum_w += w[q]
            end
            inv_sum = 1.0 / sum_w
            @inbounds for q in 1:n_q
                post[q] = w[q] * inv_sum
                n_tab[q] += post[q]
            end
            @inbounds for j in 1:n_items
                if mat[i, j] > 0.5
                    for q in 1:n_q
                        r_tab[j, q] += post[q]
                    end
                end
            end
        end
        @inbounds for j in 1:n_items
            gb = 0.0; hb = 0.0
            ga = 0.0; ha = 0.0
            for q in 1:n_q
                th = q_nodes[q]
                p = 1.0 / (1.0 + exp(-a[j] * (th - b[j])))
                diff = r_tab[j, q] - n_tab[q] * p
                w_p = n_tab[q] * p * (1.0 - p)
                gb -= a[j] * diff
                hb += (a[j]^2) * w_p
                ga += (th - b[j]) * diff
                ha += ((th - b[j])^2) * w_p
            end
            b[j] -= clamp(gb / (hb + 0.1), -0.3, 0.3)
            a[j] += clamp(ga / (ha + 0.1), -0.3, 0.3)
            a[j] = clamp(a[j], 0.2, 3.0)
        end
    end
    return a, b
end

# -----------------------------------------------------------------------------
# 3. High-Dimensional MIRT (MHRM, D = 6)
# -----------------------------------------------------------------------------
function fit_mhrm(data::Matrix{Float64}, D::Int; cycles::Int=45)
    N, J = size(data)
    a = ones(J, D)
    c = zeros(J)
    th = zeros(N, D)
    prop_sd = 0.45
    z_curr = zeros(J)
    z_prop = zeros(J)
    prop = zeros(N, D)
    rng = MersenneTwister(42)

    for k in 1:cycles
        gain = k <= 15 ? 0.18 : 0.18 / ((k - 14)^0.75)
        for i in 1:N, d in 1:D
            prop[i, d] = th[i, d] + randn(rng) * prop_sd
        end

        for i in 1:N
            for j in 1:J
                zc = c[j]
                zp = c[j]
                for d in 1:D
                    zc += a[j, d] * th[i, d]
                    zp += a[j, d] * prop[i, d]
                end
                z_curr[j] = zc
                z_prop[j] = zp
            end

            lp_c = 0.0
            lp_p = 0.0
            for j in 1:J
                pc = 1.0 / (1.0 + exp(-clamp(z_curr[j], -20.0, 20.0)))
                pp = 1.0 / (1.0 + exp(-clamp(z_prop[j], -20.0, 20.0)))
                y = data[i, j]
                lp_c += y > 0.5 ? log(clamp(pc, 1e-4, 1-1e-4)) : log(clamp(1.0 - pc, 1e-4, 1-1e-4))
                lp_p += y > 0.5 ? log(clamp(pp, 1e-4, 1-1e-4)) : log(clamp(1.0 - pp, 1e-4, 1-1e-4))
            end
            for d in 1:D
                lp_c -= 0.5 * (th[i, d]^2)
                lp_p -= 0.5 * (prop[i, d]^2)
            end

            if log(rand(rng)) < (lp_p - lp_c)
                for d in 1:D
                    th[i, d] = prop[i, d]
                end
            end
        end

        # Robbins-Monro parameter update
        for j in 1:J
            gc = 0.0
            hc = 0.5
            for i in 1:N
                z = c[j]
                for d in 1:D
                    z += a[j, d] * th[i, d]
                end
                p = 1.0 / (1.0 + exp(-clamp(z, -20.0, 20.0)))
                w = p * (1.0 - p)
                res = data[i, j] - p
                gc += res
                hc += w
            end
            c[j] += gain * clamp(gc / hc, -0.4, 0.4)

            for d in 1:D
                ga = 0.0
                ha = 0.5
                for i in 1:N
                    z = c[j]
                    for d2 in 1:D
                        z += a[j, d2] * th[i, d2]
                    end
                    p = 1.0 / (1.0 + exp(-clamp(z, -20.0, 20.0)))
                    w = p * (1.0 - p)
                    res = data[i, j] - p
                    ga += res * th[i, d]
                    ha += w * (th[i, d]^2)
                end
                a[j, d] += gain * clamp(ga / ha, -0.3, 0.3)
                a[j, d] = clamp(a[j, d], 0.15, 3.0)
            end
        end
    end
    return a, c
end

# -----------------------------------------------------------------------------
# Setup Data
# -----------------------------------------------------------------------------
lsat7_mat = read_csv_matrix("benchmarks/data/lsat7_raw.csv")
const q_nodes = [-4.0, -3.428, -2.857, -2.285, -1.714, -1.142, -0.571, 0.0, 0.571, 1.142, 1.714, 2.285, 2.857, 3.428, 4.0]
const q_weights = [0.0001, 0.0011, 0.0076, 0.0337, 0.1009, 0.2045, 0.2806, 0.2806, 0.2045, 0.1009, 0.0337, 0.0076, 0.0011, 0.0001, 0.0000]

N_mhrm = 500
J_mhrm = 12
D_mhrm = 6
data_mhrm = zeros(Float64, N_mhrm, J_mhrm)
for i in 1:N_mhrm, j in 1:J_mhrm
    data_mhrm[i, j] = ((i + j) % 2 == 0) ? 1.0 : 0.0
end

# -----------------------------------------------------------------------------
# Warm-up passes (Compiles LLVM JIT code before starting timers)
# -----------------------------------------------------------------------------
fit_1pl(lsat7_mat[1:50, :], q_nodes, q_weights; max_iter=1)
fit_2pl(lsat7_mat[1:50, :], q_nodes, q_weights, [0.0, 0.0, 0.0, 0.0, 0.0]; max_iter=1)
fit_mhrm(data_mhrm[1:50, :], D_mhrm; cycles=1)

# -----------------------------------------------------------------------------
# Benchmark Executions
# -----------------------------------------------------------------------------
t0 = time_ns()
b_1pl = fit_1pl(lsat7_mat, q_nodes, q_weights; max_iter=25)
t_1pl = (time_ns() - t0) / 1e6

t0 = time_ns()
a_2pl, b_2pl = fit_2pl(lsat7_mat, q_nodes, q_weights, b_1pl; max_iter=30)
t_2pl = (time_ns() - t0) / 1e6

t0 = time_ns()
a_mhrm, c_mhrm = fit_mhrm(data_mhrm, D_mhrm; cycles=45)
t_mhrm = (time_ns() - t0) / 1e6

println("=== JULIA BENCHMARK RESULTS ===")
println("TASK_1PL_TIME_MS: ", round(t_1pl, digits=2))
println("TASK_1PL_B: ", join([string(round(b, digits=4)) for b in b_1pl], ", "))
println("TASK_2PL_TIME_MS: ", round(t_2pl, digits=2))
println("TASK_2PL_A: ", join([string(round(a, digits=4)) for a in a_2pl], ", "))
println("TASK_2PL_B: ", join([string(round(b, digits=4)) for b in b_2pl], ", "))
println("TASK_MHRM_TIME_MS: ", round(t_mhrm, digits=2))
