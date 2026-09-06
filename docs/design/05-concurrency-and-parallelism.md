# RFC 05: Concurrencia, Paralelismo y Aceleración en GHL

- **Estado:** Propuesto
- **Área:** Runtime / Paralelismo / GPU
- **Audiencia:** Diseñadores del runtime, desarrolladores de algoritmos paralelos

---

## 1. Paralelismo de Memoria Compartida sin GIL

A diferencia de Python (que restringe los hilos mediante el Global Interpreter Lock) y R (que recurre a procesos bifurcados duplicando memoria RAM), **GHL** implementa un modelo de concurrencia nativo de memoria compartida, seguro frente a condiciones de carrera de datos (*data-race freedom*).

---

## 2. Iteradores Paralelos con Robo de Trabajo (Work-Stealing)

GHL incluye una biblioteca de paralelismo de datos integrada en el runtime (inspirada en Rayon):

```lang
use std::concurrency::ParallelIterator;

// Distribute 1,000,000 independent Monte Carlo simulations across all cores:
let results: Vector<f64> = (0..1_000_000)
    .par_iter()
    .map(seed_offset => {
        let mut worker_rng = PRNG::seed(42 + seed_offset);
        run_stochastic_simulation(&mut worker_rng)
    })
    .collect();
```

---

## 3. Paralelismo Automático en DataFrames

Las operaciones columnares dividen sus cargas de trabajo automáticamente:

```lang
let grouped = massive_df
    .group_by(["country", "product_category"])
    .parallel()
    .agg([
        col("revenue").sum().as("total_sales"),
        col("discount").mean().as("avg_discount")
    ]);
```

---

## 4. Cómputo Acelerado en GPU (`std::gpu`)

Para tensores masivos y matrices de covarianza de alta dimensión:

```lang
use std::gpu::{Device, GpuTensor};

let gpu = Device::default_gpu()?;
let tensor_on_device = GpuTensor::copy_from_host(&host_matrix, &gpu)?;

// Massively parallel matrix multiplication on hardware accelerator
let covariance = tensor_on_device.matmul(&tensor_on_device.transpose())?;
```

