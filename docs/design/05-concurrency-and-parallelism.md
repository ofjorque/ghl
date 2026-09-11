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

### 4.1 Filosofía de Diseño y Análisis Comparativo Multilenguaje

Para diseñar la aceleración por hardware en **GHL**, se realizó un estudio comparativo exhaustivo de las fortalezas y debilidades críticas en **R, Python, Julia y Rust**:

| Ecosistema | Lo que hace BIEN (Adoptado en GHL) | Lo que hace MAL (Evitado en GHL) |
|---|---|---|
| **R** (`torch`, `gpuR`, `ArrayFire`) | - Sintaxis declarativa integrada con pipelines `\|>` y operadores familiares. | - **Crash fatal de sesión en OOM**: si la VRAM se satura, el proceso de R aborta abruptamente.<br>- **Sobrecarga de copias host-device**: el GC de R no sincroniza con los búferes de la GPU, provocando copias involuntarias y fragmentación de memoria.<br>- Dependencia de bindings pesados y difíciles de configurar en entornos heterogéneos. |
| **Python** (`PyTorch`, `JAX`, `CuPy`) | - API orientada a objetos sumamente ergonómica (`.to(device)`).<br>- Streams asíncronos para solapar transferencias DMA y cálculo de kernels.<br>- Fusión de operaciones y compilación JIT (Triton, JAX XLA). | - **Sincronizaciones invisibles / Stalls**: llamadas escalares (`.item()`, `print`) detienen toda la cola de la GPU silenciosamente.<br>- **Pesadez extrema y Vendor Lock-in**: paquetes de CUDA de 2 a 4 GB; Metal (macOS) y Vulkan quedan relegados a soporte secundario.<br>- **El GIL frena el despacho concurrente**: múltiples hilos de CPU no pueden enviar comandos a la GPU sin la complejidad de multi-processing.<br>- Fugas por GC diferido: exige al usuario invocar manualmente `torch.cuda.empty_cache()`. |
| **Julia** (`CUDA.jl`, `KernelAbstractions.jl`) | - **Paridad sintáctica total vía Multiple Dispatch**: el mismo código genérico `f(A)` corre idéntico en CPU (`Array`) y GPU (`CuArray`).<br>- **Escritura de kernels en el mismo lenguaje**: no exige programar en C++/CUDA aparte.<br>- Abstracción agnóstica de hardware con `KernelAbstractions.jl`. | - **Latencia de arranque brutal (TTFX)**: compilar kernels de Julia a PTX/SPIR-V en la primera invocación toma entre 10 y 45 segundos.<br>- Descarga masiva de artefactos y dependencias binarias externas.<br>- El recolector de basura mark-and-sweep de Julia genera latencias impredecibles en el reciclaje de descriptores de GPU. |
| **Rust** (`wgpu`, `burn`, `candle`, `cudarc`) | - **Gestión RAII determinista (Zero-GC)**: los búferes de GPU se liberan exactamente en el microsegundo en que el contador de referencias llega a 0 (`Drop`). Cero necesidad de `empty_cache()`.<br>- **Portabilidad universal con `wgpu`**: opera sin drivers propietarios en Windows (DirectX 12/Vulkan), Linux (Vulkan) y macOS (Metal) con binarios de pocos megabytes.<br>- Seguridad frente a data-races en transferencias de búferes. | - **Riesgo de bloatware**: frameworks masivos como `burn` o `candle` arrastran más de 100 crates ajenos al cómputo numérico (tokenizers, safetensors) inflando el tiempo de compilación y el binario.<br>- **Latencia de compilación WGSL en runtime**: compilar texto WGSL en caliente causa micro-stalls si no hay pipelines pre-compilados y cacheados. |

---

### 4.2 Alineación Rigurosa con la Pila de Crates de GHL (RFC 08)

El diseño de `std::gpu` se acopla de manera limpia, directa y sin sobreingeniería con la infraestructura existente de GHL:

1. **Interoperabilidad Zero-Copy con `faer` (Álgebra Lineal en CPU):**
   - Las matrices en CPU de GHL se gestionan mediante `faer::Mat<f64>` (almacenamiento contiguo plano en memoria).
   - Usando `bytemuck::cast_slice(mat.as_slice())`, la memoria se transfiere directamente al búfer de staging de la GPU sin buffers temporales ni reempaquetado.
   - **Estrategia Híbrida Inteligente:** Para matrices pequeñas o medianas ($N < 1000$), GHL prioriza `faer` en CPU con `rayon` (evitando la latencia de transferencia al bus PCIe); para productos matriciales masivos ($N \ge 2000$) y covarianzas de alta dimensión, despacha a la GPU.
2. **Interoperabilidad con `polars` / Apache Arrow (DataFrames):**
   - Las columnas de `polars` almacenan arrays primitivos contiguos en memoria Arrow (alineados a 64 bytes).
   - La transferencia a un `GpuVector` se realiza en un único comando DMA directo desde el buffer Arrow subyacente.
3. **Despacho desde Código Nativo Cranelift (JIT / AOT):**
   - Cranelift genera código nativo de CPU (x86_64 / aarch64) para scripts y funciones; no compila shaders de GPU.
   - Las operaciones de GPU se despachan a través de funciones del runtime de Rust (`extern "C" fn __ghl_gpu_...`), preservando el arranque ultrarrápido (< 15 ms) sin arrastrar compiladores de shaders pesados en el frontend.
4. **Concurrencia Multihilo Segura con `rayon`:**
   - La cola de comandos `wgpu::Queue` es `Send + Sync`. Múltiples hilos de trabajo de Rayon pueden codificar y encolar comandos de GPU de forma concurrente y segura sin cuellos de botella de tipo GIL.
5. **Prevención de Bloatware y Latencia de Shaders:**
   - **Crate Núcleo Portátil: `wgpu = "24.0"` + `bytemuck = "1.21"`:** Rust puro, soporte nativo de DirectX 12, Metal y Vulkan, manteniendo el binario ligero (< 20 MB según Suite 04).
   - **Caché de Pipelines Pre-compilados (`ComputePipelineCache`):** Para evitar la penalización de TTFX observada en Julia, los shaders WGSL para multiplicación matricial (GEMM), reducciones estadísticas (`sum`, `mean`, `var`), covarianza y PRNG Philox se pre-inicializan y cachean al arrancar el runtime.
   - **Backend Opcional `cuda` vía `cudarc`:** Carga dinámica opcional de `nvcuda.dll` / `libcuda.so`. Si el sistema cuenta con hardware NVIDIA y drivers CUDA, se habilita aceleración cuBLAS; de lo contrario, el sistema opera con `wgpu` sin fallos de dependencias ausentes.

---

### 4.3 Gestión Determinista de Memoria (RAII + CoW sin Tracing GC)

En contraste con Python (donde los tensores de GPU huérfanos esperan la recolección de basura del host obligando a `torch.cuda.empty_cache()`) y R (donde los desbordamientos de memoria abortan el proceso), GHL aplica su modelo estricto de **ARC + Copy-on-Write**:

- **Liberación Inmediata (Instant Drop):** Cuando un `GpuMatrix` o `GpuVector` sale de ámbito en el host y su contador de referencias (`ref_count`) cae a 0, el descriptor de GPU y su memoria en VRAM se desasignan al instante.
- **Mutación In-Place Garantizada:** Si una operación de GPU se aplica sobre un tensor con `ref_count == 1`, el kernel reutiliza el búfer de destino en VRAM directamente sin allocaciones adicionales.

---

### 4.4 Especificación de la API en Código GHL (`std::gpu`)

#### 4.4.1 Inicialización de Dispositivos
```ghl
use std::gpu::{Device, GpuMatrix, GpuVector};

// Selección automática del adaptador GPU de mayor rendimiento disponible
let dev = Device::default_gpu()?;
println("Accelerating on: {}", dev.name());
```

#### 4.4.2 Creación, Transferencia y Álgebra Lineal Acelerada
```ghl
// Matriz en host generada con faer
let x_cpu = Matrix::random_normal(10_000, 500);

// Transferencia explícita DMA a VRAM
let x_gpu = x_cpu.to_gpu(&dev)?;

// Multiplicación matricial masiva y simétrica en GPU (X^T * X)
let xtx_gpu = x_gpu.transpose().matmul(&x_gpu)?;

// Descomposición de Cholesky directamente en GPU
let l_gpu = xtx_gpu.cholesky()?;

// Descarga explícita al host para visualización o persistencia
let l_cpu = l_gpu.to_cpu()?;
```

#### 4.4.3 Reducciones y Estadísticos Descriptivos en VRAM
```ghl
// Reducciones columnares y escalares directamente en GPU sin transferencias
let col_means = x_gpu.mean_axis(0)?; // Retorna GpuVector
let grand_sum = x_gpu.sum()?;        // Retorna f64 descargado
```

#### 4.4.4 Simulación Monte Carlo y Bootstrap en GPU
```ghl
use std::gpu::random::{PhiloxRng, Distribution};

// Generar 100,000,000 números pseudoaleatorios directamente en VRAM
let mut gpu_rng = PhiloxRng::seed(42);
let samples_gpu = gpu_rng.sample_normal(&dev, 100_000_000, 0.0, 1.0)?;

// Estimadores paralelos directos en la aceleradora:
let mu_hat = samples_gpu.mean()?;
let sigma2_hat = samples_gpu.variance()?;
```

---

### 4.5 Taxonomía de Errores y Diagnósticos Empáticos

En cumplimiento de `AGENT.md`, los fallos de GPU se clasifican rigurosamente:

- **`[Compute Error] [C0601]`**: Errores de agotamiento de memoria en VRAM:
  ```
  [C0601] (ノ°□°)ノ Out of GPU device memory!
  Requested allocation of 2048.0 MB exceeds remaining device memory (512.5 MB available on NVIDIA GeForce RTX 4090).
  = Hint: Reduce batch size or process in chunks using streaming iterators.
  ```
- **`[Compute Error] [C0602]`**: Ausencia de hardware compatible o fallo de inicialización:
  ```
  [C0602] (｡•́︿•̀｡) No compatible GPU adapter found.
  Could not initialize a DirectX 12, Metal, or Vulkan compute backend.
  = Hint: Ensure graphics drivers are updated or use CPU fallback.
  ```
- **`[Statistical Error] [S0601]`**: Invalidez matemática durante el cómputo en GPU:
  ```
  [S0601] ฅ(ﾐΦ ﻌ Φﾐ)ฅ GPU covariance matrix is not positive semi-definite!
  Cholesky factorization failed on device at pivot index 42.
  = Hint: Verify regularization or use ridge adjustment before factorizing.
  ```


