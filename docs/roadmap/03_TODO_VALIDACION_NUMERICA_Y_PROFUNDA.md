# Roadmap — Validación Cruzada de Precisión Numérica y Pruebas Profundas del Sistema

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre las fases originales 4 y 6 (van juntas: ambas son garantías de **correctitud**, no de rendimiento).

---

## Parte A: Validación Cruzada de Precisión Numérica (R, Python, Julia & NIST StRD)

Creación del harness automatizado `tests/cross_validation/` para auditar la precisión de GHL frente a los estándares de referencia de la industria:

- [x] **Paridad con R (`stats::lm`, `glm`, `MASS`):**
  - [x] OLS: Coeficientes $\hat{\beta}$, errores estándar, $t$-stat, $p$-values, $R^2$, $R^2$ ajustado, F-statistic, residuos ($|error| < 10^{-10}$).
  - [x] GLM Logit: Estimaciones IRLS, deviance, log-likelihood, matrices de dispersión ($|error| < 10^{-9}$).
  - [x] Resúmenes de modelo con paridad exacta en salidas de `summary()`.
- [x] **Paridad con Python (`statsmodels`, `scipy.stats`):**
  - [x] Matrices de covarianza robustas frente a heterocedasticidad (HC0, HC1, HC2, HC3).
  - [x] Álgebra lineal: Singular Value Decomposition (SVD), factorización Cholesky ($|error| < 10^{-10}$).
- [x] **Paridad con Julia (`GLM.jl`, `LinearAlgebra`):**
  - [x] Descomposición QR y valores/vectores propios (Eigenvalues / Eigenvectors) en matrices simétricas.
  - [x] Desempeño y precisión en resolución de sistemas lineales (`A \ b`).
- [x] **Certificación NIST StRD (Datasets Canónicos de Referencia Estadística):**
  - [x] Dataset *Pontius* (polinomio de segundo grado).
  - [x] Dataset *Filippelli* (polinomio de décimo grado).
  - [x] Dataset *Longley* (problema clásico de multicolinealidad extrema).
  - [x] Dataset *Wampler* (evaluación de estabilidad numérica frente a errores de redondeo: Wampler1 y Wampler2).

---

## Parte B: Batería de Pruebas Profundas del Sistema

Pruebas especializadas para subsistemas críticos de GHL:

- [x] **Visual Regression & Renderizado Gráfico:**
  - [x] *SVG Snapshot Testing*: Comparación de árboles SVG generados contra archivos "golden" canónicos (puntos, líneas, histogramas, boxplots).
  - [x] *Stress Rendering*: Medición de latencia y uso de memoria graficando $100,000$ puntos en scatter plot y $1,000,000$ en histograma vs `ggplot2` y `matplotlib`.
  - [x] Validación de mapeo estético exacto (`aes`) para escalas continuas, discretas y paletas de color.
- [x] **Diferenciación Automática (Taylor Test / Gradient Checking):**
  - [x] Verificación de gradientes exactos de `grad()` y `value_and_grad()` contra aproximación numérica central de Taylor:
    $$\frac{f(x + \epsilon) - f(x - \epsilon)}{2\epsilon} \approx \nabla f(x) \quad (\text{tolerancia } \epsilon = 10^{-7}, \text{error relativo} < 10^{-6})$$
  - [x] Pruebas de convergencia de descenso de gradiente en funciones de prueba clásicas (*Rosenbrock*, *Rastrigin*) contra JAX y Julia `ForwardDiff.jl`.
- [x] **Arenas Regionales de Memoria (`bumpalo` / RFC 03):**
  - [x] *Desalocación Instantánea O(1)*: Comprobar que tras alocar 10,000 vectores y matrices en un `arena::scope`, la liberación sea en tiempo constante $O(1)$ sin destructores per-objeto.
  - [x] *Cero Fragmentación de Heap*: Ejecutar 100,000 ciclos continuos de `reset()` en bucle verificando que la memoria RSS no crezca.
  - [x] *Aislamiento y Seguridad de Punteros*: Garantizar que los valores asignados dentro de una arena no puedan escapar del scope ni provocar *use-after-free*.
  - [x] *Arenas Thread-Local en Rayon*: Comprobar que cada hilo de paralelismo tenga su propia arena sin contención de locks ni condiciones de carrera.
- [x] **Generación Pseudoaleatoria y Distribuciones (Batería PRNG):**
  - [x] Test formal de bondad de ajuste Kolmogorov-Smirnov y $\chi^2$ sobre $1,000,000$ de muestras de `random_normal` y `random_gamma` ($p > 0.05$).
  - [x] Verificación de independencia estadística y ausencia de autocorrelación serial entre streams paralelos generados con `Xoshiro256PlusPlus::jump()`.
- [x] **GPU vs CPU (WGPU & Shaders):**
  - [x] Paridad numérica GPU-CPU en `matmul` para matrices de $1024 \times 1024$ y $2048 \times 2048$ ($\|C_{\text{gpu}} - C_{\text{cpu}}\| < 10^{-5}$).
  - [x] Determinación del punto de cruce de rendimiento (*latency crossover point*) para transferencias de datos CPU <-> GPU.
- [x] **Lógica Kleene 3VL y Semántica de Valores Faltantes:**
  - [x] Matriz exhaustiva $3 \times 3$ ($\text{True}, \text{False}, \text{NA}$) para todos los operadores lógicos (`&&`, `||`, `!`) y relacionales (`==`, `!=`, `<`, `>`).
  - [x] Trazabilidad de motivos semánticos (`NA:Reason`) en pipelines multi-etapa complejos (Filter -> Mutate -> Impute -> Summarize).
- [x] **Zero-Leak & Estabilidad a Largo Plazo:**
  - [x] Prueba de resistencia de $1,000,000$ de iteraciones midiendo que el Resident Set Size (RSS) en RAM se mantenga estrictamente plano.
  - [x] *Parquet / Arrow Round-Trip Interoperability*: Escribir y leer archivos Parquet y streams Arrow entre GHL, Python Polars y R `arrow` verificando integridad binaria bit a bit.

---

## Parte C: Brechas de Implementación Detectadas — Neko, Cockpit y Gramática de Gráficos (Auditoría 2026-09-15)

Al revisar el código actual contra las RFC 09 (Gráficos/Factores) y RFC 11 (Neko) aparecieron huecos concretos entre lo especificado y lo implementado. No son tareas de validación sino de completitud de features — se listan aquí porque son prerequisito directo de las pruebas de Parte A y B:

- [x] **Matrices de Contraste para Fórmulas (RFC 09 §2.4):**
  - [x] Implementar `Contrast::Treatment(ref_level)`, `Contrast::Sum`, `Contrast::Helmert` y `Contrast::Polynomial` — hoy `Factor`/`OrderedFactor` se expanden a matriz de diseño configurable en `y ~ factor` con contraste automático Treatment/Sum/Helmert/Polynomial.
  - [x] Paridad con R/Python/Julia certificada para modelos con predictores categóricos.
- [x] **Backend de Exportación Raster/Vectorial Incompleto (RFC 09 §3):**
  - [x] `draw_plotters_*` en `ghl-diagnostics/src/plot.rs` cubre `geom_point` (scatter), `geom_histogram`, `geom_boxplot` y `geom_bar` con backend PNG/SVG vía `plotters`.
  - [x] *SVG Snapshot Testing* genera y valida los 4 geoms canónicos en `validation/snapshots/`.
- [x] **Sistema de Temas Inexistente:**
  - [x] Implementado en `ghl-diagnostics/src/plot.rs` y expuesto en runtime: `theme_minimal()`, `theme_classic()`, `theme_dark()`.
- [x] **Cockpit Deck — Cobertura de Consumidores en Neko:**
  - [x] `render_cockpit` implementado y activo para OLS (`summary(model)`), GLM y GMM.
