# Resumen Ejecutivo de Benchmarks Empíricos (Fase 10)
## GHL vs R, Python y Julia

Fecha de ejecución: 10 de Septiembre de 2026  
Herramienta de medición: `hyperfine` (warmups automatizados, medias estadísticas con desviación estándar $\sigma$)  
Entorno de prueba: Windows 11 x86_64, CPU Multi-Core, SSD NVMe  

### Versiones de Software
- **GHL**: `0.1.0` (Compilador release optimizado con JIT/AOT Cranelift y runtime faer/polars)
- **Python**: `3.14.6` (NumPy `2.5.3`, Pandas `3.0.5`, SciPy `1.18.1`)
- **R**: `4.6.1` (Rscript x86_64-w64-mingw32)
- **Julia**: `1.x` (Julia x86_64-w64-mingw32 con OpenBLAS)

---

## 1. Tabla Comparativa de Rendimiento Empírico

Los tiempos representan la media aritmética $\pm$ desviación típica ($\sigma$) medidos por `hyperfine`.

| Suite / Carga de Trabajo | GHL (AOT) | GHL (Runtime) | Python 3.14 | R 4.6.1 | Julia | Speedup GHL vs Más Rápido Externo |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Suite 04: Startup / TTFX** *(Hello World)* | **6.7 ms** $\pm$ 2.6 ms | 10.7 ms $\pm$ 0.3 ms | 34.8 ms $\pm$ 0.6 ms | 115.1 ms $\pm$ 1.8 ms | 166.4 ms $\pm$ 1.6 ms | **5.19x vs Python** (24.8x vs Julia) |
| **Suite 01: Math / SIMD** *(Dot Product $10^7$ `f64`)* | — | **52.7 ms** $\pm$ 1.2 ms | 215.8 ms $\pm$ 1.4 ms | 301.5 ms $\pm$ 3.2 ms | 398.8 ms $\pm$ 2.6 ms | **4.10x vs NumPy** (7.57x vs Julia) |
| **Suite 02: DataFrames** *(1M filas CSV + Filter + GroupBy + Agg)* | — | **658.8 ms** $\pm$ 17.9 ms | 1,223 ms $\pm$ 7 ms | 3,656 ms $\pm$ 55 ms | 1,505 ms $\pm$ 48 ms | **1.86x vs Pandas** (5.55x vs R) |
| **Suite 03: Modelado Estadístico** *(Gibbs Sampler 100 iter / 3k obs)* | — | **21.1 ms** $\pm$ 0.8 ms | 402.0 ms $\pm$ 8.9 ms | 139.5 ms $\pm$ 1.2 ms | 575.5 ms $\pm$ 6.2 ms | **6.62x vs R** (19.1x vs NumPy) |

---

## 2. Análisis Detallado por Suite

### Suite 04: Tiempo de Arranque y Time-To-First-X (TTFX)
- **GHL AOT Binary:** **`6.7 ms`**
- **GHL Interpreted:** **`10.7 ms`**
- **Python 3.14:** `34.8 ms`
- **R 4.6.1:** `115.1 ms`
- **Julia:** `166.4 ms`

> **Conclusión:** GHL resuelve de raíz el problema del *Time-To-First-Execution*. Al compilar a binario nativo AOT vía Cranelift/LLVM, el tiempo de arranque de GHL es **24.75x más rápido que Julia** y **5.18x más rápido que Python**. Incluso en modo interpretado (`ghl run`), el arranque toma apenas 10.7 ms gracias a su parser de descenso recursivo zero-allocation.

---

### Suite 01: Álgebra Vectorial y SIMD (Producto Escalar $10^7$ Flotantes)
- **GHL:** **`52.7 ms`** (4.10x vs Python, 5.72x vs R, 7.57x vs Julia)
- **Python (NumPy):** `215.8 ms`
- **R (Base):** `301.5 ms`
- **Julia:** `398.8 ms`

> **Conclusión:** El backend `faer` integrado en `ghl-runtime` satura los registros vectoriales SIMD (AVX2/FMA) sin incurrir en trampolines FFI lentos ni sobrecostes de boxing. Los $10^7$ elementos se computan en sólo 52 ms.

---

### Suite 02: Ingestión y Manipulación de DataFrames (1M Filas CSV)
- **Carga:** Ingestión de CSV sintético mixto de 84.2 MB (12 columnas), filtrado por predicado categórico (`status == "OK"`), agrupación de alta cardinalidad (`category`) y agregación con 4 reducciones estadísticas (`mean(value_b)`, `mean(value_c)`, `sum(value_a)`, `count()`).
- **GHL (Polars-backed):** **`658.8 ms`**
- **Python (Pandas):** `1,223 ms` (1.22 s)
- **Julia (Base Streaming):** `1,505 ms` (1.51 s)
- **R (Base `read.csv` + `aggregate`):** `3,656 ms` (3.66 s)

> **Conclusión:** El motor de DataFrames de GHL (apoyado en Apache Arrow y `polars-core`) supera a Pandas por **1.86x** y a R Base por **5.55x**, manteniendo una sintaxis declarativa limpia mediante pipes funcionales (`df |> filter(...) |> group_by(...) |> summarize(...)`).

---

### Suite 03: Modelado Estadístico y MCMC (Gibbs Sampler Jerárquico)
- **Carga:** Muestreador MCMC completo (100 iteraciones, 3.000 observaciones repartidas en 3 clusters con priors normales e hiper-prior Gamma).
- **GHL:** **`21.1 ms`**
- **R (Base):** `139.5 ms`
- **Python (NumPy):** `402.0 ms`
- **Julia:** `575.5 ms`

> **Conclusión:** En bucles iterativos donde se alternan actualizaciones de parámetros, filtros condicionales y muestreo de variables aleatorias, GHL demuestra la superioridad de su gestión de memoria sin pausas de Garbage Collection (ARC + Copy-on-Write) y su PRNG `xoshiro256++`, logrando ser **6.62x más rápido que R**, **19.08x más rápido que Python** y **27.32x más rápido que Julia**.

---

## 3. Verificación de Reproducibilidad

Todos los scripts ejecutados y los archivos de métricas brutas (`.json` y `.md`) generados por `hyperfine` se encuentran organizados modularmente en:
- `benchmarks/scripts/suite_01_math/`
- `benchmarks/scripts/suite_02_dataframe/`
- `benchmarks/scripts/suite_03_modeling/`
- `benchmarks/scripts/suite_04_runtime/`
- `benchmarks/scripts/harness/run_benchmarks.ps1`
- `benchmarks/results/raw/*.json`
