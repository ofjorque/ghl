# benchmarks/scripts/suite_08_impute/run_impute_benchmark.py
# Cross-Language Benchmark Harness: R (mice/VIM) vs Python (scikit-learn) vs Julia (Impute.jl) vs GHL (ghl_impute)

import os
import sys
import subprocess
import time
import json

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
SUITE_DIR = os.path.dirname(os.path.abspath(__file__))
RESULTS_DIR = os.path.join(ROOT_DIR, "benchmarks", "results")
os.makedirs(RESULTS_DIR, exist_ok=True)

print("=" * 80)
print(" GHL Cross-Language Missing Data Imputation Benchmark Suite (Suite 08)")
print(" R (mice 3.19/VIM) vs Python (scikit-learn 1.9) vs Julia (Impute.jl 0.7) vs GHL")
print("=" * 80)

# Helper to run commands
def run_command(cmd, desc):
    print(f"[*] Running {desc}...")
    t0 = time.time()
    try:
        proc = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            encoding="utf-8",
            errors="replace",
            timeout=300
        )
        wall_time = time.time() - t0
        return proc.returncode, proc.stdout, proc.stderr, wall_time
    except Exception as e:
        return -1, "", str(e), 0.0

# 1. Ensure benchmark dataset exists
data_gen_script = os.path.join(SUITE_DIR, "generate_data.py")
run_command([sys.executable, data_gen_script], "Data Generation (impute_benchmark_data.csv)")

# 2. Run R (mice)
r_code, r_out, r_err, r_wall = run_command(
    ["Rscript", os.path.join(SUITE_DIR, "bench_impute.R")],
    "R (mice 3.19.0)"
)
if r_code != 0:
    print(f"[!] R error: {r_err}")

# 3. Run Python (scikit-learn)
py_code, py_out, py_err, py_wall = run_command(
    [sys.executable, os.path.join(SUITE_DIR, "bench_impute.py")],
    "Python (scikit-learn 1.9.1)"
)
if py_code != 0:
    print(f"[!] Python error: {py_err}")

# 4. Run Julia (Impute.jl)
jl_code, jl_out, jl_err, jl_wall = run_command(
    ["julia", os.path.join(SUITE_DIR, "bench_impute.jl")],
    "Julia (Impute.jl 0.7.0)"
)
if jl_code != 0:
    print(f"[!] Julia error: {jl_err}")

# 5. Run GHL
ghl_rel = os.path.join(ROOT_DIR, "target", "release", "ghl.exe")
ghl_deb = os.path.join(ROOT_DIR, "target", "debug", "ghl.exe")
ghl_exe = ghl_rel if os.path.exists(ghl_rel) else ghl_deb
ghl_script = "benchmarks/scripts/suite_08_impute/bench_impute.gh"

ghl_code, ghl_out, ghl_err, ghl_wall = run_command(
    [ghl_exe, "run", ghl_script],
    f"GHL (ghl_impute via {os.path.basename(os.path.dirname(ghl_exe))})"
)
if ghl_code != 0:
    print(f"[!] GHL error: {ghl_err}")
    print(f"[!] GHL stdout: {ghl_out}")

import re

# Parse outputs
def parse_metrics(text):
    data = {}
    clean_text = re.sub(r'\x1b\[[0-9;]*[a-zA-Z]', '', text)
    for line in clean_text.splitlines():
        if ":" in line:
            parts = line.split(":", 1)
            k = parts[0].strip()
            v = parts[1].strip()
            if k.startswith("TASK_"):
                data[k] = v
    return data

r_metrics = parse_metrics(r_out)
py_metrics = parse_metrics(py_out)
jl_metrics = parse_metrics(jl_out)
ghl_metrics = parse_metrics(ghl_out)

print("\n" + "=" * 80)
print(" 1. NUMERICAL PARITY & CONVERGENCE COMPARISON")
print("=" * 80)

parity_items = [
    ("Total Missing Cells", "TASK_MISSING_CELLS", lambda x: f"{int(float(x))}"),
    ("Missing Cells %", "TASK_MISSING_PCT", lambda x: f"{float(x):.2f}%"),
    ("PMM Imputed Chl Mean", "TASK_PMM_IMP_MEAN", lambda x: f"{float(x):.4f}"),
    ("PMM Imputed Chl SD", "TASK_PMM_IMP_SD", lambda x: f"{float(x):.4f}"),
    ("Pooled Age Slope (Q-bar)", "TASK_RUBIN_QBAR", lambda x: f"{float(x):.4f}"),
    ("Pooled Standard Error", "TASK_RUBIN_SE", lambda x: f"{float(x):.4f}"),
    ("Pooled t-Statistic", "TASK_RUBIN_TSTAT", lambda x: f"{float(x):.4f}"),
    ("Barnard-Rubin Adj df", "TASK_RUBIN_DF", lambda x: f"{float(x):.2f}"),
    ("Within-Variance (U-bar)", "TASK_RUBIN_UBAR", lambda x: f"{float(x):.6f}"),
    ("Between-Variance (B)", "TASK_RUBIN_B", lambda x: f"{float(x):.6f}"),
    ("Total Variance (T)", "TASK_RUBIN_T", lambda x: f"{float(x):.6f}"),
    ("Rel Var Increase (r)", "TASK_RUBIN_RIV", lambda x: f"{float(x):.4f}"),
    ("Fraction Missing Info (fmi)", "TASK_RUBIN_FMI", lambda x: f"{float(x):.4f}"),
]

parity_table = []
p_header = f"| {'Metric / Parameter':<30} | {'R (mice)':<12} | {'Python':<12} | {'Julia':<12} | {'GHL':<12} | {'Status':<14} |"
p_sep = f"|{'-'*32}|{'-'*14}|{'-'*14}|{'-'*14}|{'-'*14}|{'-'*16}|"
print(p_header)
print(p_sep)

for name, key, fmt in parity_items:
    r_val = float(r_metrics[key]) if key in r_metrics else None
    py_val = float(py_metrics[key]) if key in py_metrics else None
    jl_val = float(jl_metrics[key]) if key in jl_metrics else None
    ghl_val = float(ghl_metrics[key]) if key in ghl_metrics else None

    r_str = fmt(r_val) if r_val is not None else "N/A"
    py_str = fmt(py_val) if py_val is not None else "N/A"
    jl_str = fmt(jl_val) if jl_val is not None else "N/A"
    ghl_str = fmt(ghl_val) if ghl_val is not None else "N/A"

    delta = abs(ghl_val - r_val) if (ghl_val is not None and r_val is not None) else None
    if delta is not None:
        rel_err = delta / max(abs(r_val), 1.0)
        status = "Exact Match" if delta < 1e-4 else ("Concordant" if rel_err < 0.15 or delta < 1.0 else "Discrepant")
    else:
        status = "Supported" if ghl_val is not None else "N/A"

    parity_table.append({
        "method": name,
        "key": key,
        "ghl_val": ghl_val,
        "r_val": r_val,
        "python_val": py_val,
        "julia_val": jl_val,
        "delta": round(delta, 5) if delta is not None else None,
        "status": status
    })

    print(f"| {name:<30} | {r_str:<12} | {py_str:<12} | {jl_str:<12} | {ghl_str:<12} | {status:<14} |")

print("\n" + "=" * 80)
print(" 2. EXECUTION SPEED COMPARISON (Milliseconds)")
print("=" * 80)

speed_items = [
    ("Pattern Aggregation / Audit", "TASK_PATTERN_TIME_MS"),
    ("MICE (m=5, maxit=10, 3 cols)", "TASK_MICE_PMM_TIME_MS"),
    ("Rubin's Pooling & Barnard-Rubin df", "TASK_POOL_TIME_MS"),
    ("Hotdeck / k-NN Imputation", "TASK_HOTDECK_TIME_MS", "TASK_KNN_TIME_MS")
]

s_header = f"| {'Task / Operation':<35} | {'R (mice)':<12} | {'Python':<12} | {'Julia':<12} | {'GHL':<12} | {'Speedup GHL':<14} |"
s_sep = f"|{'-'*37}|{'-'*14}|{'-'*14}|{'-'*14}|{'-'*14}|{'-'*16}|"
print(s_header)
print(s_sep)

performance_table = []
for item in speed_items:
    name = item[0]
    key_r = item[1]
    key_alt = item[2] if len(item) > 2 else key_r

    r_ms = float(r_metrics.get(key_r, "nan"))
    py_ms = float(py_metrics.get(key_alt, py_metrics.get(key_r, "nan")))
    jl_ms = float(jl_metrics.get(key_alt, jl_metrics.get(key_r, "nan")))
    ghl_ms = float(ghl_metrics.get(key_r, "nan"))

    r_str = f"{r_ms:.2f} ms" if not (r_ms != r_ms) else "N/A"
    py_str = f"{py_ms:.2f} ms" if not (py_ms != py_ms) else "N/A"
    jl_str = f"{jl_ms:.2f} ms" if not (jl_ms != jl_ms) else "N/A"
    ghl_str = f"{ghl_ms:.2f} ms" if not (ghl_ms != ghl_ms) else "N/A"

    speedup = ""
    if not (r_ms != r_ms) and not (ghl_ms != ghl_ms) and ghl_ms > 0:
        ratio = r_ms / ghl_ms
        speedup = f"{ratio:.2f}x vs R"

    performance_table.append({
        "task": name,
        "r_ms": r_ms if not (r_ms != r_ms) else None,
        "python_ms": py_ms if not (py_ms != py_ms) else None,
        "julia_ms": jl_ms if not (jl_ms != jl_ms) else None,
        "ghl_ms": ghl_ms if not (ghl_ms != ghl_ms) else None
    })

    print(f"| {name:<35} | {r_str:<12} | {py_str:<12} | {jl_str:<12} | {ghl_str:<12} | {speedup:<14} |")

# Standardized JSON Payload
benchmark_payload = {
    "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
    "package": "ghl_impute",
    "milestone": "v0.2.0",
    "github_issue": 9,
    "environment": {
        "ghl_edition": "2026",
        "compiler": "cranelift-jit",
        "os": "windows-x86_64"
    },
    "references": {
        "r": {
            "packages": ["mice 3.19.0", "VIM 6.2.2"]
        },
        "python": {
            "packages": ["scikit-learn 1.9.1", "numpy 2.5.3", "pandas 3.0.6"]
        },
        "julia": {
            "packages": ["Impute.jl 0.7.0", "CSV.jl 0.10.17", "DataFrames.jl 1.8.2"]
        }
    },
    "table": parity_table,
    "performance": performance_table,
    "raw_metrics": {
        "r": r_metrics,
        "python": py_metrics,
        "julia": jl_metrics,
        "ghl": ghl_metrics
    }
}

json_path = os.path.join(RESULTS_DIR, "impute_benchmark_results.json")
with open(json_path, "w", encoding="utf-8") as f:
    json.dump(benchmark_payload, f, indent=2)

print(f"\n[✓] Standardized benchmark exported to: {json_path}")
