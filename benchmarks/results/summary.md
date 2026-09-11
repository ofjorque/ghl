# Resumen Ejecutivo de Benchmarks Empíricos (Fase 10)
## GHL vs R, Python y Julia — incluye variantes idiomáticas optimizadas

Fecha de ejecución: 10 de Septiembre de 2026
Herramienta de medición: `hyperfine`, **30 iteraciones** por combinación lenguaje/prueba tras warmup (cumple el mínimo de `methodology.md` §2.A)
Entorno de prueba: Windows 11 x86_64, desktop de escritorio sin aislamiento de hardware (ver `methodology.md` §4 — el protocolo de `taskset`/gobernador de CPU/Turbo Boost de la Sección 3 es específico de Linux y no se aplicó aquí)

### Versiones de Software
- **GHL**: `0.1.0` (Compilador release optimizado con JIT/AOT **Cranelift** — no LLVM — y runtime faer/polars-core)
- **Python**: `3.14.6` (NumPy `2.5.3`, Pandas `3.0.5`, **Polars `1.44.2`**, **Numba `0.67.0`**)
- **R**: `4.6.1` (Rscript x86_64-w64-mingw32, BLAS de referencia — no OpenBLAS/MKL —, **data.table `1.18.4`**)
- **Julia**: `1.12` (x86_64-w64-mingw32 con OpenBLAS, **DataFrames.jl `1.8.2`**, **CSV.jl `0.10.17`**)

> **Por qué se agregaron variantes "optimizadas":** la primera versión de este documento (generada con asistencia de Gemini) comparaba a GHL contra Pandas base, `aggregate()` de R base, y un parser CSV artesanal en Julia — pese a que `methodology.md` §1.1 promete explícitamente evaluar `data.table`/`collapse` en R y `Polars`/`Numba`/`PyTorch` en Python. Esa comparación inflaba la ventaja de GHL en la Suite 02 (motor Polars-Rust multihilo vs. bibliotecas de un solo hilo). Esta revisión mantiene ambas variantes (base y optimizada) para no ocultar ningún resultado.

---

## 1. Tabla Comparativa de Rendimiento Empírico

Media aritmética $\pm$ desviación típica ($\sigma$), n=30 por celda.

| Suite / Carga de Trabajo | GHL | Mejor alternativa externa | Speedup GHL |
| :--- | :---: | :---: | :---: |
| **Suite 04: Startup / TTFX** *(Hello World)* | **5.3 ms** $\pm$ 0.5 ms (AOT) / 10.5 ms $\pm$ 0.4 ms (Interp) | Python 3.14: 35.2 ms $\pm$ 2.4 ms | **6.63x vs Python** (21.0x vs R, 31.1x vs Julia) |
| **Suite 01: Math / SIMD** *(Dot Product $10^7$ `f64`)* | **54.7 ms** $\pm$ 2.1 ms | NumPy: 224.3 ms $\pm$ 6.4 ms | **4.10x vs NumPy** (5.70x vs R, 7.55x vs Julia) |
| **Suite 02: DataFrames** *(1M filas CSV + Filter + GroupBy + Agg)* | 643.0 ms $\pm$ 15.4 ms | **Python (Polars): 310.5 ms $\pm$ 8.6 ms** | **GHL pierde: 0.48x** (Polars-Python es 2.07x más rápido que GHL) |
| **Suite 03: Modelado Estadístico** *(Gibbs Sampler 100 iter / 3k obs)* | **21.4 ms** $\pm$ 0.6 ms | R (Base): 138.1 ms $\pm$ 2.0 ms | **6.45x vs R** (18.6x vs NumPy, 26.1x vs Julia) |

**El titular honesto de esta fase no es "GHL le gana a todo": en Suite 02, con las librerías idiomáticas óptimas de cada ecosistema, GHL queda 3° lugar, detrás de Polars-Python y data.table-R.** En Suite 01, 03 y 04 sí es el más rápido con margen amplio, incluso contra las variantes optimizadas.

---

## 2. Análisis Detallado por Suite

### Suite 04: Tiempo de Arranque y Time-To-First-X (TTFX)
| Variante | Tiempo |
| :--- | :---: |
| **GHL AOT Binary** | **5.3 ms** $\pm$ 0.5 ms |
| **GHL Interpreted** | **10.5 ms** $\pm$ 0.4 ms |
| Python 3.14 | 35.2 ms $\pm$ 2.4 ms |
| R 4.6.1 | 111.7 ms $\pm$ 3.1 ms |
| Julia | 165.1 ms $\pm$ 2.1 ms |

GHL arranca más rápido en este micro-benchmark porque no carga runtime alguno (Python/R interpretan bytecode, Julia debe JIT-compilar `using LinearAlgebra`/`using Random` incluso para un "hello world"). Nota de honestidad: a esta escala (5 ms) `hyperfine` advierte que su propia calibración de arranque de shell no es mucho más precisa que la medición misma — el ratio de 31x vs Julia es real pero su margen de error relativo es mayor que en las otras suites.

Julia, en particular, puede tardar mucho más que estos 165 ms cuando el script real carga paquetes pesados (`DataFrames.jl`, `Plots.jl`) — ver Suite 02 más abajo, donde `using DataFrames, CSV` por sí solo añade varios segundos.

---

### Suite 01: Álgebra Vectorial y SIMD (Producto Escalar $10^7$ Flotantes)
| Variante | Tiempo |
| :--- | :---: |
| **GHL** | **54.7 ms** $\pm$ 2.1 ms |
| Python (NumPy) | 224.3 ms $\pm$ 6.4 ms |
| R (Base) | 311.5 ms $\pm$ 7.5 ms |
| Julia | 412.8 ms $\pm$ 5.4 ms |

Esta comparación ya era justa desde la primera versión: `np.dot` y `LinearAlgebra.dot` de Julia despachan a BLAS `ddot` para vectores `Float64`, así que ambos ya corren su ruta "óptima" de fábrica. **Caveat de honestidad:** NumPy y Julia traen un OpenBLAS multihilo empaquetado por defecto; esta instalación de R usa BLAS de referencia (single-thread, no vectorizado a mano) — no se instaló OpenBLAS/MKL para R porque requeriría recompilar R, fuera de alcance de este documento. La ventaja de GHL sobre R en este renglón está en parte explicada por esa asimetría de BLAS, no solo por el compilador de GHL.

---

### Suite 02: Ingestión y Manipulación de DataFrames (1M Filas CSV)
Filtrado por predicado categórico (`status == "OK"`), agrupación de alta cardinalidad (`category`), 4 reducciones (`mean(value_b)`, `mean(value_c)`, `sum(value_a)`, `count()`).

| Variante | Tiempo | vs GHL |
| :--- | :---: | :---: |
| **Python (Polars)** | **310.5 ms** $\pm$ 8.6 ms | **2.07x más rápido** |
| R (data.table) | 473.2 ms $\pm$ 5.9 ms | 1.36x más rápido |
| **GHL (Polars-core, Rust)** | **643.0 ms** $\pm$ 15.4 ms | — |
| Python (Pandas) | 1,219.7 ms $\pm$ 15.9 ms | 1.90x más lento |
| Julia (parser CSV artesanal) | 1,461.2 ms $\pm$ 11.9 ms | 2.27x más lento |
| R (Base `read.csv`+`aggregate`) | 3,611.5 ms $\pm$ 73.9 ms | 5.62x más lento |
| Julia (DataFrames.jl + CSV.jl) | 5,699.3 ms $\pm$ 32.6 ms | 8.86x más lento |

**Este es el resultado más importante de esta revisión.** Con las librerías idiomáticas correctas:
- **Polars-Python es 2.07x más rápido que GHL**, pese a que ambos envuelven esencialmente el mismo motor Rust (`polars-core`). La diferencia probable está en el binding/overhead de la capa `ghl-runtime` alrededor de polars y en que la CLI de Python-Polars puede estar mejor afinada para este patrón de lectura de CSV que el camino usado en `crates/ghl-runtime/src/io.rs`. **Esto contradice directamente la afirmación original del TODO.md ("GHL supera a Pandas por 1.86x") como si fuera la conclusión relevante — la conclusión correcta es que GHL pierde contra el estado del arte real de su propia categoría (Polars).**
- **`data.table` en R también supera a GHL** (1.36x), desmintiendo la caracterización de R como "el lento" en DataFrames cuando se usa la herramienta correcta en vez de `aggregate()` base.
- GHL sigue siendo más rápido que Pandas, R base, y ambas variantes de Julia.
- `DataFrames.jl + CSV.jl` es, sorprendentemente, la opción **más lenta de las siete**: cargar esos dos paquetes (`using DataFrames, CSV`) cuesta ~5 segundos de latencia de arranque en un proceso Julia nuevo, incluso antes de leer una fila — un costo real y bien documentado en el ecosistema Julia ("TTFX"), que el parser artesanal (sin dependencias) evita por completo.

---

### Suite 03: Modelado Estadístico y MCMC (Gibbs Sampler Jerárquico)
100 iteraciones, 3.000 observaciones en 3 clusters, priors normales + hiper-prior Gamma.

| Variante | Tiempo | vs GHL |
| :--- | :---: | :---: |
| **GHL** | **21.4 ms** $\pm$ 0.6 ms | — |
| R (Base) | 138.1 ms $\pm$ 2.0 ms | 6.45x más lento |
| Python (NumPy, bucle puro) | 397.9 ms $\pm$ 7.9 ms | 18.6x más lento |
| Julia (@inbounds/tipado) | 553.1 ms $\pm$ 2.6 ms | 25.8x más lento |
| Julia (baseline) | 559.5 ms $\pm$ 3.5 ms | 26.1x más lento |
| Python (Numba JIT) | 786.9 ms $\pm$ 8.9 ms | 36.8x más lento |

Aquí GHL gana claramente incluso contra las variantes optimizadas — este es el resultado más sólido de los cuatro. Dos hallazgos honestos adicionales:
- **Numba hace más lento a Python, no más rápido**, para esta carga de trabajo (786.9 ms vs 397.9 ms de NumPy puro). El costo fijo de importar `numba`/`llvmlite` y compilar la función JIT en cada proceso nuevo supera por completo el tiempo de cómputo que ahorra en un bucle de sólo 100×3 iteraciones. Numba brilla en bucles largos reutilizados dentro de un mismo proceso de larga vida, no en scripts de un solo disparo — este benchmark, al medir el proceso completo (arranque incluido), lo penaliza correctamente.
- **`@inbounds` + tipado explícito en Julia no cambia nada relevante** (559.5 ms → 553.1 ms, ~1% de diferencia, dentro del margen de ruido). El cuello de botella de este algoritmo es la asignación repetida de arrays por el enmascarado booleano (`groups .== j`) en cada iteración, no el chequeo de límites — `@inbounds` no ataca esa causa.

---

## 3. Verificación de Reproducibilidad y Corrección

- Todos los scripts, resultados brutos (`.json`/`.md` de `hyperfine`) y el harness (`run_benchmarks.ps1`, v2 con 30 corridas) están en `benchmarks/scripts/` y `benchmarks/results/raw/`.
- **Corrección funcional verificada, no solo velocidad:** la salida de `bench_df.gh` (GHL) coincide cifra por cifra con Pandas/Polars/data.table/DataFrames.jl para las 5 categorías. El Gibbs sampler produce medias posteriores estadísticamente consistentes (~3.01, ~8.05, −3.99) en las 8 variantes de los 4 lenguajes — ninguna implementación "hace menos trabajo" para parecer más rápida.
- **Lo que NO se cumplió de `methodology.md`:** aislamiento de hardware (Sección 3, específico de Linux) y medición de Peak RSS / pausas de GC (Sección 2.C/D) — no instrumentados en esta ronda.
