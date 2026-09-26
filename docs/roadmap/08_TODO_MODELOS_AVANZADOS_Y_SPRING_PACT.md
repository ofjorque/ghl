# Roadmap 08 — Modelos Econométricos Avanzados, SEM, Kernel Stdlib y Librería `spring_pact`

> Backlog temático activo derivado de [`TODO.md`](../../TODO.md), [`RFC 14`](../design/14-statistical-contracts-and-expectation-protocols.md) y [`RFC 15`](../design/15-kernel-primitives-and-self-hosted-standard-library.md). 
> Aborda la expansión del motor econométrico NEKO, las primitivas de kernel y la concepción de **`spring_pact`**, la primera librería oficial desarrollada 100% en código fuente GHL.

---

## Parte A: Fórmulas y Estimadores de Ecuaciones Estructurales (SEM / CFA)

Cierre de la brecha analítica para constructos latentes y modelos de ecuaciones estructurales:

- [x] **Sintaxis de Fórmulas SEM en `ghl-syntax` y Runtime:**
  - [x] Reconocimiento léxico de nuevos operadores:
    - `=~` (`Token::EqualTilde`): Especificación de variables latentes (*"is measured by"* / modelo de medición).
    - `~~` (`Token::TildeTilde`): Covarianzas residuales y varianzas libres (*"correlated with"*).
  - [x] Extensión del parser de fórmulas para permitir especificaciones multi-ecuación (bloques `sem_spec { eq1; eq2; ... }`).
  - [x] Soporte de formateo canónico en `ghl fmt`, inferencia en type-checker y evaluación a `Value::SemSpec` en runtime.
- [x] **Estimador SEM (Kernel Rust + Lógica en GHL):**
  - [x] **Kernel Rust (Capa 0 - Matemática Pesada):** Cálculo de matriz de covarianzas empírica observada $S$, función de discrepancia de Máxima Verosimilitud de Wishart ($F_{ML} = \log|\Sigma(\theta)| + \text{tr}(S \Sigma^{-1}(\theta)) - \log|S| - p$) y minimización con `optim()`.
  - [x] **Estimador GHL (Capa 1 - Lógica y Capacidades):** Función pública `sem(spec, df)` en `.gh`, cálculo de índices de ajuste global ($\chi^2$, CFI, TLI, RMSEA, SRMR) y `struct SemResult` con `impl` de `summary()`, `tidy()` y `glance()`.

---

## Parte B: Estimadores Econométricos Canónicos con Sintaxis Representativa

Implementación desacoplada entre matemática de bajo nivel y estimadores de usuario, aprovechando la expresividad del DSL de fórmulas de GHL:

- [x] **Sintaxis de Fórmulas Multiparte (`|` en `ghl-syntax`):**
  - [x] Reconocimiento y estructuración de fórmulas divididas por tuberías (`FormulaParts`: `lhs ~ rhs | part2 | part3`).
  - [x] Evaluación y desglosado en `ghl-runtime` para asociar predictores principales, factores absorbidos e instrumentos.
- [x] **Efectos Fijos de Alta Dimensión (Panel Data — `feols` con `|`):**
  - [x] **Kernel Rust (Capa 0 - Matemática Pesada):** Algoritmo de centrado por medias (*Within-transformation* / proyección Frisch-Waugh-Lovell de alta dimensión absorbiendo `entity + time`).
  - [x] **Estimador GHL (Capa 1 - Lógica y Capacidades):** Función `feols(y ~ x | entity + time, df)` en `.gh`, cálculo de errores estándar (clásicos, robustos y clusterizados por grupo), y `struct FeolsResult` con `tidy`, `summary`, `glance`.
- [x] **Variables Instrumentales (IV / 2SLS — `iv_regress` con `|`):**
  - [x] **Estimador GHL (Capa 1 - GHL & Kernel Rust):** Primitiva `model_matrix(formula, df)`, función `iv_regress(y ~ x_exog + x_endog | x_exog + z_instr, df)` con descomposición automática de variables exógenas, endógenas e instrumentos excluidos, verificación de condición de orden ($L \ge K$), dos etapas de proyección (2SLS con faer), cálculo de residuos estructurales sobre $X$ real, errores estándar clásicos y robustos (HC1), diagnóstico de instrumentos débiles ($F > 10$), test de endogeneidad de Wu-Hausman, test de sobreidentificación de Sargan y `struct IvResult` con `impl` de `summary()`, `tidy()`, `glance()`, `vcov()`, `coef()` y `residuals()`.
- [x] **Regresión Regularizada Penalizada (Lasso, Ridge, ElasticNet):**
  - [x] **Kernel Rust (Capa 0 - Matemática Pesada):** Algoritmo de descenso por coordenadas (*Coordinate Descent* estilo `glmnet`) para penalizaciones $L_1$ y $L_2$ con warm starts a lo largo del path de regularización.
  - [x] **Estimador GHL (Capa 1 - Lógica y Capacidades):** Funciones `lasso`, `ridge`, `elastic_net`, búsqueda óptima de $\lambda$ mediante validación cruzada $K$-fold (`cv_glmnet`) reportando $\lambda_{\min}$ y regla de 1 error estándar $\lambda_{1\text{se}}$, y `struct RegularizedResult` con `impl` de `summary()`, `tidy()`, `glance()`, `coef()`, `residuals()` y `predict()`.

---

## Parte C: `spring_pact` — Primera Librería Oficial 100% GHL

Diseño e implementación de la primera librería de la comunidad escrita completamente en GHL para contratos estadísticos y protocolos de expectativas ([`RFC 14`](../design/14-statistical-contracts-and-expectation-protocols.md)):

- [x] **Infraestructura de Paquetes en la CLI (`ghl-cli::package`):**
  - [x] Creación de paquetes estandarizados con `ghl new`.
  - [x] Manifiesto canónico `ghl.toml` y resolución reproducible con `ghl fetch` (`ghl.lock` con SHA-256).
  - [x] Harness de pruebas unitarias ejecutadas directamente con `ghl test`.
- [x] **Desarrollo de la Librería `spring_pact` en GHL Puro:**
  - [x] Inicialización del paquete mediante `ghl new spring_pact`.
  - [x] **Módulo de Expectativas de Datos (`data_rules.gh`):**
    - [x] `expect_range(col, min, max)`: Verificación de rangos válidos admisibles (ej. edad $\ge 18$).
    - [x] `expect_na_max(col, threshold)`: Tolerancia porcentual de valores faltantes.
    - [x] `expect_outlier_rule(col, method, factor)`: Detección y tratamiento de casos extremos (IQR / Z-score).
    - [x] `enforce_data_rules(df, contract)`: Pipeline de saneamiento conforme al pacto.
  - [x] **Módulo de Expectativas de Modelos (`model_rules.gh`):**
    - [x] `expect_vif_max(threshold)`: Auditoría de multicolinealidad entre predictores.
    - [x] `expect_homoscedasticity(p_min)`: Exigencia de matrices de covarianza robustas (HC0–HC3) si hay heterocedasticidad.
    - [x] `expect_min_sample_size(min_n)`: Restricción de muestra mínima efectiva para inferencia.
  - [x] **Sello Criptográfico y Certificado de Cumplimiento (`certificate.gh`):**
    - [x] Generación de hash determinista **SHA-256** del contrato pactado.
    - [x] Emisión de recibo formal de cumplimiento (`AuditReceipt`) con resumen de reglas aprobadas/falladas.
    - [x] Renderizado en terminal mediante panel visual **Cockpit Deck** con felicitación de Haru (`/ᐠ˵- ⩊ -˵マ ✧`).
  - [x] **Suite de Pruebas y Validación:**
    - [x] Pruebas unitarias de la librería ejecutadas directamente con `ghl test`.

---

## Parte F: Kernel de Primitivas y Librería Estándar Auto-Alojada ([RFC 15](../design/15-kernel-primitives-and-self-hosted-standard-library.md))

Desbloqueo de primitivas base para que la librería estándar crezca en GHL puro:

- [x] **Fase 1 — Desbloqueo del Puente (Capa 0):**
  - [x] Exponer `model_matrix(formula, df)` en `ghl-runtime` reutilizando `Blueprint::bake()`.
  - [x] Subsistema de distribuciones probabilísticas (`statrs` + `rand_xoshiro`) exponiendo la cuádruple interfaz canónica (`pdf`/`pmf`, `cdf`, `quantile`/`inv_cdf`, `sample` con seed) para continuas ($t$, $F$, $\chi^2$, Normal, Gamma, Beta, Uniforme, Exponencial) y discretas (Binomial, Poisson).
  - [x] Extender funciones globales de capacidades (`tidy`, `glance`, `summary`, `augment`, `vcov`) hacia `Value::Struct`.
  - [x] Exponer primitivas nativas de Cockpit Deck (`cockpit`, `cockpit_with_badge`, `cockpit_add_kv`, `cockpit_add_line`, `cockpit_add_divider`, `show_cockpit`, `format_cockpit`, `sparkline`) y tipo `Value::CockpitPanel` para que paquetes (`spring_pact`, `ghl_causal`) rendericen paneles visuales de terminal sin strings hardcodeados.
- [x] **Fase 2 — Piloto de Stdlib en GHL Puro (Capa 1):**
  - [x] Implementar `anova(formula, df)` y tests de hipótesis clásicos (`t_test`, `chisq_test`) 100% en código `.gh`.
- [x] **Fase 3 — Optimizador Genérico en Capa 0 (`optim`):**
  - [x] Implementar Nelder-Mead y L-BFGS numérico sobre `autodiff.rs`.

---

## Parte E: Canon Representativo del Core (`std::stats` & `std::prob`) vs. Paquetes del Ecosistema

- [x] **Subsistema de Probabilidad e Inferencia (`std::prob` en GHL Puro):**
  - [x] Tipos canónicos de distribuciones como structs de primera clase (`Normal`, `StudentT`, `FisherF`, `ChiSq`, `Gamma`, `Beta`, `Binomial`, `Poisson`) exponiendo métodos `.pdf(x)`, `.cdf(q)`, `.quantile(p)` y `.sample(n, [seed])`.
  - [x] Inferencia estadística en GHL puro: cálculo de p-valores (`p_value(stat, dist, alternative)`), intervalos de confianza (`conf_int(estimate, se, dist, level)`), y contrastes clásicos ($t$-Student, $F$-test, $\chi^2$, proporciones, correlación).
- [x] **Canon Representativo del Core de GHL (`std::stats`):**
  - Solo los estimadores fundamentales que justifican o ejercitan las capacidades nucleares del lenguaje:
    1. Regresión lineal y generalizada: `ols`, `logistic`, `poisson` (álgebra lineal `faer`, IRLS y `model_matrix`).
    2. Ecuaciones estructurales: `sem(spec, df)` (DSL sintáctico específico con `=~`, `~~`, `sem_spec` y optimizador $F_{ML}$).
    3. Modelos con partición de fórmulas `|`: `feols` (efectos fijos / panel con proyección within) e `iv_regress` (2SLS bi-etápico).
    4. Modelos penalizados: `lasso`, `ridge`, `elastic_net` (descenso por coordenadas).
    5. Descomposición de varianza: `anova(formula, df)`, `manova` (contrastes ortogonales y tests F).
- [x] **Ecosistema de Paquetes Externos (Desarrollados en GHL vía `ghl new` y distribuidos con `ghl.toml`):**
  - *Principio:* El repositorio del compilador no se sobrecarga con modelos hiperespecializados; estos se desarrollan como paquetes comunitarios o plugins en código GHL puro:
    - [x] `ghl-causal` (`packages/ghl_causal`): Inferencia causal insignia en GHL puro (DiD clásico 2x2, test diagnóstico de pre-tendencias paralelas y Event Study dinámico con `ghl test`).
    - [x] `ghl-timeseries` (`packages/ghl_timeseries`): Series temporales avanzadas (Filtro de Kalman 1D de espacio de estados, modelos ARIMA/SARIMA con forecasting, volatilidad GARCH(1,1), telemetría Cockpit y test suites en GHL puro).
    - [x] `ghl-survival` (`packages/ghl_survival`): Análisis de supervivencia y eventos temporales (Curvas Kaplan-Meier con Greenwood CI, test de hipótesis Log-Rank de 2 muestras, regresión de Cox con Newton-Raphson y telemetría Cockpit).
    - [x] `ghl-panel` (`packages/ghl_panel`): Modelos de panel dinámico (Arellano-Bond Difference GMM, Blundell-Bond System GMM, tests de correlación serial AR(1)/AR(2) clusterizados, test de Sargan y telemetría Cockpit).
    - [x] `ghl-multilevel` (`packages/ghl_multilevel`): Modelos jerárquicos multinivel de efectos mixtos (`lme4`-style con descomposición de varianza REML/EM, Intraclass Correlation ICC, efectos de diseño de Kish y BLUPs empíricos de Bayes).

---

## Parte D: Identidad Felina Dual y Dinámica de Personalidades (Gojo & Haru)

Actualización canónica de la UX y narrativa de GHL para reflejar fielmente a los dos gatos reales del creador (respetando la regla *"Show, don't tell"* de `AGENT.md`):

- [x] **Alineación Conceptual y Documental (`AGENT.md` y RFCs):**
  - [x] Erradicar cualquier referencia canina (`(U・ᴥ・U)`) en la documentación y código.
  - [x] Definir los perfiles oficiales de los dos gatos:
    - **Haru (Gato Blanco y Naranja 🐱🧡🤍):** Tierno, amable, juguetón y sociable. Gobierna los éxitos (`PASS`, `CONVERGED`), bienvenida y despedida en el REPL, documentación y sellos de aprobación de `spring_pact`.
    - **Gojo (Gato Gris 😼🩶):** Fuerte, dominante, riguroso y rudo. Gobierna el rigor implacable, errores de compilación (`[Compute Error]`), matrices singulares no invertibles (`[Statistical Error]`), advertencias estadísticas y contratos violados.
- [x] **Actualización del Registro Semántico (`ghl-diagnostics::registry`):**
  - [x] Adopción de Kaomojis canónicos de Haru (`ฅ(•⩊ •マ`, `ദ്ദി/ᐠ - ⩊ -マ`, `/ᐠ˵- ⩊ -˵マ ✧`) y Gojo (`/ᐠ ¬`‸´¬ マ`, `≽(◉˕ ◉ ≼マ`, `ദ്ദി(ᓀ‸ᓂマ ੭`).
- [x] **Propagación en Badges y Mensajes del Runtime, CLI y REPL:**
  - [x] Paneles de convergencia en NEKO (`neko.rs`, `glm.rs`, `gmm.rs`): `/ᐠ˵- ⩊ -˵マ ✧ CONVERGED`.
  - [x] REPL (`repl.rs`): Saludo y bienvenida de Haru `ฅ(•⩊ •マ`.
  - [x] Subcomandos de `ghl-cli` (`package.rs`, `main.rs`): Badges `[PASS]`, `[LOCKED]`, `[CREATED]`.

