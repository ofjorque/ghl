# TODO — Índice Activo del Proyecto GHL

Este documento es el índice activo del trabajo pendiente de GHL. Se mantiene deliberadamente conciso: las fases completadas se archivan en [`docs/archive/`](docs/archive/), y el backlog temático de referencia vive en [`docs/roadmap/`](docs/roadmap/).

> **Histórico de Fases Archivadas (100% Completadas):**
> - **Roadmap Normativo por RFC (RFC 00 a RFC 13):** Especificación completa, semántica y paridad arquitectónica. Archivado en [`docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md`](docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md).
> - **Roadmap de Motor, Paralelismo y Aceleración (Fases 0 a 10):** Optimizaciones de bajo nivel, JIT Cranelift, iteradores Rayon y benchmarks empíricos. Archivado en [`docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md`](docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md).
> - **Roadmap de Ecosistema, Herramientas y DX (Roadmaps 01 a 07):** Editor Positron/VS Code VSIX, LSP, REPL/Quarto, validación numérica NIST StRD/R, benchmarks TPC-H/H2O/CLBG, validación masiva con datos reales (11M Higgs, 2.38M Airline), documentación `ghl doc`, guías de usuario y calidad/fuzzing. Archivado en [`docs/archive/TODO_ECOSYSTEM_AND_DX_ROADMAP.md`](docs/archive/TODO_ECOSYSTEM_AND_DX_ROADMAP.md).

---

## En Trabajo Activo: Roadmap 08 — Modelos Econométricos Avanzados, SEM, Kernel Stdlib y Librería `spring_pact`

Backlog activo detallado en [`docs/roadmap/08_TODO_MODELOS_AVANZADOS_Y_SPRING_PACT.md`](docs/roadmap/08_TODO_MODELOS_AVANZADOS_Y_SPRING_PACT.md) y [`RFC 15`](docs/design/15-kernel-primitives-and-self-hosted-standard-library.md):

> ### 🦀 / 🐱 Principio Rector de División de Responsabilidades:
> - **Rust (Capa 0 — Matemática Pesada de Bajo Nivel):** En Rust vive **estrictamente** la computación numérica intensiva donde la vectorización SIMD, la estabilidad de punto flotante o los bucles masivos $O(N \times \text{iter})$ son críticos: descomposiciones matriciales con `faer` (QR, SVD, Cholesky, Eigen), cálculo numérico de gradientes (`autodiff`), solver de optimización genérico multivariado `optim` (Nelder-Mead / L-BFGS), descenso por coordenadas (`glmnet`), proyecciones de centrado por medias de alta dimensión (*Within-transformation*), distribuciones continuas/discretas (`statrs`), y el constructor rápido de matriz de diseño (`model_matrix`).
> - **GHL (Capa 1 — Estimadores, Semántica y Experiencia de Usuario):** En código fuente `.gh` vive **todo el resto**: la firma pública de cada estimador (`sem`, `feols`, `iv_regress`, `lasso`, `anova`), la orquestación del análisis, las estructuras de resultados (`struct SemResult`, `struct FeolsResult`), la implementación de las capacidades (`impl ... { fn tidy, fn glance, fn summary, fn augment, fn vcov }`), la inferencia (p-valores, estadísticos $t$, intervalos de confianza), los diagnósticos y las librerías del ecosistema como `spring_pact`.

- [x] **Parte F: Primitivas de Matemática Pesada (Kernel Rust) y Habilitación de GHL ([RFC 15](docs/design/15-kernel-primitives-and-self-hosted-standard-library.md)):**
  - [x] **Kernel Rust (Capa 0):** Exponer `model_matrix(formula, df)` reutilizando `Blueprint::bake()` (retorna matriz $X$, vector $y$, nombres de términos y trazabilidad `RowDisposition`).
  - [x] **Kernel Rust (Capa 0):** Subsistema de distribuciones probabilísticas (`statrs` + `rand_xoshiro`) exponiendo la cuádruple interfaz canónica (`pdf`/`pmf`, `cdf`, `quantile`/`inv_cdf`, `sample` con seed reproducible) para familias continuas ($t$, $F$, $\chi^2$, Normal, Gamma, Beta, Uniforme, Exponencial) y discretas (Binomial, Poisson).
  - [x] **Kernel Rust (Capa 0):** Generalizar verbos de capacidades (`tidy`, `summary`, `glance`, `augment`, `vcov`) hacia `Value::Struct` para habilitar call-sites funcionales (`tidy(m)`).
  - [x] **Kernel Rust (Capa 0):** Implementar solver de optimización genérico `optim(fn, init, method)` (Nelder-Mead y L-BFGS sobre `autodiff.rs`).
  - [x] **Sintaxis de Fórmulas Multiparte (`|`):** Extender `ghl-syntax` para admitir particiones con `|` (`lhs ~ rhs | fe_or_parts | instruments`), permitiendo la sintaxis canónica de `feols` e `iv_regress`.
  - [x] **GHL Puro (Capa 1):** Implementar estimador piloto `anova(formula, df)` y tests de hipótesis clásicos (`t_test`, `chisq_test`) 100% en código `.gh` con `struct AnovaResult` y métodos `tidy()` y `glance()`.
- [x] **Parte A: Ecuaciones Estructurales (SEM / CFA):**
  - [x] **Sintaxis GHL:** Operadores `=~` (medición latente) y `~~` (covarianza/varianza residual) en `ghl-syntax`, más el bloque `sem_spec { eq1; eq2; ... }` en parser, AST y type-checker.
  - [x] **Kernel Rust (Capa 0 - Matemática Pesada):** Función de discrepancia de Máxima Verosimilitud de Wishart ($F_{ML}$) y cálculo de matriz de covarianza observada $S$.
  - [x] **Estimador GHL (Capa 1 - Lógica y Reporte):** Función `sem(spec, df)` en `.gh`, optimización vía `optim()`, extracción de índices de ajuste global ($\chi^2$, CFI, TLI, RMSEA) y `struct SemResult` con capacidades `summary()`, `tidy()`, `glance()`.
- [x] **Parte B: Estimadores Econométricos Clave con Sintaxis Representativa:**
  - [x] **Efectos Fijos de Alta Dimensión (`feols` — sintaxis con `|`):**
    - **Kernel Rust (Capa 0):** Operador de proyección y centrado por medias de alta dimensión (*Within-transformation* / Frisch-Waugh-Lovell absorbiendo `id + time`).
    - **Estimador GHL (Capa 1):** Función `feols(y ~ x | id + time, df)` en `.gh`, cálculo de errores estándar (robustos y por clúster), y `struct FeolsResult` con `tidy`, `summary`, `glance`.
  - [x] **Variables Instrumentales (`iv_regress` / 2SLS — sintaxis con `|`):**
    - **Estimador GHL (Capa 1):** 100% en `.gh` componiendo dos etapas de OLS (`model_matrix` + álgebra lineal sobre `y ~ x | z`), test F de instrumentos débiles, test de Wu-Hausman y `struct IvResult` con capacidades completas.
  - [x] **Regresión Regularizada (`lasso`, `ridge`, `elastic_net`):**
    - **Kernel Rust (Capa 0):** Solver de descenso por coordenadas (*Coordinate Descent* estilo `glmnet`) con penalizaciones $L_1$ y $L_2$.
    - **Estimador GHL (Capa 1):** Funciones `lasso`, `ridge`, búsqueda de $\lambda$ óptimo mediante validación cruzada $K$-fold (`cv_glmnet`) y `struct RegularizedResult` con `tidy`/`glance`.
- [x] **Parte C: `spring_pact` — Primera Librería Oficial 100% GHL ([RFC 14](docs/design/14-statistical-contracts-and-expectation-protocols.md)):**
  - [x] **Tooling CLI en Rust:** Comandos `ghl new`, `ghl fetch`, `ghl test`, y resolución reproducible con `ghl.lock` (SHA-256) en `ghl-cli::package`.
  - [x] **Librería 100% GHL:** Creación del paquete `spring_pact` con `ghl new` y configuración de `ghl.toml`.
  - [x] **Módulos de Reglas en GHL:** Reglas de datos (`expect_filter`, `expect_na_max`, `expect_outlier_rule`) y supuestos de modelos (`expect_vif_max`, `expect_homoscedasticity`, `expect_min_sample_size`).
  - [x] **Certificación en GHL:** Hashing criptográfico **SHA-256** del contrato y emisión de Certificado de Cumplimiento Cockpit Deck.
  - [x] **Suite de Pruebas en GHL:** Pruebas unitarias de la librería ejecutadas con `ghl test`.
- [x] **Parte E: Canon Representativo del Core (`std::stats` & `std::prob`) vs. Paquetes del Ecosistema:**
  - [x] **Subsistema de Probabilidad e Inferencia (`std::prob` en GHL Puro):**
    - Tipos canónicos de distribuciones como structs de primera clase (`Normal`, `StudentT`, `FisherF`, `ChiSq`, `Gamma`, `Beta`, `Binomial`, `Poisson`) exponiendo métodos `.pdf(x)`, `.cdf(q)`, `.quantile(p)` y `.sample(n, [seed])`.
    - Inferencia estadística en GHL puro: cálculo de p-valores (`p_value(stat, dist, alternative)`), intervalos de confianza (`conf_int(estimate, se, dist, level)`), y contrastes clásicos ($t$-Student, $F$-test, $\chi^2$, proporciones, correlación).
  - [x] **Canon Representativo del Core de GHL (`std::stats`):**
    - Solo los estimadores fundamentales que ejercitan las capacidades nucleares del lenguaje:
      1. Regresión lineal y generalizada: `ols`, `logistic`, `poisson` (álgebra lineal `faer`, IRLS y `model_matrix`).
      2. Ecuaciones estructurales: `sem(spec, df)` (DSL sintáctico específico con `=~`, `~~`, `sem_spec` y optimizador $F_{ML}$).
      3. Modelos con partición de fórmulas `|`: `feols` (efectos fijos / panel con proyección within) e `iv_regress` (2SLS bi-etápico).
      4. Modelos penalizados: `lasso`, `ridge`, `elastic_net` (descenso por coordenadas).
      5. Descomposición de varianza: `anova(formula, df)`, `manova` (contrastes ortogonales y tests F).
  - [x] **Ecosistema de Paquetes Externos (Desarrollados en GHL vía `ghl new` y distribuidos con `ghl.toml`):**
    - *Principio:* El repositorio del compilador no se sobrecarga con modelos hiperespecializados; estos se desarrollan como paquetes comunitarios o plugins en código GHL puro:
      - `ghl-causal` (`packages/ghl_causal`): Inferencia causal insignia en GHL puro (DiD clásico 2x2, test diagnóstico de pre-tendencias paralelas y Event Study dinámico con `ghl test`).
      - `ghl-timeseries`: Series temporales avanzadas (Filtro de Kalman, modelos ARIMA/SARIMA, volatilidad GARCH, modelos VAR).
      - `ghl-survival`: Análisis de supervivencia y eventos temporales (Curvas Kaplan-Meier, regresión de riesgos proporcionales de Cox).
      - `ghl-panel`: Modelos de panel dinámico (Arellano-Bond, Blundell-Bond GMM).
      - `ghl-multilevel`: Modelos jerárquicos multinivel de efectos mixtos (`lme4`-style).
      - `ghl_irt` (`packages/ghl_irt`): Suite psicométrica completa en GHL puro (1PL, 2PL, 3PL, 4PL, GRM, GPCM, PCM, RSM, NRM, Person Fit Zh, DIF Mantel-Haenszel, Equating, MIRT M2PL, y motor CAT adaptativo MFI).
- [x] **Parte D: Identidad Felina Dual y Dinámica de Personalidades (Gojo & Haru):**
  - [x] Corregir historia oficial en `AGENT.md` y RFCs (ambos son gatos reales del creador: Haru blanco/naranja y Gojo gris dominante; cero perros).
  - [x] Catálogo canónico de Kaomojis en `ghl-diagnostics::registry` y colores de terminal (`caps.haru` naranja cálido, `caps.gojo` gris pizarra).
  - [x] Reemplazo universal de `(U・ᴥ・U)` y tabla rota en runtime, REPL, CLI y diagnósticos por Haru y Gojo.
  - [x] Regla de oro *"Show, don't tell"*: jamás narrar acciones en texto ("Gojo te dio un zarpazo:"), dejar que el glifo se exprese por sí solo.

---

## En Trabajo Activo: Roadmap 09 — Psicometría Computacional Avanzada, Sintaxis Unificada SEM-IRT y Paridad Total con `mirt`

Backlog temático activo detallado en [`docs/roadmap/09_TODO_PSICOMETRIA_AVANZADA_MIRT_Y_PARIDAD.md`](docs/roadmap/09_TODO_PSICOMETRIA_AVANZADA_MIRT_Y_PARIDAD.md):

- [x] **Paquete Insignia `ghl_irt` (Fase 1 Inicial):**
  - [x] Modelos dicotómicos (1PL, 2PL, 3PL, 4PL) y politómicos (GRM, GPCM, PCM, RSM, NRM).
  - [x] Diagnóstico de ajuste de sujetos (*Person Fit*: $Z_h$ de Drasgow con alerta para $Z_h < -2.0$).
  - [x] Detección de sesgo (*DIF* Mantel-Haenszel y métrica ETS Delta Clases A/B/C).
  - [x] Enlace de escalas (*Equating* Mean-Sigma y Mean-Mean).
  - [x] Multidimensional IRT (M2PL compensatorio, métricas de Reckase $MDISC$/$MDIFF$, EAP 2D).
  - [x] Motor de test adaptativo (*CAT* con selección por Máxima Información de Fisher $MFI$, parada por SEM y simulador de trayectorias con sparklines).
  - [x] Cockpits visuales interactivos con sparklines ASCII (`  ▂▅▇███▇▅▂ ` y `▄ ▄█▇█`) y telemetría felina.
- [x] **Sintaxis Unificada SEM-IRT y Restricciones de Parámetros (Parte A):**
  - [x] Habilitar bloque `irt_spec` reutilizando operadores `=~` (medición), `~~` (covarianza de dimensiones) y `==` de SEM.
  - [x] Restricciones de igualdad entre ítems (`item1.a == item2.a`) y fijación de parámetros (`item3.a == 1.0`, `item4.c == 0.20`).
  - [x] Primitiva nativa `decompose_spec(spec)` que entrega un DataFrame estructurado con `lhs`, `op`, `rhs`.
- [x] **Diagnósticos Avanzados de Ajuste Global y Dependencia Local (Parte B):**
  - [x] Estadístico $M_2$ de información limitada (Maydeu-Olivares & Joe) para tablas dispersas $2^J$ con $\text{RMSEA}_2$, TLI y CFI categórico.
  - [x] Matriz de residuos $Q_3$ de Yen para evaluar dependencia local de ítems (*Local Item Dependence* / LID) y alerta de pares $Q_3 > 0.20$.
  - [x] Cockpit Deck unificado para diagnósticos avanzados con telemetría visual y kaomojis contextuales.
- [x] **Modelos Estructurales Complejos y Bi-Factor (Parte C):**
  - [x] Modelo Bi-Factor canónico (`bfactor`) con reducción dimensional de Gibbons & Hedeker (1992) evaluando integrales 2D $(G, s_k)$.
  - [x] Cálculo nativo de Varianza Común Explicada total ($ECV$), $I\text{-}ECV$ por ítem, y Confiabilidad Jerárquica ($\omega_h$ y $\omega_t$).
  - [x] Cockpit Deck bi-factor con sparklines de $I\text{-}ECV$ y diagnóstico de unidimensionalidad esencial.
- [x] **DIF Multigrupo Formal por Razón de Verosimilitud (LRT) (Parte D):**
  - [x] Calibración conjunta multigrupo (`compute_multigroup_lrt_dif` / `audit_multigroup_lrt_dif`) para contraste simultáneo de DIF uniforme ($\Delta d$), no uniforme ($\Delta a$) y test omnibús ($\Delta \chi^2(2)$).
  - [x] Estimación de impacto latente ($\mu_{\text{foc}}, \sigma_{\text{foc}}$) con grupo de referencia normalizado a $N(0, 1)$ y Cockpit Deck con certificación de invarianza.
- [x] **Muestreador MHRM para Alta Dimensionalidad ($>5D$) (Parte E):**
  - [x] Algoritmo estocástico Metropolis-Hastings Robbins-Monro (Cai, 2010) con propuesta adaptativa, decaimiento de ganancia $\gamma_k = \gamma_0 / k^{0.75}$ y suavizado asintótico de Polyak-Ruppert.
  - [x] Reducción de complejidad: evita la explosión combinatoria cartesiana $15^D$ ($11.39\text{M}$ nodos para $D=6$) sustituyéndola por imputación estocástica $\mathcal{O}(D)$.
  - [x] Cockpit Deck interactivo `render_mhrm_cockpit` con telemetría de tasa de aceptación, sparklines de $MDISC$ y kaomojis de certificación.
- [ ] **Modelos Mixtos IRT (`mixedmirt`):**
  - [ ] Modelos lineales logísticos de test (LLTM de Fischer) y regresión latente con covariables de examinado/ítem.

---

## Observaciones de UX / REPL e Interfaz Visual (Sesión en Vivo)

Puntos detectados durante el uso interactivo del REPL para pulir:

- [x] **Espaciado del Prompt REPL:** Separar `ghl` del kaomoji (`ghl ฅ(•⩊ •マ> ` en vez de `ghlฅ(•⩊ •マ> `) para mejorar la legibilidad del cursor.
- [x] **Comando `:var` como alias:** El REPL ahora acepta tanto `:vars`, `:var` como `:v` de manera indistinta para listar variables activas.
- [x] **Alineación tabular en `:vars`:** Reemplazado `CockpitPanel` por `CockpitTable` en `repl.rs` — nombres, tipos y valores ahora se alinean en columnas de ancho consistente por fila. De paso se corrigió un panic latente: el preview del valor truncaba por índice de byte crudo (`&s[..28]`), lo cual podía partir un carácter UTF-8 multibyte.
- [x] **Prompt con color ANSI en Rustyline:** Verificado contra el código fuente de `rustyline` 18.0.1 — `calculate_position` (matemática del cursor) siempre usa `prompt.raw()`, nunca el prompt estilizado, y `wrap_at_eol` (renderizado real, Unix y Windows) trata las secuencias CSI como ancho cero. El coloreado del kaomoji de Haru en `GhlPromptHelper::highlight_prompt` (`repl.rs`) ya es seguro tal cual está; no rompe el historial ni el cálculo de longitud de línea.
- [x] **Sugerencias de comandos desconocidos:** El error `C0005` ahora calcula distancia de Levenshtein (implementación propia, sin dependencia externa) contra la lista de comandos REPL conocidos y sugiere el más cercano cuando la distancia es ≤2 (ej. `:clera` → "Did you mean `:clear`?").



