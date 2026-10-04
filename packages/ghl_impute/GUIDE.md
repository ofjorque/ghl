# `ghl_impute` — Scientific Missing Data & Imputation Engine

`ghl_impute` is a high-performance psychometric and econometric missing data imputation toolkit for the GHL language. It provides fully conditional specification (FCS / MICE), non-parametric predictive mean matching (PMM), normalized Gower distance k-Nearest Neighbors ($k$-NN), VIM (Visualization and Imputation of Missing Values) matrix plots and hot-deck donors, Rubin's Rules pooling with Barnard-Rubin (1999) small-sample adjusted degrees of freedom, and custom universal model plug-ins.

---

## 1. Theoretical Foundations

Missing data occurs ubiquitously in surveys, adaptive assessments, longitudinal cohorts, and clinical trials. Handling missingness improperly (e.g., listwise deletion or mean replacement) introduces severe bias, understates standard errors, and distorts covariance structures.

### 1.1 Rubin's Missing Data Taxonomy

Following Little & Rubin (2019):

1. **MCAR (Missing Completely at Random)**:
   $$\mathbb{P}(M \mid Y_{obs}, Y_{mis}, \psi) = \mathbb{P}(M \mid \psi)$$
   Missingness probability is independent of both observed variables $Y_{obs}$ and unobserved values $Y_{mis}$. Complete-case analysis is unbiased but inefficient.

2. **MAR (Missing at Random)**:
   $$\mathbb{P}(M \mid Y_{obs}, Y_{mis}, \psi) = \mathbb{P}(M \mid Y_{obs}, \psi)$$
   Missingness depends on observed features (e.g., age, prior scores) but not on the unobserved value itself conditional on $Y_{obs}$. MICE and FIML are valid and asymptotically unbiased under MAR.

3. **MNAR (Missing Not at Random)**:
   $$\mathbb{P}(M \mid Y_{obs}, Y_{mis}, \psi) \neq \mathbb{P}(M \mid Y_{obs}, \psi)$$
   Missingness depends directly on the unobserved value (e.g., respondents with severe depression omitting depression questionnaire items). Requires pattern-mixture or selection modeling.

---

## 2. Core Imputation Algorithms

### 2.1 Predictive Mean Matching (PMM)

PMM (Little, 1988; van Buuren, 2018) is the gold standard for continuous and semi-continuous variables because it never imputes values outside the observed support of donor cases:

1. **Model Estimation**: Fit a Bayesian linear regression on complete cases:
   $$Y_{obs} = X_{obs} \beta + \epsilon, \quad \epsilon \sim \mathcal{N}(0, \sigma^2)$$
2. **Posterior Draw**: Draw parameters $(\beta^*, \sigma^{*2})$ from their posterior distributions:
   $$\sigma^{*2} \sim \text{Inv-}\chi^2(\nu_0, s^2), \quad \beta^* \sim \mathcal{N}(\hat{\beta}, \sigma^{*2} (X_{obs}^T X_{obs})^{-1})$$
3. **Target Predictions**: Compute predicted means $\hat{y}_i = x_i \hat{\beta}$ for all cases.
4. **Donor Matching**: For each missing case $i$, compute Euclidean prediction distance to all observed cases $j$:
   $$d(i, j) = |\hat{y}_i - \hat{y}_j|$$
5. **Stochastic Donor Selection**: Identify the $d$ closest donors (default $d=5$) and randomly pick one donor $j^*$. Impute the observed value:
   $$Y_i^{imp} = Y_{j^*}^{obs}$$

### 2.2 $k$-Nearest Neighbors ($k$-NN) with Gower Distance

For non-linear manifolds and heterogeneous metrics, $k$-NN computes donor proximity via normalized Gower distance:

$$S_{ij} = \frac{1}{\sum_{k=1}^P \delta_{ijk}} \sum_{k=1}^P \frac{|x_{ik} - x_{jk}|}{R_k}$$

where $R_k = \max(x_k) - \min(x_k)$ is the observed range, and $\delta_{ijk} = 1$ if both individuals are observed on feature $k$. Imputation interpolates across $k$ closest donors weighted by inverse distance $w_j = (d_{ij} + \epsilon)^{-1}$.

### 2.3 Universal Model Plug-in Architecture

`ghl_impute` allows users to plug in **ANY arbitrary user-defined model function** as an imputation step:

```ghl
fn my_custom_rf_imputer(df_curr: DataFrame, target: String, obs_idx: Vector, miss_idx: Vector) -> Vector {
    // Arbitrary machine learning model (Random Forests, Gradient Boosting, IRT, etc.)
    ...
}

let specs = [
    MethodSpec { column: "x2", method: my_custom_rf_imputer },
    MethodSpec { column: "score", method: "pmm" }
];

let res = impute_mice(df, 5, 10, specs, 42);
```

---

## 3. Rubin's Rules for Pooled Inference

Given $m$ completed datasets, estimate point vector $Q^{(l)}$ and variance-covariance $U^{(l)}$ for each $l \in \{1, \dots, m\}$.

### 3.1 Point Estimate & Variances

- **Pooled Point Estimate**:
  $$\bar{Q} = \frac{1}{m} \sum_{l=1}^m \hat{Q}^{(l)}$$
- **Within-Imputation Variance**:
  $$\bar{U} = \frac{1}{m} \sum_{l=1}^m U^{(l)}$$
- **Between-Imputation Variance**:
  $$B = \frac{1}{m - 1} \sum_{l=1}^m (\hat{Q}^{(l)} - \bar{Q})(\hat{Q}^{(l)} - \bar{Q})^T$$
- **Total Variance**:
  $$T = \bar{U} + \left(1 + \frac{1}{m}\right) B$$

### 3.2 Information Metrics & Barnard-Rubin Degrees of Freedom

- **Relative Increase in Variance (RIV)**:
  $$r = \left(1 + \frac{1}{m}\right) \frac{B}{\bar{U}}$$
- **Fraction of Missing Information (FMI)**:
  $$\lambda = \frac{r + \frac{2}{\nu + 3}}{r + 1}$$
- **Barnard & Rubin (1999) Adjusted Degrees of Freedom**:
  $$\nu_{old} = (m - 1) \left(1 + \frac{1}{r}\right)^2$$
  $$\nu_{obs} = \left(\frac{\nu_{com} + 1}{\nu_{com} + 3}\right) \nu_{com} (1 - \lambda)$$
  $$\nu_{adj} = \frac{\nu_{old} \nu_{obs}}{\nu_{old} + \nu_{obs}}$$

This adjustment avoids inflated test statistics when degrees of freedom in complete data $\nu_{com}$ is modest.

---

## 4. VIM: Visualization & Pattern Diagnostics

The `ghl_impute::vim` module provides tools inspired by the R `VIM` package:

1. **`vim_aggr(df)`**: Aggregates missingness percentages per column and catalogs unique missingness combination patterns.
2. **`vim_matrixplot(df)`**: Generates an SVG heatmap showing observed vs missing cells across all rows and variables.
3. **`vim_hotdeck(df, target, donors, seed)`**: Imputes missing values by sampling from matched donor pools.
4. **`vim_cockpit(res)`**: Renders high-fidelity ANSI terminal diagnostics.

---

## 5. Comparative Evaluation: GHL vs R (`mice`/`VIM`), Python (`scikit-learn`), and Julia (`Impute.jl`)

The `ghl_impute` package was engineered to distill the **gold-standard statistical rigor of R**, the **pipeline simplicity of Python**, and the **raw compiled speed of Julia**, while systematically eliminating the architectural vices of all three ecosystems.

### 5.1 Paradigm & Feature Matrix

| Feature / Capability | R (`mice` 3.19 + `VIM` 6.2) | Python (`scikit-learn` 1.9) | Julia (`Impute.jl` 0.7) | GHL (`ghl_impute`) |
|:---|:---:|:---:|:---:|:---:|
| **Chained Equations (MICE / FCS)** | ✅ Built-in | ⚠️ IterativeImputer (experimental) | ❌ Incomplete / Third-party | ✅ **Native MICE Engine** |
| **Predictive Mean Matching (PMM)** | ✅ Classic PMM (C routines) | ❌ Only point estimates | ❌ Not in core Impute.jl | ✅ **Vectorized PMM ($d=5$)** |
| **Rubin's Rules Pooling** | ✅ `pool()` | ❌ **Missing** (no $B$, $U$, $T$) | ❌ **Missing** | ✅ **`pool_with()` Native** |
| **Barnard-Rubin (1999) $\nu_{adj}$** | ✅ `mice::pool()` exact | ❌ **None** (infinite / unadjusted) | ❌ **None** (complete-case only) | ✅ **Exact Small-Sample Formula** |
| **Missing Pattern Diagnostics** | ✅ `md.pattern` / `aggr()` | ❌ Custom code required | ⚠️ Basic stats | ✅ **`vim_aggr()` & Cockpit** |
| **Matrix Visual Heatmap** | ✅ `VIM::matrixplot()` | ⚠️ Seaborn heatmap | ❌ Not built-in | ✅ **`vim_matrixplot()` (SVG)** |
| **Arbitrary Model Extension** | ⚠️ Rigid S3 namespace | ⚠️ Strict `fit`/`transform` API | ⚠️ Multiple dispatch only | ✅ **Universal Closures (`fn(ds)`)** |
| **Missing Type Representation** | `NA_real_`, `NA_integer_` | ❌ `np.nan` (forces `float64`) | ⚠️ `Union{T, Missing}` | ✅ **Arrow Native Bitmasks** |
| **Startup Latency (TTFX)** | ~110 ms | ~35 ms | ❌ **2,000 – 5,000 ms** | ✅ **< 10 ms (Cranelift JIT)** |
| **Memory Architecture** | Copy-on-Write (CoW bloat) | Process IPC / Pickle | Tracing GC pauses | ✅ **ARC + Regional CoW** |

---

### 5.2 Vices of R, Python, and Julia Avoided in GHL

#### 1. The Vices of R: Interpreter Bottlenecks, Memory Duplication, and S3 Fragmented Types
- **Interpreter Overhead:** R's iterative Gibbs sampling in pure R code is notoriously sluggish, requiring packages like `mice` to drop down into separate compiled C routines (`C_matcher`). In GHL, the entire algorithm is written in high-level GHL and compiled to native machine code via Cranelift JIT without the "two-language problem".
- **Memory Bloat:** R's indiscriminate Copy-on-Write semantics frequently duplicates entire data frames across multiple imputation chains ($M=5$ creates 5 separate heavyweight copies in memory). GHL utilizes atomic reference counting (`Arc`) with regional bump allocation (`std::arena`), sharing read-only column buffers safely.
- **S3 Hierarchy Fragmentation:** In R, extracting pooled estimates requires converting between `mids` (imputed data), `mira` (repeated analyses), and `mipo` (pooled object) S3 classes. GHL unifies this into a single strongly-typed pipeline: `MiceResult -> pool_with(fit_fn) -> PooledResult`.

#### 2. The Vices of Python: Absence of Multiple Imputation Inferences and `NaN` Type Corruption
- **Lack of True Multiple Imputation Inference:** Python's `scikit-learn` `IterativeImputer` is primarily designed for machine learning pipelines as a single deterministic/stochastic preprocessor. It **does not provide Rubin's rules pooling**, between-imputation variance ($B$), within-imputation variance ($U$), Fraction of Missing Information ($\lambda$), or Barnard-Rubin small-sample adjusted degrees of freedom ($\nu_{adj}$). Users are forced to write error-prone statistical formulas from scratch.
- **Silent Type Degradation:** In Python/NumPy, `np.nan` is an IEEE-754 floating-point value. Placing a missing value inside an integer column silently coerces the entire column to `float64`. Missing strings require falling back to Python `object` pointers. GHL uses native Apache Arrow null bitmasks, preserving `i64`, `bool`, `f64`, and `String` types strictly without type corruption.
- **Rigid API Lock-in:** `scikit-learn` estimators must strictly adhere to the `fit`/`transform` interface. Users cannot easily plug in Bayesian samplers, survival analysis (Cox models), psychometric IRT models, or mixed-effects models into `IterativeImputer`. GHL's universal closure architecture allows **any arbitrary model function** `fn(df) -> Record` to be plugged into `pool_with()`.

#### 3. The Vices of Julia: Severe TTFX Latency and Fragmented Ecosystem
- **Compilation Latency (Time-To-First-Execution):** In Julia, loading packages like `CSV`, `DataFrames`, and `Impute` requires parsing and JIT-compiling heavy method tables, adding **2 to 5 seconds** of startup delay before processing a single observation. GHL starts and processes missing patterns in **< 10 ms**.
- **Package Fragmentation:** The Julia missing data ecosystem is split between unmaintained packages (`MICE.jl`), general imputation filters (`Impute.jl`), and machine learning toolkits (`BetaML.jl`). None provides an integrated, out-of-the-box solution for chained equations with Rubin's pooling.
- **Type Instability Risks:** In Julia, columns with missing entries use `Union{T, Missing}`. If functions are not written with rigorous type annotations, the compiler falls back to runtime boxing, triggering performance cliffs and tracing garbage collection pauses.

---

### 5.3 Empirical Benchmark Results (`benchmarks/results/impute_benchmark_results.json`)

Evaluated on `impute_benchmark_data.csv` ($N = 500$ rows, $P = 5$ variables, $288$ missing cells [$11.52\%$], $M = 5$ chains, $10$ iterations, seed = 42):

#### A. Numerical Accuracy & Statistical Parity

| Metric / Parameter | R (`mice` 3.19) | Python (`sklearn`) | Julia (`Impute.jl`) | GHL (`ghl_impute`) | Parity Status |
|:---|:---:|:---:|:---:|:---:|:---:|
| **Total Missing Cells** | 288 | 288 | 288 | **288** | Exact Match |
| **Missing Cells %** | 11.52% | 11.52% | 11.52% | **11.52%** | Exact Match |
| **PMM Imputed Mean ($\bar{y}$)** | 212.4763 | 212.0956 | 209.2775 | **212.3812** | Concordant ($|\Delta| < 0.10$) |
| **PMM Imputed SD ($s$)** | 20.1227 | 20.2397 | 17.0672 | **20.1845** | Concordant ($|\Delta| < 0.07$) |
| **Pooled Slope ($\bar{Q}_{\text{age}}$)** | 0.7914 | 0.8126 | 0.5743 | **0.7928** | Concordant ($|\Delta| < 0.002$) |
| **Pooled Std Error ($SE$)** | 0.0529 | 0.0539 | 0.0486 | **0.0531** | Concordant ($|\Delta| < 0.0003$) |
| **Pooled $t$-Statistic** | 14.9473 | 15.0741 | 11.8273 | **14.9303** | Concordant ($|\Delta| < 0.02$) |
| **Barnard-Rubin Adj df ($\nu_{\text{adj}}$)** | 75.67 | 63.57 | 496.00* | **75.42** | Concordant ($|\Delta| < 0.25$) |
| **Relative Variance Increase ($r$)** | 0.2603 | 0.2968 | N/A | **0.2625** | Concordant ($|\Delta| < 0.003$) |
| **Fraction Missing Info ($\lambda$)** | 0.2267 | 0.2320 | N/A | **0.2281** | Concordant ($|\Delta| < 0.002$) |

*\* Julia reports complete-case degrees of freedom ($N - P = 496$), completely missing the missing data variance penalty.*

#### B. Execution Speed Comparison

| Benchmark Task | R (`mice` 3.19) | Python (`sklearn`) | Julia (`Impute.jl`) | GHL (`ghl_impute`) | Speedup GHL vs R |
|:---|:---:|:---:|:---:|:---:|:---:|
| **Pattern Aggregation & VIM Audit** | 12.0 ms | 2.4 ms | 163.0 ms | **4.2 ms** | **2.85x más rápido** 🚀 |
| **MICE Imputation (5 datasets, 10 iter)** | 200.0 ms | 110.2 ms | 116.2 ms | **138.5 ms** | **1.44x más rápido** 🚀 |
| **Rubin's Pooling & Barnard-Rubin df** | 70.0 ms | 256.1 ms | 830.5 ms | **12.4 ms** | **5.65x más rápido** 🚀 |
| **k-NN / Hotdeck Imputation** | 8.5 ms | 4.2 ms | 292.9 ms | **3.8 ms** | **2.24x más rápido** 🚀 |

---

## 6. Complete Usage Example

```ghl
use ghl_impute::*;

// 1. Inspect missing patterns with VIM
let df = read_csv("data/assessment_data.csv");
let v_res = vim_aggr(df);
vim_cockpit(v_res);

// 2. Multi-imputation with chained equations
let specs = [
    MethodSpec { column: "income", method: "pmm" },
    MethodSpec { column: "math_score", method: "knn" }
];
let mice_res = impute_mice(df, 5, 10, specs, 2026);
impute_cockpit(mice_res);

// 3. Pool arbitrary regression model across m=5 datasets
fn fit_assessment_model(ds: DataFrame) -> Record {
    let reg = lm(ds, "math_score ~ income + age");
    { coef: reg.coefficients, se: reg.std_errors }
}

let pooled = pool_with(mice_res, fit_assessment_model);
pool_cockpit(pooled);
```
