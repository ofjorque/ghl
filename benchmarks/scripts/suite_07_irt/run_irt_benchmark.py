# benchmarks/scripts/suite_07_irt/run_irt_benchmark.py
# Cross-Language Benchmark Harness: R (mirt) vs Python (girth) vs Julia vs GHL (ghl_irt)

import os
import sys
import glob
import subprocess
import time
import json
import re

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

print("=" * 70)
print(" GHL Cross-Language IRT Benchmark: GHL vs R (mirt) vs Python vs Julia")
print("=" * 70)

# 1. GHL script path (uses native `use ghl_irt::*;`)
main_script_path = "benchmarks/scripts/suite_07_irt/bench_irt_main.gh"
print(f"[*] GHL benchmark script: {main_script_path} (using native `use ghl_irt::*;`)")

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

# 2. Run R (mirt)
r_code, r_out, r_err, r_wall = run_command(
    ["Rscript", os.path.join(SUITE_DIR, "bench_irt.R")],
    "R (mirt 1.47)"
)

# 3. Run Python (girth)
py_code, py_out, py_err, py_wall = run_command(
    [sys.executable, os.path.join(SUITE_DIR, "bench_irt.py")],
    "Python (girth 0.8.0)"
)

# 4. Run Julia
jl_code, jl_out, jl_err, jl_wall = run_command(
    ["julia", os.path.join(SUITE_DIR, "bench_irt.jl")],
    "Julia 1.12.7"
)

# 5. Run GHL
ghl_exe = os.path.join("target", "release", "ghl.exe")
ghl_code, ghl_out, ghl_err, ghl_wall = run_command(
    [ghl_exe, "run", main_script_path],
    "GHL (ghl_irt / Cranelift JIT)"
)
if ghl_code != 0:
    print(f"[!] GHL failed with return code {ghl_code}")
    print(f"[!] GHL stderr: {ghl_err}")
    print(f"[!] GHL stdout: {ghl_out}")

# Parse output values
def parse_metrics(text):
    data = {}
    for line in text.splitlines():
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

print("\n" + "=" * 70)
print(" EMPIRICAL RESULTS SUMMARY")
print("=" * 70)

tasks = [
    ("TASK_1PL_TIME_MS", "1PL (Rasch) Model (LSAT7, N=1000, J=5)"),
    ("TASK_2PL_TIME_MS", "2PL (Birnbaum) Model (LSAT7, N=1000, J=5)"),
    ("TASK_GRM_TIME_MS", "Graded Response Model (Science, N=392, J=4)"),
    ("TASK_MHRM_TIME_MS", "High-Dim MIRT MHRM (D=6, N=500, J=12)"),
    ("TASK_DIF_TIME_MS", "Multigroup LRT DIF (LSAT7, N=1000, J=5)")
]

table_rows = []
for key, label in tasks:
    r_val = float(r_metrics.get(key, "nan")) if key in r_metrics else float("nan")
    py_val = float(py_metrics.get(key, "nan")) if key in py_metrics else float("nan")
    jl_val = float(jl_metrics.get(key, "nan")) if key in jl_metrics else float("nan")
    ghl_val = float(ghl_metrics.get(key, "nan")) if key in ghl_metrics else float("nan")

    row = {
        "task": label,
        "key": key,
        "r_ms": r_val,
        "python_ms": py_val,
        "julia_ms": jl_val,
        "ghl_ms": ghl_val
    }
    table_rows.append(row)

# Print Table
header = f"| {'Task / Benchmark':<45} | {'R (mirt)':<10} | {'Python':<10} | {'Julia':<10} | {'GHL':<10} | {'Ratio GHL/R':<12} |"
sep = f"|{'-'*47}|{'-'*12}|{'-'*12}|{'-'*12}|{'-'*12}|{'-'*14}|"
print(header)
print(sep)

for r in table_rows:
    r_str = f"{r['r_ms']:.1f} ms" if not (r['r_ms'] != r['r_ms']) else "N/A"
    py_str = f"{r['python_ms']:.1f} ms" if not (r['python_ms'] != r['python_ms']) else "N/A"
    jl_str = f"{r['julia_ms']:.1f} ms" if not (r['julia_ms'] != r['julia_ms']) else "N/A"
    ghl_str = f"{r['ghl_ms']:.1f} ms" if not (r['ghl_ms'] != r['ghl_ms']) else "N/A"
    
    speedup = ""
    if not (r['r_ms'] != r['r_ms']) and not (r['ghl_ms'] != r['ghl_ms']) and ghl_str != "N/A" and r['ghl_ms'] > 0:
        ratio = r['r_ms'] / r['ghl_ms']
        speedup = f"{ratio:.2f}x faster" if ratio >= 1.0 else f"{1.0/ratio:.2f}x slower"
    print(f"| {r['task']:<45} | {r_str:<10} | {py_str:<10} | {jl_str:<10} | {ghl_str:<10} | {speedup:<12} |")

# Parameter precision recovery comparisons
print("\n" + "=" * 70)
print(" PRECISION & PARAMETER RECOVERY VERIFICATION")
print("=" * 70)
print(f"Rasch Item Difficulties (LSAT7):")
print(f"  R (mirt):   {r_metrics.get('TASK_1PL_B', 'N/A')}")
print(f"  Python:     {py_metrics.get('TASK_1PL_B', 'N/A')}")
print(f"  Julia:      {jl_metrics.get('TASK_1PL_B', 'N/A')}")
print(f"  GHL:        {ghl_metrics.get('TASK_1PL_B', 'N/A')}")
print("-" * 70)
print(f"2PL Discrimination Slopes (LSAT7):")
print(f"  R (mirt):   {r_metrics.get('TASK_2PL_A', 'N/A')}")
print(f"  Python:     {py_metrics.get('TASK_2PL_A', 'N/A')}")
print(f"  GHL:        {ghl_metrics.get('TASK_2PL_A', 'N/A')}")
print("-" * 70)
print(f"2PL Item Difficulties (LSAT7):")
print(f"  R (mirt):   {r_metrics.get('TASK_2PL_B', 'N/A')}")
print(f"  Python:     {py_metrics.get('TASK_2PL_B', 'N/A')}")
print(f"  GHL:        {ghl_metrics.get('TASK_2PL_B', 'N/A')}")
print("-" * 70)
print(f"Graded Response Discrimination Slopes (Science):")
print(f"  R (mirt):   {r_metrics.get('TASK_GRM_A', 'N/A')}")
print(f"  Python:     {py_metrics.get('TASK_GRM_A', 'N/A')}")
print(f"  GHL:        {ghl_metrics.get('TASK_GRM_A', 'N/A')}")

# Save JSON results
benchmark_payload = {
    "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
    "table": table_rows,
    "r": r_metrics,
    "python": py_metrics,
    "julia": jl_metrics,
    "ghl": ghl_metrics
}

res_json_path = os.path.join(RESULTS_DIR, "irt_benchmark_results.json")
with open(res_json_path, "w", encoding="utf-8") as fjson:
    json.dump(benchmark_payload, fjson, indent=2)

print(f"\n[✓] Results successfully exported to {res_json_path}")
