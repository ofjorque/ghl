import numpy as np
import pandas as pd
import os
import sys

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

def generate_benchmark_data(n=500, seed=42):
    np.random.seed(seed)
    
    # 1. Fully observed baseline variables
    age = np.random.randint(20, 66, size=n)
    
    # 2. Continuous BMI with true relationship: bmi = 20.0 + 0.12 * age + eps
    eps_bmi = np.random.normal(0, 3.5, size=n)
    bmi_true = 20.0 + 0.12 * age + eps_bmi
    
    # 3. Binary Hypertension (hyp): logit link
    logit_p = -3.2 + 0.045 * age + 0.05 * bmi_true
    prob_hyp = 1.0 / (1.0 + np.exp(-logit_p))
    hyp = (np.random.uniform(0, 1, size=n) < prob_hyp).astype(int)
    
    # 4. Continuous Cholesterol (chl): chl = 145.0 + 0.75 * age + 1.1 * bmi + 14.0 * hyp + eps
    eps_chl = np.random.normal(0, 12.0, size=n)
    chl_true = 145.0 + 0.75 * age + 1.1 * bmi_true + 14.0 * hyp + eps_chl
    
    # 5. Continuous Income: income = 25.0 + 0.55 * age + eps
    eps_inc = np.random.normal(0, 8.0, size=n)
    income_true = 25.0 + 0.55 * age + eps_inc

    # Missingness Mechanisms:
    # A) BMI: MCAR (18% missing)
    mask_bmi_na = np.random.uniform(0, 1, size=n) < 0.18
    
    # B) CHL: MAR (Missing at Random depending on age and hyp: higher missingness if older/hyp)
    mar_prob = 0.10 + 0.20 * (age > 45) + 0.15 * (hyp == 1)
    mask_chl_na = np.random.uniform(0, 1, size=n) < mar_prob
    
    # C) Income: MCAR (12% missing)
    mask_inc_na = np.random.uniform(0, 1, size=n) < 0.12

    # Assemble DataFrame
    df = pd.DataFrame({
        "age": age,
        "bmi": np.round(np.where(mask_bmi_na, np.nan, bmi_true), 2),
        "hyp": hyp,
        "chl": np.round(np.where(mask_chl_na, np.nan, chl_true), 2),
        "income": np.round(np.where(mask_inc_na, np.nan, income_true), 2)
    })
    
    return df

if __name__ == "__main__":
    out_dir = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "data"))
    os.makedirs(out_dir, exist_ok=True)
    out_csv = os.path.join(out_dir, "impute_benchmark_data.csv")
    
    df = generate_benchmark_data(n=500, seed=42)
    df.to_csv(out_csv, index=False, na_rep="NA")
    print(f"[✓] Generated benchmark dataset: {out_csv}")
    print(f"    Rows: {len(df)}, Missing counts:\n{df.isna().sum()}")
