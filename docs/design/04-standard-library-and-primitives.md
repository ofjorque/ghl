# RFC 04: Biblioteca Estándar y Primitivas Estadísticas de GHL

- **Estado:** Propuesto
- **Área:** Biblioteca Estándar (Stdlib) / API de Primitivas
- **Audiencia:** Desarrolladores de la biblioteca estándar, usuarios de GHL

---

## 1. Módulos de la Biblioteca Estándar

GHL está estructurado para ser simultáneamente un lenguaje de sistemas generalista y un entorno de análisis estadístico de vanguardia:

```
std/
├── core/         # Primitive types, memory management, asynchronous I/O, TCP/HTTP servers
├── math/         # Special functions (Gamma, Beta, Digamma, Erf), numerical root solvers
├── linalg/       # Vectors, matrices, tensors, decompositions (SVD, QR, Cholesky, LU)
├── dataframe/    # Native columnar table engine (Apache Arrow IPC compatible)
├── stats/        # Distributions, hypothesis tests, linear/generalized models, MCMC
└── autodiff/     # Native reverse and forward mode automatic differentiation
```

---

## 2. Álgebra Lineal (`std::linalg`)

```lang
use std::linalg::{Matrix, Vector};

let matrix_a = mat[
    4.0, 1.0;
    1.0, 3.0
];

let vector_b = [9.0, 7.0];

// Solve linear system A * x = b via backslash operator:
let solution_x = matrix_a \ vector_b;

// Common matrix decompositions
let (q_mat, r_mat) = matrix_a.qr()?;
let cholesky_l = matrix_a.cholesky()?;
let svd_decomp = matrix_a.svd()?;
```

---

## 3. DataFrames Columnares Nativos (`std::dataframe`)

El motor de datos de GHL utiliza buffers continuos de Apache Arrow en memoria:

```lang
use std::dataframe::{DataFrame, col};

// Read remote or local data smoothly (fetch pattern)
let experiment_df = DataFrame::read_parquet("s3://experiments/trial_01.parquet")?
    .filter(col("dose") > 0.0 && !is_na(col("outcome")))
    .group_by(["treatment_arm", "gender"])
    .agg([
        col("response").mean().as("mean_response"),
        col("response").std().as("sd_response"),
        col("patient_id").count().as("cohort_size")
    ]);
```

---

## 4. Distribuciones Probabilísticas y Modelado (`std::stats`)

Cada distribución implementa el trait común `Distribution`:

```lang
use std::stats::distributions::{Normal, Poisson, Gamma};
use std::stats::rng::PRNG;

let standard_normal = Normal::new(mean: 0.0, std_dev: 1.0)?;

let p_val = standard_normal.cdf(1.96);         // ~0.9750
let log_density = standard_normal.log_pdf(0.0); // -0.9189
let critical = standard_normal.quantile(0.95);  // ~1.6448

// Cross-platform bitwise deterministic PRNG (Xoshiro256++ / ChaCha20)
let mut rng = PRNG::seed(1337);
let random_sample = standard_normal.sample_n(50_000, &mut rng);
```

---

## 5. Diferenciación Automática (`std::autodiff`)

Para optimizadores numéricos, inferencia variacional y algoritmos Hamiltonian Monte Carlo:

```lang
use std::autodiff::grad;

// Custom objective function in pure GHL:
fn loss_function(weights: &Vector<f64>, x: &Matrix<f64>, y: &Vector<f64>) -> f64 {
    let residuals = (x * weights) - y;
    residuals.dot(&residuals) / (2.0 * y.len() as f64)
}

// Compute exact analytical gradient automatically:
let gradient_fn = grad(w => loss_function(&w, &features, &targets));
let current_grad = gradient_fn(&initial_weights);
```

---

## 6. Primitivas de Propósito General (`std::core`)

Para garantizar que GHL sea un lenguaje de programación completo:
- **Redes y Servidores Web:** Cliente y servidor HTTP nativo con soporte asíncrono para construir microservicios y APIs REST para servir modelos.
- **I/O y Archivos:** Lectura eficiente sin copias (`read_exact_at`, `mmap`).
- **Serialización:** Soporte nativo para JSON, CSV, Parquet y serialización binaria Arrow Flight.

