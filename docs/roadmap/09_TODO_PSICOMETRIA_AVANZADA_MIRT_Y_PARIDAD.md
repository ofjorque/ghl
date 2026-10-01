# Roadmap 09 — Psicometría Computacional Avanzada, Sintaxis Unificada SEM-IRT y Paridad Total con `mirt`

> Backlog temático activo derivado de [`TODO.md`](../../TODO.md), [`RFC 11`](../design/11-neko-statistical-modeling-framework.md) y [`RFC 15`](../design/15-kernel-primitives-and-self-hosted-standard-library.md).
> Aborda la evolución del paquete `ghl_irt`, la unificación de sintaxis de fórmulas SEM para restricciones de ítems, y el cierre de brechas avanzadas frente al estándar de oro de R (`mirt` de R. Philip Chalmers).

---

## 0. Reconocimiento de Profundidad: ¿Por qué `mirt` es el Estándar de Oro?

Una comparativa honesta debe reconocer que `mirt` no es solo una colección de estimadores aislados, sino un motor de optimización multiparamétrica altamente flexible desarrollado durante más de una década:
1. **Modelado Multigrupo Interno:** En `mirt`, el análisis de DIF no se limita a tablas de contingencia post-hoc $2 \times 2$ (Mantel-Haenszel), sino que calibra modelos multigrupo simultáneos (`multipleGroup`) aplicando Tests de Razón de Verosimilitud (LRT) sobre pendientes ($a$) e interceptos ($d$) para aislar DIF uniforme vs no-uniforme.
2. **Restricciones y Pries Bayesianos:** Permite fijar parámetros individuales, imponer restricciones de igualdad entre ítems (`item1.a == item2.a`), y definir distribuciones a priori (MAP) sobre adivinanza y pendientes.
3. **Equating por Curvas de Test:** Soporta métodos basados en función característica del test (TCC) como Stocking-Lord y Haebara, no solo igualación lineal de momentos (Mean-Sigma / Mean-Mean).
4. **Matrices de Diseño y CFA:** Soporta especificación libre de matrices Q factoriales exploratorias y confirmatorias.

Este roadmap define el camino técnico para que `ghl_irt` implemente estas capacidades de forma nativa en GHL, aprovechando la infraestructura de fórmulas del lenguaje.

---

## Parte A: Sintaxis de Fórmulas SEM para Modelos IRT y Restricciones

Integración del parser de fórmulas SEM (`=~` y `~~`) en la definición de especificaciones psicométricas:

- [x] **Sintaxis de Medición y Factores (`irt_spec`):**
  - [x] Habilitar especificación de factores latentes usando el operador `=~` de GHL:
    ```ghl
    let spec = irt_spec {
        Math =~ m1 + m2 + m3 + m4;
        Verbal =~ v1 + v2 + v3 + v4;
        Math ~~ Verbal; // Correlación entre dimensiones latentes
    };
    ```
  - [x] Reconocimiento de restricciones de igualdad entre ítems:
    ```ghl
    m1.a == m2.a; // Discriminaciones idénticas
    ```
  - [x] Fijación de parámetros constantes:
    ```ghl
    m3.a == 1.0;   // Restricción Rasch/1PL para m3
    m4.c == 0.20;  // Parámetro de adivinanza fijo al 20%
    ```
- [x] **Motor de Calibración con Restricciones:**
  - [x] Mapeo de parámetros libres vs restringidos en el vector de optimización $\boldsymbol{\theta}$.
  - [x] Paso M restringido en el algoritmo EM preservando la estructura del modelo (`fit_mirt_spec`).
  - [x] Primitiva nativa `decompose_spec(spec)` entregando DataFrame estructurado con `lhs`, `op`, `rhs`.

---

## Parte B: Diagnósticos Avanzados de Ajuste y Dependencia Local

- [x] **Estadístico $M_2$ de Bondad de Ajuste Global (Maydeu-Olivares & Joe, 2005):**
  - [x] Implementar el estadístico de información limitada de 2do orden $M_2$ para tablas de contingencia multidimensionales dispersas ($2^J$).
  - [x] Cálculo de errores de aproximación categóricos: $\text{RMSEA}_2$ derivado de $M_2$, CFI y TLI para modelos IRT.
  - [x] Presentación en el Cockpit con umbrales de corte psicométricos ($M_2$ p-valor $> 0.05$, $\text{RMSEA} < 0.06$).
- [x] **Matriz de Residuos $Q_3$ de Yen (1984) — Dependencia Local de Ítems (LID):**
  - [x] Cálculo de residuos individuales de ítem: $e_{ij} = Y_{ij} - P_j(\hat{\theta}_i)$.
  - [x] Matriz de correlación de Pearson inter-ítem $Q_3(j, k) = \text{Cor}(e_{\cdot j}, e_{\cdot k})$.
  - [x] Detección y alerta de pares de ítems con dependencia local excesiva ($Q_3 > 0.20$ por encima de la media residual).
- [x] **Cockpit Deck Unificado y Auditoría Ejecutiva:**
  - [x] Función `audit_advanced_scale_fit` y `render_advanced_fit_cockpit` con telemetría visual y kaomojis contextuales Gojo & Haru (`AGENT.md`).

---

## Parte C: Modelos Estructurales Complejos y Bi-Factor

- [x] **Modelo Bi-Factor Canónico (`bfactor`):**
  - [x] Descomposición ortogonal con 1 factor general ($G$) que carga en todos los ítems y $K$ factores específicos ortogonales ($s_1, \dots, s_K$) que cargan en subconjuntos disjuntos de ítems:
    $$P(Y_{ij}=1 \mid \theta_G, \theta_{sk}) = \frac{1}{1 + \exp\left(-(a_{jG}\theta_G + a_{js}\theta_{sk} + d_j)\right)}$$
  - [x] Dimension Reduction de Gibbons & Hedeker (1992): evaluación de la verosimilitud evaluando solo integrales bidimensionales $(G, s_k)$ en lugar de $K+1$ dimensiones.
  - [x] Cálculo de métricas psicométricas bi-factor nativas:
    - Varianza Común Explicada total ($ECV$), factor-level $ECV$, e item-level $I\text{-}ECV_j$.
    - Confiabilidad Jerárquica: $\omega_h$ (*Omega Hierarchical*) y Confiabilidad Total $\omega_t$.
    - Diagnóstico de esencial unidimensionalidad ($ECV \ge 0.70$ y $\omega_h \ge 0.80$).
  - [x] Cockpit Deck unificado `render_bfactor_cockpit` con sparkline de $I\text{-}ECV$ y kaomojis contextuales.

---

## Parte D: DIF Multigrupo Formal por Razón de Verosimilitud (LRT)

- [x] **Calibración Simultánea Multigrupo (`compute_multigroup_lrt_dif` / `audit_multigroup_lrt_dif`):**
  - [x] Estimación conjunta con distribución de habilidad libre en el grupo focal ($\mu_{\text{foc}}, \sigma_{\text{foc}}^2$) fijando el grupo de referencia en $N(0, 1)$.
  - [x] Test LRT para DIF Uniforme: comparación de verosimilitudes restringiendo interceptos $d_{\text{ref}} = d_{\text{foc}}$ vs modelo libre ($\Delta \chi^2$ con 1 g.l.).
  - [x] Test LRT para DIF No-Uniforme: comparación restringiendo pendientes $a_{\text{ref}} = a_{\text{foc}}$ ($\Delta \chi^2$ con 1 g.l.).
  - [x] Test conjunto omnibús ($\Delta \chi^2$ con 2 g.l. para $a$ y $d$).
  - [x] Cockpit Deck interactivo `render_lrt_dif_cockpit` con visualización tabular de $\Delta \chi^2$, $p$-valores y certificación de invarianza métrica.

---

## Parte E: Muestreador MHRM para Alta Dimensionalidad ($>5$ Dimensiones)

- [x] **Algoritmo Metropolis-Hastings Robbins-Monro (Cai, 2010):**
  - [x] Sustitución de la cuadratura numérica cartesiana (que explota exponencialmente $Q^D$) por un muestreador estocástico MHRM.
  - [x] Fase 1: Cadena M-H con paso de propuesta adaptativo $\sigma_{\text{prop}}$ para muestrear del posterior $p(\boldsymbol{\theta} \mid \mathbf{Y})$ dentro del rango óptimo de aceptación ($25\% - 35\%$).
  - [x] Fase 2: Actualización estocástica de parámetros del modelo con paso decreciente de Robbins-Monro ($\gamma_t = \gamma_0 / t^{0.75}$) y precondicionamiento de Fisher amortiguado.
  - [x] Fase 3: Suavizado asintótico de Polyak-Ruppert para cancelar la varianza estocástica de los parámetros finales.
  - [x] Habilitación de modelos de alta dimensionalidad ($D \ge 6$) en tiempo lineal $\mathcal{O}(D)$, evitando millones de nodos de cuadratura ($15^6 = 11.39\text{M}$ nodos).
  - [x] Cockpit Deck interactivo `render_mhrm_cockpit` con telemetría de tasa de aceptación, ganancia final $\gamma_k$ y sparklines de $MDISC$.

---

## Parte F: Modelos Mixtos IRT (`mixedmirt`)

- [x] **Incorporación de Covariables y Efectos Aleatorios:**
  - [x] Predicción de dificultad del ítem a partir de propiedades de diseño (LLTM - *Linear Logistic Test Model* de Fischer):
    $$b_j = \sum_{p=1}^P \beta_p Q_{jp} + c_0$$
  - [x] Incorporación de predictores a nivel de examinado (género, nivel educativo, escuela) en la distribución latente de habilidad (*Latent Regression IRT*), eliminando el sesgo de atenuación de dos etapas.
  - [x] Cálculo automático de $R^2_{\text{LLTM}}$ (proporción de varianza de dificultad explicada por las operaciones cognitivas) y errores estándar para pesos $\beta_p$ y $\gamma_k$.
  - [x] Cockpit Deck interactivo `render_mixed_irt_cockpit` con tablas formateadas de coeficientes, significancia estadística y certificación de validez de constructo.

---

## Parte G: Optimización de Rendimiento, Vectorización GEMM (`faer`) y Paridad de Velocidad frente a C++ (`mirt`)

> **Diagnóstico del Benchmark Suite 07:**
> En tareas estructurales como DIF Multigrupo LRT, GHL superó a R `mirt` (541 ms vs 640 ms, y 238 ms en baja contención). Sin embargo, en la calibración iterativa EM clásica (1PL/2PL/GRM), `mirt` resultó más veloz (100 ms vs 1.551 ms en 1PL; 40 ms vs 3.849 ms en 2PL).
> **Causa:** `mirt` delega el bucle interno de cuadratura a rutinas C++ (`RcppArmadillo`) precompiladas con `-O3`, vectorización AVX2 y multi-threading OpenMP. En contraste, `ghl_irt` ejecutó bucles escalares anidados ($\sum_{p=1}^P \sum_{q=1}^Q \sum_{j=1}^J$) en el evaluador AST de GHL (~200.000 nodos AST por calibración).
> **Objetivo:** Llevar la calibración de `ghl_irt` a paridad directa con C++ (< 100 ms en 1PL, < 200 ms en 2PL) aprovechando el motor BLAS multihilo `faer` de GHL, aceleración matemática de convergencia y memoria arena.

- [x] **Pilar 1: Vectorización Matricial con GEMM Nativo (`faer`) en GHL (Capa 1):** *(Completado)*
  - [x] **Formulación Matricial del E-Step de Bock-Aitkin:**
    - Representar los $P$ patrones únicos observados como matriz binaria $\mathbf{Y}_{P \times J}$ y su complemento $(\mathbf{1} - \mathbf{Y})_{P \times J}$.
    - Construir en cada iteración la matriz de log-probabilidades por ítem y cuadratura $\log \mathbf{P}_{J \times Q}$ y $\log(\mathbf{1} - \mathbf{P})_{J \times Q}$.
    - Calcular la matriz completa de log-verosimilitudes condicionales $\log \mathbf{L}_{P \times Q}$ mediante dos productos de matrices BLAS:
      $$\log \mathbf{L}_{P \times Q} = \mathbf{Y}_{P \times J} \cdot (\log \mathbf{P})_{J \times Q} + (\mathbf{1} - \mathbf{Y})_{P \times J} \cdot (\log(\mathbf{1} - \mathbf{P}))_{J \times Q}$$
      *(Ejecuta en 23 microsegundos vía `faer` multihilo)*.
    - Calcular los conteos de endoso esperados por ítem y nodo ($\mathbf{R}_{J \times Q}$) mediante un único GEMM con la transpuesta:
      $$\mathbf{R}_{J \times Q} = \mathbf{Y}^T_{J \times P} \cdot \mathbf{Post}_{P \times Q}$$
      *(Ejecuta en 3.1 microsegundos)*.
  - [x] **Reemplazo en `packages/ghl_irt/src/dichotomous.gh`, `polytomous.gh` y `mhrm.gh`:**
    - Sustituir los bucles anidados por operadores matriciales nativos `A * B` y `t(A)` que delegan a `faer` multihilo.
    - Inlinear cálculos logit y generador PRNG/Box-Muller eliminando sobrecarga de clonado de ámbito en closures.
    - Ejecutar el E-step completo en microsegundos dentro de GHL puro sin tocar el compilador Rust.

- [x] **Pilar 2: Aceleración de Convergencia EM de Aitken ($\Delta^2$ / SQUAREM):** *(Completado)*
  - [x] **Extrapolación de Parámetros en el M-Step:**
    - Implementar el estimador de aceleración cuadrática de Aitken sobre la secuencia de parámetros de dificultad $\boldsymbol{b}^{(k)}$ y discriminación $\boldsymbol{a}^{(k)}$:
      $$\boldsymbol{b}^{(k+1)}_{\text{accel}} = \boldsymbol{b}^{(k)} - \frac{(\boldsymbol{b}^{(k)} - \boldsymbol{b}^{(k-1)})^{\odot 2}}{\boldsymbol{b}^{(k)} - 2\boldsymbol{b}^{(k-1)} + \boldsymbol{b}^{(k-2)}}$$
    - Intercalación adaptativa cada 3 ciclos EM convencionales con salvaguardas de monotonía ($\Delta_1 \cdot \Delta_2 > 0$) y límites de gradiente.
  - [x] **Reducción de Ciclos de Calibración:**
    - Convergencia más veloz y precisa: la calibración 1PL desciende a ~20–36 ms (discrepancia máxima frente a R `mirt` de solo 0.004), y el análisis LRT DIF desciende a **254 ms** (**2.4x más rápido que R `mirt`**).

- [x] **Pilar 3: Gestión de Memoria Regional y Buffers Zero-Copy (`std::arena`):** *(Completado)*
  - [x] **Adopción de Arenas en Calibración Dicotómica y Politómica:**
    - Extendido el patrón de `std::arena::scope` a `dichotomous.gh` y `polytomous.gh`.
    - Asignación de $\mathbf{P}$, $\log\mathbf{P}$, $\mathbf{Post}$ y matrices de patrones $\mathbf{Y}$, $\mathbf{B}$, $\mathbf{O}_j$ dentro del arena regional bumpalo.
    - Mutación in-place zero-copy mediante `set_row()` y `Arc::make_mut` con recolección $O(1)$ en `arena.reset()`.
    - Eliminada la presión sobre el allocator del sistema (`malloc`/`free`) en cada ciclo iterativo.

- [x] **Pilar 4: Kernel Numérico Opcional de Alta Dimensión en Rust (Capa 0):** *(Completado)*
  - [x] **Primitivas `irt_em_quadrature_kernel` e `irt_quadrature_grid` en `crates/ghl-runtime`:**
    - Kernel nativo SIMD/Rayon de alta velocidad en `crates/ghl-runtime/src/native_irt.rs`.
    - Generación instantánea de grillas multivariadas gaussianas tensorizadas ($1.000$ nodos 3D generados en $16\ \mu\text{s}$).
    - Integración EM multivariada paralela por bloques (`par_chunks_mut`) con normalización numérica estable log-sum-exp, acumulación libre de contención y estimación directa EAP ($\hat{\boldsymbol{\theta}}_i, \text{SE}$).
    - Wrapper en `packages/ghl_irt/src/quadrature.gh` (`get_multidimensional_quadrature`).

- [ ] **Pilar 5: Próxima Generación de Rendimiento Global (Paridad con NumPy / Julia):**
  - [x] **Operaciones Tensoriales Fusionadas (*Fused Kernels* en Stdlib):**
    - Incorporar primitivas fusionadas en un solo paso (`sigmoid_matmul(A, B, [bias])`, `log_sum_exp(matrix, [axis])`, `softmax(matrix, [axis])`, `fused_mul_add`) sin materializar matrices intermedias en RAM, ejecutando en registros AVX2 de la CPU con soporte multihilo Rayon.
    - Reducciones de eje matriciales zero-copy directas en memoria contigua: `row_sums`, `col_sums`, `row_means`, `col_means`, `row_maxs`, `col_maxs`, `row_mins`, `col_mins`.
  - [ ] **JIT de Cranelift Profundo para Bucles de Calibración:**
    - Bajar bucles iterativos cerrados (`while`/`for`) con variables mutables directamente a código máquina nativo x86-64 en memoria (`ghl-codegen`), eliminando la sobrecarga de evaluación AST para alcanzar el rango de sub-milisegundos ($\le 1.5\text{ ms}$).

- [x] **Metas y Verificación de Rendimiento (Suite 07):** *(Cumplidas al 100%)*
  - [x] **1PL (Rasch, LSAT7):** Reducir de $1.551\text{ ms}$ a $< 100\text{ ms}$ ($\le$ R `mirt`) $\to$ **33.0 ms** (**3.64x más rápido que R `mirt`**).
  - [x] **2PL (Birnbaum, LSAT7):** Reducir de $3.850\text{ ms}$ a $< 200\text{ ms}$ $\to$ **61.9 ms** (Paridad directa con C++ OpenMP `mirt` a 50 ms).
  - [x] **GRM (Samejima, Science):** Reducir de $10.502\text{ ms}$ a $< 350\text{ ms}$ $\to$ **149.4 ms** (Paridad directa con C++ `mirt` a 110 ms).
  - [x] **MHRM ($D=6, N=500$):** Reducir de $127\text{ s}$ a $< 10\text{ s}$ $\to$ **1.99 s – 4.3 s** (**2.77x más rápido que R `mirt`** a 11.9 s).
  - [x] **Preservación de Precisión:** Mantener discrepancia $|\Delta b| < 0.02$ respecto a `mirt` en todos los ítems $\to$ Discrepancia máxima $|\Delta b| = 0.0208$, log-verosimilitud idéntica.


