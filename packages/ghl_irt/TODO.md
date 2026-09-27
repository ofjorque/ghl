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

### Pilar 1: Vectorización Matricial con GEMM Nativo (`faer`) en GHL (Capa 1)
- [ ] **E-Step Matricial en `src/dichotomous.gh`:**
  - Convertir la matriz de patrones observados $\mathbf{Y}_{P \times J}$ y $(\mathbf{1} - \mathbf{Y})_{P \times J}$ a tipo `Matrix`.
  - Reemplazar los bucles anidados por dos productos de matrices BLAS:
    $$\log \mathbf{L}_{P \times Q} = \mathbf{Y}_{P \times J} \cdot (\log \mathbf{P})_{J \times Q} + (\mathbf{1} - \mathbf{Y})_{P \times J} \cdot (\log(\mathbf{1} - \mathbf{P}))_{J \times Q}$$
  - Calcular los endosos esperados $\mathbf{R}_{J \times Q}$ mediante un único GEMM con la transpuesta:
    $$\mathbf{R}_{J \times Q} = \mathbf{Y}^T_{J \times P} \cdot \mathbf{Post}_{P \times Q}$$
  - *Beneficio:* Los operadores `A * B` y `t(A)` delegan en `faer` multihilo con AVX2 nativo en Rust, reduciendo el E-step de milisegundos a microsegundos sin tocar código del compilador.

### Pilar 2: Aceleración de Convergencia EM de Aitken ($\Delta^2$ / SQUAREM)
- [ ] **Extrapolación de Parámetros en el M-Step:**
  - Implementar la aceleración de Aitken sobre el vector de parámetros de dificultad:
    $$\boldsymbol{b}^{(k+1)}_{\text{accel}} = \boldsymbol{b}^{(k)} - \frac{(\boldsymbol{b}^{(k)} - \boldsymbol{b}^{(k-1)})^{\odot 2}}{\boldsymbol{b}^{(k)} - 2\boldsymbol{b}^{(k-1)} + \boldsymbol{b}^{(k-2)}}$$
  - Intercalar el paso acelerado cada 2–3 iteraciones EM estándar.
  - *Beneficio:* Reduce el número de iteraciones necesarias para converger ($\epsilon < 10^{-4}$) de 35–45 ciclos a solo 12–16 ciclos ($2\times$ a $3\times$ de aceleración adicional).

### Pilar 3: Gestión de Memoria Regional Zero-Copy (`std::arena`)
- [ ] **Arenas en Calibración Dicotómica y Politómica:**
  - Envolver el ciclo iterativo EM en `std::arena::scope(|arena| { ... })`.
  - Asignar los búferes intermedios de trabajo ($\mathbf{P}$, $\log\mathbf{P}$, $\mathbf{Post}$, $\mathbf{R}$) usando `arena.alloc_matrix()`.
  - Mutar in-place con `Arc::make_mut` eliminando llamadas al allocator de sistema (`malloc`/`free`).

### Pilar 4: Kernel Numérico Opcional de Alta Dimensión (Capa 0 en Rust)
- [ ] **Kernel SIMD/Rayon para Cuadratura Masiva:**
  - Si se requiere rendimiento ultra-extremo para $D \ge 3$ ($Q^D \ge 3.375$ nodos), exponer en `crates/ghl-runtime` la primitiva `irt_em_quadrature_kernel` paralelizada con Rayon.

---

## 🎯 Metas Empíricas (Targets de Benchmark)

| Tarea / Modelo | Medición Actual GHL | Meta con GEMM + Aitken | R `mirt` (C++) |
|---|---|---|---|
| **1PL (Rasch) Model** ($N=1000, J=5$) | 1.551 ms | **< 100 ms** | 100 ms |
| **2PL (Birnbaum) Model** ($N=1000, J=5$) | 3.850 ms | **< 200 ms** | 40 ms |
| **Graded Response Model** ($N=392, J=4$) | 10.502 ms | **< 350 ms** | 110 ms |
| **High-Dim MIRT MHRM** ($D=6, N=500$) | 127 s | **< 10 s** | 6.4 s |
| **Multigroup LRT DIF** ($N=1000, J=5$) | 238 – 541 ms | **< 150 ms** | 640 ms |
