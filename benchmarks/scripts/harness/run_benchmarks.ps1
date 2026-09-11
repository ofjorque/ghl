# Phase 10 Benchmark Execution Harness
# Runs Hyperfine across all 4 benchmark suites comparing GHL with Python, R, and Julia

$ErrorActionPreference = "Stop"

$ghlExe = ".\target\release\ghl.exe"
$pyExe  = ".\benchmarks\.venv\Scripts\python.exe"
$rExe   = "Rscript"
$jlExe  = "julia"

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

hyperfine --warmup 3 --runs 10 `
    --export-json $suite04Json `
    --export-markdown $suite04Md `
    -n "GHL (AOT Binary)" ".\benchmarks\scripts\suite_04_runtime\hello_aot.exe" `
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

hyperfine --warmup 2 --runs 5 `
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

hyperfine --warmup 1 --runs 3 `
    --export-json $suite02Json `
    --export-markdown $suite02Md `
    -n "GHL (Polars-backed)" "$ghlExe run benchmarks\scripts\suite_02_dataframe\bench_df.gh" `
    -n "Python (Pandas)" "$pyExe benchmarks\scripts\suite_02_dataframe\bench_df.py" `
    -n "R (Base)" "$rExe benchmarks\scripts\suite_02_dataframe\bench_df.R" `
    -n "Julia (Base Streaming)" "$jlExe benchmarks\scripts\suite_02_dataframe\bench_df.jl"

# -----------------------------------------------------------------------------
# Suite 03: Statistical Modeling (Hierarchical Gibbs Sampler 100 Iterations)
# -----------------------------------------------------------------------------
Write-Host "`n>>> [4/4] Running Suite 03: Statistical Modeling (Gibbs Sampler 100 iter)..." -ForegroundColor Yellow
$suite03Json = "$resultsDir\suite_03_modeling.json"
$suite03Md   = "$resultsDir\suite_03_modeling.md"

hyperfine --warmup 1 --runs 3 `
    --export-json $suite03Json `
    --export-markdown $suite03Md `
    -n "GHL" "$ghlExe run benchmarks\scripts\suite_03_modeling\bench_gibbs.gh" `
    -n "Python (NumPy)" "$pyExe benchmarks\scripts\suite_03_modeling\bench_gibbs.py" `
    -n "R (Base)" "$rExe benchmarks\scripts\suite_03_modeling\bench_gibbs.R" `
    -n "Julia" "$jlExe benchmarks\scripts\suite_03_modeling\bench_gibbs.jl"

Write-Host "`n=================================================================" -ForegroundColor Green
Write-Host " All benchmark suites completed successfully! Results stored in: " -ForegroundColor Green
Write-Host " $resultsDir" -ForegroundColor Green
Write-Host "=================================================================" -ForegroundColor Green
