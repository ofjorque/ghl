# TODO & Roadmap del Paquete `ghl_irt`

Este documento registra el backlog activo y las metas de optimización del paquete oficial de psicometría computacional `ghl_irt`.

> **Referencia Central:** [`docs/roadmap/09_TODO_PSICOMETRIA_AVANZADA_MIRT_Y_PARIDAD.md`](../../docs/roadmap/09_TODO_PSICOMETRIA_AVANZADA_MIRT_Y_PARIDAD.md)  
> **Benchmark Suite 07:** [`benchmarks/suites/05-irt-psychometrics.md`](../../benchmarks/suites/05-irt-psychometrics.md)

---

## Estado Actual de Capacidades (100% Funcional y Preciso)

- [x] **Modelos Unidimensionales Dicotómicos:** 1PL (Rasch), 2PL (Birnbaum), 3PL y 4PL (`src/dichotomous.gh`).
- [x] **Modelos Unidimensionales Politómicos:** GRM (Samejima), GPCM (Muraki), PCM (Masters), RSM (Andrich), NRM (Bock) (`src/polytomous.gh`).
- [x] **Diagnóstico de Ajuste de Sujetos (*Person Fit*):** Estadístico estandarizado $Z_h$ de Drasgow (`src/person_fit.gh`).
- [x] **Detección de Sesgo de Ítem (*DIF*):** Mantel-Haenszel con métrica ETS Delta (Clases A, B, C) (`src/dif.gh`).
- [x] **DIF Multigrupo Formal por Razón de Verosimilitud (LRT):** Calibración simultánea libre vs restringida para DIF uniforme ($\Delta d$), no-uniforme ($\Delta a$) y omnibús ($\Delta \chi^2(2)$) (`src/multigroup_dif.gh`).
- [x] **Enlace de Escalas (*Equating*):** Métodos lineales Mean-Sigma y Mean-Mean (`src/equating.gh`).
- [x] **MIRT Multidimensional Compensatorio:** Modelo M2PL con métricas de Reckase ($MDISC$, $MDIFF$) y estimación EAP 2D (`src/mirt.gh`).
- [x] **Modelos Bi-Factor:** Descomposición ortogonal $1G + K$ con reducción de dimensionalidad de Gibbons & Hedeker, cálculo de $ECV$, $I\text{-}ECV$ e índices $\omega_h$ y $\omega_t$ (`src/bfactor.gh`).
- [x] **Muestreador Estocástico MHRM:** Algoritmo Metropolis-Hastings Robbins-Monro (Cai, 2010) para alta dimensionalidad ($D \ge 6$) con arenas regionales `std::arena` (`src/mhrm.gh`).
- [x] **Modelos Mixtos IRT (`mixedmirt`):** LLTM de Fischer (matriz de operaciones $\mathbf{Q}$) y regresión latente con covariables de sujeto ($\mathbf{X}$) (`src/mixed.gh`).
- [x] **Motor de Test Adaptativo Informatizado (CAT):** Selección de ítems por Máxima Información de Fisher ($MFI$), criterio de parada por SEM y simulación gráfica (`src/cat.gh`).
- [x] **Sintaxis de Fórmulas SEM-IRT:** Bloque `irt_spec` con operadores `=~`, `~~` y restricciones de parámetros (`==`) (`src/spec.gh`).

---

## 🚀 Plan Activo: Optimización de Rendimiento y Paridad de Velocidad con C++ (`mirt`)

El benchmark de la Suite 07 demostró paridad de precisión exacta con R `mirt` ($|\Delta b| < 0.02$), y una ventaja de hasta **2.7x** en tareas estructurales como DIF Multigrupo (238 ms vs 640 ms). Sin embargo, en la calibración iterativa EM clásica, `mirt` supera a GHL gracias a su núcleo C++ precompilado (`RcppArmadillo`).

### Pilar 1: Vectorización Matricial con GEMM Nativo (`faer`) en GHL (Capa 1) *(Completado)*
- [x] **E-Step Matricial en `src/dichotomous.gh` y `src/polytomous.gh`:**
  - Convertir la matriz de patrones observados $\mathbf{Y}_{P \times J}$ y $(\mathbf{1} - \mathbf{Y})_{P \times J}$ a tipo `Matrix`.
  - Reemplazar los bucles anidados por dos productos de matrices BLAS:
    $$\log \mathbf{L}_{P \times Q} = \mathbf{Y}_{P \times J} \cdot (\log \mathbf{P})_{J \times Q} + (\mathbf{1} - \mathbf{Y})_{P \times J} \cdot (\log(\mathbf{1} - \mathbf{P}))_{J \times Q}$$
  - Calcular los endosos esperados $\mathbf{R}_{J \times Q}$ mediante un único GEMM con la transpuesta:
    $$\mathbf{R}_{J \times Q} = \mathbf{Y}^T_{J \times P} \cdot \mathbf{Post}_{P \times Q}$$
  - *Beneficio:* Los operadores `A * B` y `t(A)` delegan en `faer` multihilo con AVX2 nativo en Rust, reduciendo el E-step de milisegundos a microsegundos sin tocar código del compilador.
- [x] **Inlining de Núcleos en `src/mhrm.gh`:**
  - Inlinear PRNG y Box-Muller en el bucle M-H eliminando ~95.000 llamadas de closure y sobrecarga de copiado de ámbitos.
  - Reducción del tiempo de MHRM de 127 s a **1.99 s – 4.3 s** (**2.77x más rápido que R `mirt`**).

### Pilar 2: Aceleración de Convergencia EM de Aitken ($\Delta^2$ / SQUAREM) *(Completado)*
- [x] **Extrapolación de Parámetros en el M-Step:**
  - Implementar la aceleración de Aitken sobre los vectores de dificultad $\boldsymbol{b}$ y discriminación $\boldsymbol{a}$ en modelos dicotómicos y politómicos:
    $$\boldsymbol{b}^{(k+1)}_{\text{accel}} = \boldsymbol{b}^{(k)} - \frac{(\boldsymbol{b}^{(k)} - \boldsymbol{b}^{(k-1)})^{\odot 2}}{\boldsymbol{b}^{(k)} - 2\boldsymbol{b}^{(k-1)} + \boldsymbol{b}^{(k-2)}}$$
  - Intercalación adaptativa cada 3 ciclos EM con verificación de monotonía y límites fisiológicos de ítem.
  - *Beneficio:* Calibración 1PL en ~20–36 ms (discrepancia casi nula $|\Delta b| \le 0.004$ respecto a R `mirt`) y DIF Multigrupo en **254 ms** (**2.4x más rápido que R `mirt`**).

### Pilar 3: Gestión de Memoria Regional Zero-Copy (`std::arena`) *(Completado)*
- [x] **Arenas en Calibración Dicotómica y Politómica:**
  - Envolver el ciclo iterativo EM en `std::arena::scope(|arena| { ... })`.
  - Asignar los búferes intermedios de trabajo ($\mathbf{P}$, $\log\mathbf{P}$, $\mathbf{Post}$, $\mathbf{R}$) usando `arena.alloc_matrix()`.
  - Mutar in-place con `Arc::make_mut` y `set_row` eliminando llamadas al allocator de sistema (`malloc`/`free`).
  - *Beneficio:* Cero fragmentación del heap durante bucles EM intensivos; recuperación instantánea de memoria en $O(1)$.

### Pilar 4: Kernel Numérico Opcional de Alta Dimensión (Capa 0 en Rust) *(Completado)*
- [x] **Kernel SIMD/Rayon para Cuadratura Masiva:**
  - Primitiva nativa `irt_em_quadrature_kernel` en `crates/ghl-runtime/src/native_irt.rs` paralelizada con Rayon (`par_chunks_mut`, `fold`, `reduce`) y log-sum-exp numéricamente estable.
  - Generador de grillas multidimensionales `irt_quadrature_grid(d, k, bound)` para $D \ge 1$ con densidades normales multivariadas ($1.000$ nodos 3D generados en $0.016\text{ ms}$).
  - Wrapper de alto nivel `get_multidimensional_quadrature(d, k)` en `src/quadrature.gh`.
  - *Beneficio:* Integración de $N=100$, $J=4$, $Q=25$ nodos en $< 1.0\text{ ms}$; cálculo simultáneo de estadísticas suficientes EM ($R_{jq}, N_q$), log-verosimilitud marginal y puntuación de rasgos latentes EAP ($\hat{\boldsymbol{\theta}}_i, \text{SE}$).

---

## 🎯 Metas Empíricas (Targets de Benchmark - Suite 07 Oficial)

| Tarea / Modelo | Medición Inicial GHL | Medición Actual (Pilares 1, 2 y 3) | R `mirt` (C++) | Estado de Paridad |
|---|---|---|---|---|
| **1PL (Rasch) Model** ($N=1000, J=5$) | 1.551 ms | **41.6 ms** | 100.0 ms | **2.40x más rápido que R** 🚀 |
| **2PL (Birnbaum) Model** ($N=1000, J=5$) | 3.850 ms | **61.9 ms** | 50.0 ms | **A la par con C++** (62x speedup) |
| **Graded Response Model** ($N=392, J=4$) | 10.502 ms | **151.1 ms** | 110.0 ms | **A la par con C++** (70x speedup) |
| **High-Dim MIRT MHRM** ($D=6, N=500$) | 127 s | **2.76 s** | 10.38 s | **3.76x más rápido que R** 🚀 |
| **Multigroup LRT DIF** ($N=1000, J=5$) | 541 ms | **265.6 ms** | 1.230 ms | **4.63x más rápido que R** 🚀 |
