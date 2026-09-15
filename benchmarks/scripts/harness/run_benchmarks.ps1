# Phase 10 Benchmark Execution Harness
# Runs Hyperfine across all 4 benchmark suites comparing GHL with Python, R, and Julia.
#
# v2: bumped to >=30 iterations per methodology.md, and added an "idiomatic
# optimized" variant per competing language where the naive baseline was not
# representative of real-world best practice (Polars for Python, data.table
# for R, DataFrames.jl for Julia in Suite 02; Numba-JIT for Python in Suite 03;
# @inbounds/type-annotated Julia in Suite 03). Baselines are kept alongside the
# optimized variants rather than replaced, so both stories stay visible.
#
# NOTE on hardware isolation: methodology.md's CPU-governor/turbo-boost/taskset
# protocol targets Linux and does not apply on this Windows machine. No such
# isolation is performed here; treat these as realistic desktop numbers, not
# lab-isolated ones.
#
# NOTE on this re-run (2026-09-11, different Windows machine than the original
# Phase 10 audit): this machine's Bitdefender Endpoint Protection blocks
# execution of the freshly-compiled `hello_aot.exe` (Cranelift-emitted PE,
# unsigned, flagged by heuristic/behavioral scanning — confirmed NOT a general
# "block new exes" policy, since a plain rustc-built hello-world binary runs
# fine). This is an EDR decision on a machine we don't control, not something
# to route around. The "GHL (AOT Binary)" row is therefore skipped in Suite 04
# on this run; "GHL (Interpreted)" still runs and is directly comparable to
# the other interpreted/JIT runtimes below.

$ErrorActionPreference = "Stop"

$ghlExe     = ".\target\release\ghl.exe"
$pyExe      = ".\benchmarks\.venv\Scripts\python.exe"
$rExe       = if (Test-Path "C:\Program Files\R\R-4.6.1\bin\Rscript.exe") {
    (New-Object -ComObject Scripting.FileSystemObject).GetFile("C:\Program Files\R\R-4.6.1\bin\Rscript.exe").ShortPath
} elseif (Get-Command Rscript -ErrorAction SilentlyContinue) {
    (Get-Command Rscript).Source
} else {
    "Rscript"
}
$jlExe      = if (Get-Command julia -ErrorAction SilentlyContinue) { (Get-Command julia).Source } else { "C:\Users\ofjorque\AppData\Local\Programs\Julia-1.13.0\bin\julia.exe" }
$hyperfine  = if (Get-Command hyperfine -ErrorAction SilentlyContinue) { (Get-Command hyperfine).Source } else { "hyperfine" }

Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host " GHL Empirical Benchmark Suite (Phase 10) vs Python, R, Julia   " -ForegroundColor Cyan
Write-Host "=================================================================" -ForegroundColor Cyan

# 1. Ensure output directory
$resultsDir = ".\benchmarks\results\raw"
if (-not (Test-Path $resultsDir)) {
    New-Item -ItemType Directory -Force -Path $resultsDir | Out-Null
}

# -----------------------------------------------------------------------------
# Suite 04: Startup Latency & Time-To-First-X (TTFX)
# -----------------------------------------------------------------------------
Write-Host "`n>>> [1/4] Running Suite 04: Startup / TTFX (Hello World)..." -ForegroundColor Yellow
$suite04Json = "$resultsDir\suite_04_runtime.json"
$suite04Md   = "$resultsDir\suite_04_runtime.md"

& $hyperfine --warmup 5 --runs 30 `
    --export-json $suite04Json `
    --export-markdown $suite04Md `
    -n "GHL (Interpreted)" "$ghlExe run benchmarks\scripts\suite_04_runtime\hello.gh" `
    -n "Python 3.14" "$pyExe benchmarks\scripts\suite_04_runtime\hello.py" `
    -n "R 4.6.1" "$rExe benchmarks\scripts\suite_04_runtime\hello.R" `
    -n "Julia" "$jlExe benchmarks\scripts\suite_04_runtime\hello.jl"

# -----------------------------------------------------------------------------
# Suite 01: Vector and Matrix Math (Dot Product 10M floats)
# -----------------------------------------------------------------------------
Write-Host "`n>>> [2/4] Running Suite 01: Math (Dot Product 10M Elements)..." -ForegroundColor Yellow
$suite01Json = "$resultsDir\suite_01_math.json"
$suite01Md   = "$resultsDir\suite_01_math.md"

# NumPy's np.dot and Julia's LinearAlgebra.dot already dispatch to BLAS ddot
# for Float64 vectors, so they are already each ecosystem's "optimized" path.
# R here links against reference BLAS (not OpenBLAS/MKL), so base `sum(a*b)`
# is realistically as fast as `crossprod` gets on this machine; no separate
# "optimized R" variant is added for this suite (see summary.md for the caveat
# that NumPy/Julia ship a tuned BLAS out of the box while this R does not).
& $hyperfine --warmup 5 --runs 30 `
    --export-json $suite01Json `
    --export-markdown $suite01Md `
    -n "GHL" "$ghlExe run benchmarks\scripts\suite_01_math\bench_dot.gh" `
    -n "Python (NumPy)" "$pyExe benchmarks\scripts\suite_01_math\bench_dot.py" `
    -n "R (Base)" "$rExe benchmarks\scripts\suite_01_math\bench_dot.R" `
    -n "Julia" "$jlExe benchmarks\scripts\suite_01_math\bench_dot.jl"

# -----------------------------------------------------------------------------
# Suite 02: DataFrame Operations (1M Rows CSV Ingestion + Filter + GroupBy + Agg)
# -----------------------------------------------------------------------------
Write-Host "`n>>> [3/4] Running Suite 02: DataFrame Operations (1M Rows)..." -ForegroundColor Yellow
$suite02Json = "$resultsDir\suite_02_dataframe.json"
$suite02Md   = "$resultsDir\suite_02_dataframe.md"

& $hyperfine --warmup 3 --runs 30 `
    --export-json $suite02Json `
    --export-markdown $suite02Md `
    -n "GHL (Polars-backed)" "$ghlExe run benchmarks\scripts\suite_02_dataframe\bench_df.gh" `
    -n "Python (Pandas)" "$pyExe benchmarks\scripts\suite_02_dataframe\bench_df.py" `
    -n "Python (Polars)" "$pyExe benchmarks\scripts\suite_02_dataframe\bench_df_polars.py" `
    -n "R (Base)" "$rExe benchmarks\scripts\suite_02_dataframe\bench_df.R" `
    -n "R (data.table)" "$rExe benchmarks\scripts\suite_02_dataframe\bench_df_datatable.R" `
    -n "Julia (Base Streaming)" "$jlExe benchmarks\scripts\suite_02_dataframe\bench_df.jl" `
    -n "Julia (DataFrames.jl)" "$jlExe benchmarks\scripts\suite_02_dataframe\bench_df_dataframes.jl"

# -----------------------------------------------------------------------------
# Suite 03: Statistical Modeling (Hierarchical Gibbs Sampler 100 Iterations)
# -----------------------------------------------------------------------------
Write-Host "`n>>> [4/4] Running Suite 03: Statistical Modeling (Gibbs Sampler 100 iter)..." -ForegroundColor Yellow
$suite03Json = "$resultsDir\suite_03_modeling.json"
$suite03Md   = "$resultsDir\suite_03_modeling.md"

& $hyperfine --warmup 3 --runs 30 `
    --export-json $suite03Json `
    --export-markdown $suite03Md `
    -n "GHL" "$ghlExe run benchmarks\scripts\suite_03_modeling\bench_gibbs.gh" `
    -n "Python (NumPy)" "$pyExe benchmarks\scripts\suite_03_modeling\bench_gibbs.py" `
    -n "Python (Numba JIT)" "$pyExe benchmarks\scripts\suite_03_modeling\bench_gibbs_numba.py" `
    -n "R (Base)" "$rExe benchmarks\scripts\suite_03_modeling\bench_gibbs.R" `
    -n "Julia" "$jlExe benchmarks\scripts\suite_03_modeling\bench_gibbs.jl" `
    -n "Julia (@inbounds/typed)" "$jlExe benchmarks\scripts\suite_03_modeling\bench_gibbs_inbounds.jl"

Write-Host "`n=================================================================" -ForegroundColor Green
Write-Host " All benchmark suites completed successfully! Results stored in: " -ForegroundColor Green
Write-Host " $resultsDir" -ForegroundColor Green
Write-Host "=================================================================" -ForegroundColor Green
