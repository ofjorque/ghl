# benchmarks/scripts/suite_07_irt/bench_irt.jl
# Benchmark Suite: IRT Psychometrics in Julia (LinearAlgebra + Quadrature)

using LinearAlgebra
using Statistics

# 1. Read LSAT7 raw data
function read_csv_matrix(path::String)
    lines = readlines(path)
    header = split(lines[1], ",")
    n_cols = length(header)
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

lsat7_mat = read_csv_matrix("benchmarks/data/lsat7_raw.csv")
n_persons, n_items = size(lsat7_mat)

# Quadrature nodes (15 points)
const q_nodes = [-4.0, -3.428, -2.857, -2.285, -1.714, -1.142, -0.571, 0.0, 0.571, 1.142, 1.714, 2.285, 2.857, 3.428, 4.0]
const q_weights = [0.0001, 0.0011, 0.0076, 0.0337, 0.1009, 0.2045, 0.2806, 0.2806, 0.2045, 0.1009, 0.0337, 0.0076, 0.0011, 0.0001, 0.0000]
const n_q = length(q_nodes)

# -----------------------------------------------------------------------------
# 1. LSAT7: 1PL (Rasch) Model
# -----------------------------------------------------------------------------
t0 = time_ns()
b_1pl = zeros(n_items)
for j in 1:n_items
    p_bar = clamp((sum(lsat7_mat[:, j]) + 0.5) / (n_persons + 1.0), 0.05, 0.95)
    b_1pl[j] = -log(p_bar / (1.0 - p_bar))
end

for iter in 1:25
    r_tab = zeros(n_items, n_q)
    n_tab = zeros(n_q)
    for i in 1:n_persons
        log_p = zeros(n_q)
        for q in 1:n_q
            th = q_nodes[q]
            lp = 0.0
            for j in 1:n_items
                p = 1.0 / (1.0 + exp(-(th - b_1pl[j])))
                y = lsat7_mat[i, j]
                lp += y > 0.5 ? log(clamp(p, 1e-4, 1-1e-4)) : log(clamp(1.0 - p, 1e-4, 1-1e-4))
            end
            log_p[q] = lp
        end
        max_lp = maximum(log_p)
        w = exp.(log_p .- max_lp) .* q_weights
        post = w ./ sum(w)
        n_tab .+= post
        for j in 1:n_items
            if lsat7_mat[i, j] > 0.5
                r_tab[j, :] .+= post
            end
        end
    end
    for j in 1:n_items
        g = 0.0
        h = 0.0
        for q in 1:n_q
            p = 1.0 / (1.0 + exp(-(q_nodes[q] - b_1pl[j])))
            diff = r_tab[j, q] - n_tab[q] * p
            g -= diff
            h += n_tab[q] * p * (1.0 - p)
        end
        b_1pl[j] -= clamp(g / (h + 0.1), -0.4, 0.4)
    end
end
t_1pl = (time_ns() - t0) / 1e6

# -----------------------------------------------------------------------------
# 2. LSAT7: 2PL Model
# -----------------------------------------------------------------------------
t0 = time_ns()
a_2pl = ones(n_items)
b_2pl = copy(b_1pl)

for iter in 1:30
    r_tab = zeros(n_items, n_q)
    n_tab = zeros(n_q)
    for i in 1:n_persons
        log_p = zeros(n_q)
        for q in 1:n_q
            th = q_nodes[q]
            lp = 0.0
            for j in 1:n_items
                p = 1.0 / (1.0 + exp(-a_2pl[j] * (th - b_2pl[j])))
                y = lsat7_mat[i, j]
                lp += y > 0.5 ? log(clamp(p, 1e-4, 1-1e-4)) : log(clamp(1.0 - p, 1e-4, 1-1e-4))
            end
            log_p[q] = lp
        end
        max_lp = maximum(log_p)
        w = exp.(log_p .- max_lp) .* q_weights
        post = w ./ sum(w)
        n_tab .+= post
        for j in 1:n_items
            if lsat7_mat[i, j] > 0.5
                r_tab[j, :] .+= post
            end
        end
    end
    for j in 1:n_items
        gb = 0.0; hb = 0.0
        ga = 0.0; ha = 0.0
        for q in 1:n_q
            th = q_nodes[q]
            p = 1.0 / (1.0 + exp(-a_2pl[j] * (th - b_2pl[j])))
            diff = r_tab[j, q] - n_tab[q] * p
            w_p = n_tab[q] * p * (1.0 - p)
            gb -= a_2pl[j] * diff
            hb += (a_2pl[j]^2) * w_p
            ga += (th - b_2pl[j]) * diff
            ha += ((th - b_2pl[j])^2) * w_p
        end
        b_2pl[j] -= clamp(gb / (hb + 0.1), -0.3, 0.3)
        a_2pl[j] += clamp(ga / (ha + 0.1), -0.3, 0.3)
        a_2pl[j] = clamp(a_2pl[j], 0.2, 3.0)
    end
end
t_2pl = (time_ns() - t0) / 1e6

# -----------------------------------------------------------------------------
# 4. High-Dimensional MIRT (MHRM, D = 6)
# -----------------------------------------------------------------------------
N_mhrm = 500
J_mhrm = 12
D_mhrm = 6
data_mhrm = zeros(Float64, N_mhrm, J_mhrm)
for i in 1:N_mhrm, j in 1:J_mhrm
    data_mhrm[i, j] = ((i + j) % 2 == 0) ? 1.0 : 0.0
end

t0 = time_ns()
cycles = 45
a_mhrm = ones(J_mhrm, D_mhrm)
c_mhrm = zeros(J_mhrm)
th_mhrm = zeros(N_mhrm, D_mhrm)
prop_sd = 0.45

for k in 1:cycles
    gain = k <= 15 ? 0.18 : 0.18 / ((k - 14)^0.75)
    prop = th_mhrm .+ randn(N_mhrm, D_mhrm) .* prop_sd
    
    # Vectorized MH
    for i in 1:N_mhrm
        z_curr = a_mhrm * th_mhrm[i, :] .+ c_mhrm
        z_prop = a_mhrm * prop[i, :] .+ c_mhrm
        p_c = 1.0 ./ (1.0 .+ exp.(-clamp.(z_curr, -20.0, 20.0)))
        p_p = 1.0 ./ (1.0 .+ exp.(-clamp.(z_prop, -20.0, 20.0)))
        
        lp_c = sum(data_mhrm[i, :] .* log.(clamp.(p_c, 1e-4, 1-1e-4)) .+ (1.0 .- data_mhrm[i, :]) .* log.(clamp.(1.0 .- p_c, 1e-4, 1-1e-4))) - 0.5 * sum(th_mhrm[i, :].^2)
        lp_p = sum(data_mhrm[i, :] .* log.(clamp.(p_p, 1e-4, 1-1e-4)) .+ (1.0 .- data_mhrm[i, :]) .* log.(clamp.(1.0 .- p_p, 1e-4, 1-1e-4))) - 0.5 * sum(prop[i, :].^2)
        
        if log(rand()) < (lp_p - lp_c)
            th_mhrm[i, :] = prop[i, :]
        end
    end
    
    # RM update
    for j in 1:J_mhrm
        z = th_mhrm * a_mhrm[j, :] .+ c_mhrm[j]
        p = 1.0 ./ (1.0 .+ exp.(-clamp.(z, -20.0, 20.0)))
        w = p .* (1.0 .- p)
        res = data_mhrm[:, j] .- p
        gc = sum(res)
        hc = sum(w) + 0.5
        c_mhrm[j] += gain * clamp(gc / hc, -0.4, 0.4)
        for d in 1:D_mhrm
            ga = sum(res .* th_mhrm[:, d])
            ha = sum(w .* (th_mhrm[:, d].^2)) + 0.5
            a_mhrm[j, d] += gain * clamp(ga / ha, -0.3, 0.3)
            a_mhrm[j, d] = clamp(a_mhrm[j, d], 0.15, 3.0)
        end
    end
end
t_mhrm = (time_ns() - t0) / 1e6

println("=== JULIA BENCHMARK RESULTS ===")
println("TASK_1PL_TIME_MS: ", round(t_1pl, digits=2))
println("TASK_1PL_B: ", join([string(round(b, digits=4)) for b in b_1pl], ", "))
println("TASK_2PL_TIME_MS: ", round(t_2pl, digits=2))
println("TASK_2PL_A: ", join([string(round(a, digits=4)) for a in a_2pl], ", "))
println("TASK_2PL_B: ", join([string(round(b, digits=4)) for b in b_2pl], ", "))
println("TASK_MHRM_TIME_MS: ", round(t_mhrm, digits=2))
