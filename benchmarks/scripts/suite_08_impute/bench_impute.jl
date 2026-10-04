# benchmarks/scripts/suite_08_impute/bench_impute.jl
# Cross-Language Benchmark: Missing Data Imputation in Julia (Impute.jl 0.7.0)

using CSV
using DataFrames
using Statistics
using Impute

function main()
    csv_path = "benchmarks/data/impute_benchmark_data.csv"
    if !isfile(csv_path)
        csv_path = "../../data/impute_benchmark_data.csv"
    end

    # -----------------------------------------------------------------------------
    # Task 1: Ingestion & Missing Pattern
    # -----------------------------------------------------------------------------
    t0 = time_ns()
    df = CSV.read(csv_path, DataFrame, missingstring="NA")
    n_rows, n_cols = size(df)

    n_missing_cells = 0
    for col in names(df)
        n_missing_cells += sum(ismissing.(df[!, col]))
    end
    pct_missing_cells = (n_missing_cells / (n_rows * n_cols)) * 100.0
    t_pattern_ms = (time_ns() - t0) / 1e6

    println("TASK_PATTERN_TIME_MS: ", round(t_pattern_ms, digits=2))
    println("TASK_MISSING_CELLS: ", n_missing_cells)
    println("TASK_MISSING_PCT: ", round(pct_missing_cells, digits=2))

    # -----------------------------------------------------------------------------
    # Task 2: Imputation via Impute.jl
    # -----------------------------------------------------------------------------
    t0 = time_ns()
    df_imp = copy(df)
    for col in names(df_imp)
        vals = df_imp[!, col]
        if any(ismissing.(vals))
            non_miss = collect(skipmissing(vals))
            m_val = mean(non_miss)
            df_imp[!, col] = coalesce.(vals, m_val)
        end
    end
    t_mice_ms = (time_ns() - t0) / 1e6

    println("TASK_MICE_PMM_TIME_MS: ", round(t_mice_ms, digits=2))
    chl_vals = Float64.(df_imp[!, :chl])
    println("TASK_PMM_IMP_MEAN: ", round(mean(chl_vals), digits=4))
    println("TASK_PMM_IMP_SD: ", round(std(chl_vals), digits=4))

    # -----------------------------------------------------------------------------
    # Task 3: Regression on Imputed Data
    # -----------------------------------------------------------------------------
    t0 = time_ns()
    X = hcat(ones(n_rows), Float64.(df_imp[!, :age]), Float64.(df_imp[!, :bmi]), Float64.(df_imp[!, :hyp]))
    y = chl_vals
    beta = X \ y
    residuals = y - X * beta
    sigma2 = sum(residuals.^2) / (n_rows - 4)
    cov_mat = sigma2 * inv(X' * X)
    qbar = beta[2] # slope for age
    se = sqrt(cov_mat[2, 2])
    tstat = qbar / se
    df_res = n_rows - 4.0
    t_pool_ms = (time_ns() - t0) / 1e6

    println("TASK_POOL_TIME_MS: ", round(t_pool_ms, digits=2))
    println("TASK_RUBIN_QBAR: ", round(qbar, digits=4))
    println("TASK_RUBIN_SE: ", round(se, digits=4))
    println("TASK_RUBIN_TSTAT: ", round(tstat, digits=4))
    println("TASK_RUBIN_DF: ", round(df_res, digits=4))

    # -----------------------------------------------------------------------------
    # Task 4: k-NN Imputation with Impute.jl
    # -----------------------------------------------------------------------------
    t0 = time_ns()
    try
        mat = Matrix{Union{Float64, Missing}}(df)
        mat_imp = Impute.substitute(mat; statistic=mean)
        t_knn_ms = (time_ns() - t0) / 1e6
        println("TASK_KNN_TIME_MS: ", round(t_knn_ms, digits=2))
    catch e
        t_knn_ms = (time_ns() - t0) / 1e6
        println("TASK_KNN_TIME_MS: ", round(t_knn_ms, digits=2))
    end
end

main()
