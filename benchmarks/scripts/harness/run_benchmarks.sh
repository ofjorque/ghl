#!/usr/bin/env bash
# Phase 10 Benchmark Execution Harness -- Linux port of run_benchmarks.ps1.
# Runs Hyperfine across all 4 benchmark suites comparing GHL with Python, R, and Julia.
#
# Same suite scripts, same iteration counts (30 runs, warmup 5 for Suite 04/01, warmup
# 3 for Suite 02/03), same output paths as the Windows harness -- so results/raw/*.json
# and results/raw/*.md are directly comparable in shape across machines. Historical
# numbers from prior machines are NOT overwritten here: they live as prose in
# results/summary.md's per-machine sections (see that file's own convention, established
# across the two prior Windows runs) -- this script only produces the raw files for
# *this* run, same as the .ps1 always did.
#
# Difference from the Windows harness: this machine has no EDR blocking the freshly
# built AOT binary (`ghl build --release`), so Suite 04 includes a "GHL (AOT Binary)"
# row here, same as the very first Windows audit had (the second Windows run had to
# skip it due to Bitdefender).
#
# NOTE on hardware isolation: methodology.md's CPU-governor/turbo-boost/taskset/isolcpus
# protocol (Section 3) is intentionally NOT applied here either, for consistency with
# both prior Windows runs -- this is real user hardware, not a dedicated benchmarking
# rig, and flipping the CPU governor system-wide wasn't asked for. Treat these as
# realistic desktop numbers, not lab-isolated ones, exactly like the Windows results.
#
# Usage: bash benchmarks/scripts/harness/run_benchmarks.sh
# Prereqs: hyperfine, R (+ data.table), Julia (+ DataFrames.jl/CSV.jl), a Python venv at
# benchmarks/.venv (numpy/pandas/polars/numba/scipy), target/release/ghl built, and
# target/synthetic_1m.csv generated (see generate_synthetic_csv example).

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

GHL_EXE="./target/release/ghl"
PY_EXE="./benchmarks/.venv/bin/python"
R_EXE="Rscript"
JL_EXE="julia"
HYPERFINE="hyperfine"

echo "================================================================="
echo " GHL Empirical Benchmark Suite (Phase 10) vs Python, R, Julia -- Linux"
echo "================================================================="

RESULTS_DIR="./benchmarks/results/raw"
mkdir -p "$RESULTS_DIR"

# -----------------------------------------------------------------------------
# Suite 04: Startup Latency & Time-To-First-X (TTFX)
# -----------------------------------------------------------------------------
echo
echo ">>> [1/4] Running Suite 04: Startup / TTFX (Hello World)..."

AOT_BIN="./target/hello_aot"
"$GHL_EXE" build benchmarks/scripts/suite_04_runtime/hello.gh --release -o "$AOT_BIN"

"$HYPERFINE" --warmup 5 --runs 30 \
    --export-json "$RESULTS_DIR/suite_04_runtime.json" \
    --export-markdown "$RESULTS_DIR/suite_04_runtime.md" \
    -n "GHL (Interpreted)" "$GHL_EXE run benchmarks/scripts/suite_04_runtime/hello.gh" \
    -n "GHL (AOT Binary)" "$AOT_BIN" \
    -n "Python 3.14" "$PY_EXE benchmarks/scripts/suite_04_runtime/hello.py" \
    -n "R 4.6.1" "$R_EXE benchmarks/scripts/suite_04_runtime/hello.R" \
    -n "Julia" "$JL_EXE benchmarks/scripts/suite_04_runtime/hello.jl"

# -----------------------------------------------------------------------------
# Suite 01: Vector and Matrix Math (Dot Product 10M floats)
# -----------------------------------------------------------------------------
echo
echo ">>> [2/4] Running Suite 01: Math (Dot Product 10M Elements)..."

"$HYPERFINE" --warmup 5 --runs 30 \
    --export-json "$RESULTS_DIR/suite_01_math.json" \
    --export-markdown "$RESULTS_DIR/suite_01_math.md" \
    -n "GHL" "$GHL_EXE run benchmarks/scripts/suite_01_math/bench_dot.gh" \
    -n "Python (NumPy)" "$PY_EXE benchmarks/scripts/suite_01_math/bench_dot.py" \
    -n "R (Base)" "$R_EXE benchmarks/scripts/suite_01_math/bench_dot.R" \
    -n "Julia" "$JL_EXE benchmarks/scripts/suite_01_math/bench_dot.jl"

# -----------------------------------------------------------------------------
# Suite 02: DataFrame Operations (1M Rows CSV Ingestion + Filter + GroupBy + Agg)
# -----------------------------------------------------------------------------
echo
echo ">>> [3/4] Running Suite 02: DataFrame Operations (1M Rows)..."

if [ ! -f target/synthetic_1m.csv ]; then
    echo "target/synthetic_1m.csv missing -- generating (deterministic, same seed as prior machines)..."
    cargo run --release --example generate_synthetic_csv -p ghl-runtime -- 1000000 target/synthetic_1m.csv
fi

"$HYPERFINE" --warmup 3 --runs 30 \
    --export-json "$RESULTS_DIR/suite_02_dataframe.json" \
    --export-markdown "$RESULTS_DIR/suite_02_dataframe.md" \
    -n "GHL (Polars-backed)" "$GHL_EXE run benchmarks/scripts/suite_02_dataframe/bench_df.gh" \
    -n "Python (Pandas)" "$PY_EXE benchmarks/scripts/suite_02_dataframe/bench_df.py" \
    -n "Python (Polars)" "$PY_EXE benchmarks/scripts/suite_02_dataframe/bench_df_polars.py" \
    -n "R (Base)" "$R_EXE benchmarks/scripts/suite_02_dataframe/bench_df.R" \
    -n "R (data.table)" "$R_EXE benchmarks/scripts/suite_02_dataframe/bench_df_datatable.R" \
    -n "Julia (Base Streaming)" "$JL_EXE benchmarks/scripts/suite_02_dataframe/bench_df.jl" \
    -n "Julia (DataFrames.jl)" "$JL_EXE benchmarks/scripts/suite_02_dataframe/bench_df_dataframes.jl"

# -----------------------------------------------------------------------------
# Suite 03: Statistical Modeling (Hierarchical Gibbs Sampler 100 Iterations)
# -----------------------------------------------------------------------------
echo
echo ">>> [4/4] Running Suite 03: Statistical Modeling (Gibbs Sampler 100 iter)..."

"$HYPERFINE" --warmup 3 --runs 30 \
    --export-json "$RESULTS_DIR/suite_03_modeling.json" \
    --export-markdown "$RESULTS_DIR/suite_03_modeling.md" \
    -n "GHL" "$GHL_EXE run benchmarks/scripts/suite_03_modeling/bench_gibbs.gh" \
    -n "Python (NumPy)" "$PY_EXE benchmarks/scripts/suite_03_modeling/bench_gibbs.py" \
    -n "Python (Numba JIT)" "$PY_EXE benchmarks/scripts/suite_03_modeling/bench_gibbs_numba.py" \
    -n "R (Base)" "$R_EXE benchmarks/scripts/suite_03_modeling/bench_gibbs.R" \
    -n "Julia" "$JL_EXE benchmarks/scripts/suite_03_modeling/bench_gibbs.jl" \
    -n "Julia (@inbounds/typed)" "$JL_EXE benchmarks/scripts/suite_03_modeling/bench_gibbs_inbounds.jl"

echo
echo "================================================================="
echo " All benchmark suites completed successfully! Results stored in: "
echo " $RESULTS_DIR"
echo "================================================================="
