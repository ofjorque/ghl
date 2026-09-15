# Roadmap — Validación Cruzada de Precisión Numérica y Pruebas Profundas del Sistema

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre las fases originales 4 y 6 (van juntas: ambas son garantías de **correctitud**, no de rendimiento).

---

## Parte A: Validación Cruzada de Precisión Numérica (R, Python, Julia & NIST StRD)

Creación del harness automatizado `tests/cross_validation/` para auditar la precisión de GHL frente a los estándares de referencia de la industria:

- [ ] **Paridad con R (`stats::lm`, `glm`, `MASS`):**
  - [ ] OLS: Coeficientes $\hat{\beta}$, errores estándar, $t$-stat, $p$-values, $R^2$, $R^2$ ajustado, F-statistic, residuos ($|error| < 10^{-10}$).
  - [ ] GLM Logit: Estimaciones IRLS, deviance, log-likelihood, matrices de dispersión ($|error| < 10^{-9}$).
  - [ ] Resúmenes de modelo con paridad exacta en salidas de `summary()`.
- [ ] **Paridad con Python (`statsmodels`, `scipy.stats`):**
  - [ ] Matrices de covarianza robustas frente a heterocedasticidad (HC0, HC1, HC2, HC3).
  - [ ] Álgebra lineal: Singular Value Decomposition (SVD), factorización Cholesky ($|error| < 10^{-10}$).
- [ ] **Paridad con Julia (`GLM.jl`, `LinearAlgebra`):**
  - [ ] Descomposición QR y valores/vectores propios (Eigenvalues / Eigenvectors) en matrices simétricas.
  - [ ] Desempeño y precisión en resolución de sistemas lineales (`A \ b`).
- [ ] **Certificación NIST StRD (Datasets Canónicos de Referencia Estadística):**
  - [ ] Dataset *Pontius* (polinomio de segundo grado).
  - [ ] Dataset *Filippelli* (polinomio de décimo grado).
  - [ ] Dataset *Longley* (problema clásico de multicolinealidad extrema).
  - [ ] Dataset *Wampler* (evaluación de estabilidad numérica frente a errores de redondeo).

---

## Parte B: Batería de Pruebas Profundas del Sistema

Pruebas especializadas para subsistemas críticos de GHL:

- [ ] **Visual Regression & Renderizado Gráfico:**
  - [ ] *SVG Snapshot Testing*: Comparación de árboles SVG generados contra archivos "golden" canónicos (puntos, líneas, histogramas, boxplots).
  - [ ] *Stress Rendering*: Medición de latencia y uso de memoria graficando $100,000$ puntos en scatter plot y $1,000,000$ en histograma vs `ggplot2` y `matplotlib`.
  - [ ] Validación de mapeo estético exacto (`aes`) para escalas continuas, discretas y paletas de color.
- [ ] **Diferenciación Automática (Taylor Test / Gradient Checking):**
  - [ ] Verificación de gradientes exactos de `grad()` y `value_and_grad()` contra aproximación numérica central de Taylor:
    $$\frac{f(x + \epsilon) - f(x - \epsilon)}{2\epsilon} \approx \nabla f(x) \quad (\text{tolerancia } \epsilon = 10^{-7}, \text{error relativo} < 10^{-6})$$
  - [ ] Pruebas de convergencia de descenso de gradiente en funciones de prueba clásicas (*Rosenbrock*, *Rastrigin*) contra JAX y Julia `ForwardDiff.jl`.
- [ ] **Arenas Regionales de Memoria (`bumpalo` / RFC 03):**
  - [ ] *Desalocación Instantánea O(1)*: Comprobar que tras alocar 10,000 vectores y matrices en un `arena::scope`, la liberación sea en tiempo constante $O(1)$ sin destructores per-objeto.
  - [ ] *Cero Fragmentación de Heap*: Ejecutar 100,000 ciclos continuos de `reset()` en bucle verificando que la memoria RSS no crezca.
  - [ ] *Aislamiento y Seguridad de Punteros*: Garantizar que los valores asignados dentro de una arena no puedan escapar del scope ni provocar *use-after-free*.
  - [ ] *Arenas Thread-Local en Rayon*: Comprobar que cada hilo de paralelismo tenga su propia arena sin contención de locks ni condiciones de carrera.
- [ ] **Generación Pseudoaleatoria y Distribuciones (Batería PRNG):**
  - [ ] Test formal de bondad de ajuste Kolmogorov-Smirnov y $\chi^2$ sobre $1,000,000$ de muestras de `random_normal` y `random_gamma` ($p > 0.05$).
  - [ ] Verificación de independencia estadística y ausencia de autocorrelación serial entre streams paralelos generados con `Xoshiro256PlusPlus::jump()`.
- [ ] **GPU vs CPU (WGPU & Shaders):**
  - [ ] Paridad numérica GPU-CPU en `matmul` para matrices de $1024 \times 1024$ y $2048 \times 2048$ ($\|C_{\text{gpu}} - C_{\text{cpu}}\| < 10^{-5}$).
  - [ ] Determinación del punto de cruce de rendimiento (*latency crossover point*) para transferencias de datos CPU <-> GPU.
- [ ] **Lógica Kleene 3VL y Semántica de Valores Faltantes:**
  - [ ] Matriz exhaustiva $3 \times 3$ ($\text{True}, \text{False}, \text{NA}$) para todos los operadores lógicos (`&&`, `||`, `!`) y relacionales (`==`, `!=`, `<`, `>`).
  - [ ] Trazabilidad de motivos semánticos (`NA:Reason`) en pipelines multi-etapa complejos (Filter -> Mutate -> Impute -> Summarize).
- [ ] **Zero-Leak & Estabilidad a Largo Plazo:**
  - [ ] Prueba de resistencia de $1,000,000$ de iteraciones midiendo que el Resident Set Size (RSS) en RAM se mantenga estrictamente plano.
  - [ ] *Parquet / Arrow Round-Trip Interoperability*: Escribir y leer archivos Parquet y streams Arrow entre GHL, Python Polars y R `arrow` verificando integridad binaria bit a bit.
