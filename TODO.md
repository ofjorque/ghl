# TODO — Índice Activo del Proyecto GHL

Este documento es el índice activo del trabajo pendiente de GHL. Se mantiene deliberadamente conciso: las fases completadas se archivan en [`docs/archive/`](docs/archive/), y el backlog temático de referencia vive en [`docs/roadmap/`](docs/roadmap/).

> **Histórico de Fases Archivadas (100% Completadas):**
> - **Roadmap Normativo por RFC (RFC 00 a RFC 13):** Especificación completa, semántica y paridad arquitectónica. Archivado en [`docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md`](docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md).
> - **Roadmap de Motor, Paralelismo y Aceleración (Fases 0 a 10):** Optimizaciones de bajo nivel, JIT Cranelift, iteradores Rayon y benchmarks empíricos. Archivado en [`docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md`](docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md).
> - **Roadmap de Ecosistema, Herramientas y DX (Roadmaps 01 a 07):** Editor Positron/VS Code VSIX, LSP, REPL/Quarto, validación numérica NIST StRD/R, benchmarks TPC-H/H2O/CLBG, validación masiva con datos reales (11M Higgs, 2.38M Airline), documentación `ghl doc`, guías de usuario y calidad/fuzzing. Archivado en [`docs/archive/TODO_ECOSYSTEM_AND_DX_ROADMAP.md`](docs/archive/TODO_ECOSYSTEM_AND_DX_ROADMAP.md).

---

## En Trabajo Activo: Roadmap 08 — Modelos Econométricos Avanzados, SEM y Librería `spring_pact`

Backlog activo detallado en [`docs/roadmap/08_TODO_MODELOS_AVANZADOS_Y_SPRING_PACT.md`](docs/roadmap/08_TODO_MODELOS_AVANZADOS_Y_SPRING_PACT.md):

- [ ] **Parte A: Ecuaciones Estructurales (SEM / CFA):**
  - [x] Operadores `=~` (medición latente) y `~~` (covarianza/varianza residual) en `ghl-syntax`, más el bloque `sem_spec { eq1; eq2; ... }` (`;` obligatorio por ecuación) para agrupar varias en una sola especificación.
  - [ ] Estimador `sem(spec, df)` en `ghl-runtime::neko` basado en optimización Wishart ML e índices de ajuste ($\chi^2$, CFI, TLI, RMSEA).
- [ ] **Parte B: Estimadores Econométricos Clave:**
  - [ ] Efectos Fijos de Alta Dimensión (`feols(y ~ x | id + time, df)`) con algoritmo de centrado por medias (*Within-transformation*).
  - [ ] Variables Instrumentales (`iv_regress(y ~ x_exog + x_endog | x_exog + z_instr, df)`) con 2SLS, F-stat y test de Wu-Hausman.
  - [ ] Regresión Regularizada (`lasso`, `ridge`, `elastic_net`) con descenso por coordenadas (`glmnet` style).
- [ ] **Parte C: `spring_pact` — Primera Librería Oficial 100% GHL ([RFC 14](docs/design/14-statistical-contracts-and-expectation-protocols.md)):**
  - [ ] Estructura de paquete con `ghl new spring_pact` y `ghl.toml`.
  - [ ] Módulos de reglas de datos (`expect_filter`, `expect_na_max`, `expect_outlier_rule`).
  - [ ] Módulos de supuestos estadísticos (`expect_vif_max`, `expect_homoscedasticity`, `expect_min_sample_size`).
  - [ ] Hashing criptográfico **SHA-256** del contrato y emisión de Certificado de Cumplimiento Cockpit Deck.
  - [ ] Suite de pruebas unitarias ejecutadas con `ghl test`.
- [ ] **Parte E: Backlog Extendido de Estimadores Estadísticos y Econométricos** (ya cubiertos: OLS, regresión logística/GLM, GMM de clustering, y Parte A/B de arriba — SEM, `feols`, `iv_regress`, `lasso`/`ridge`/`elastic_net`):
  - [ ] **Series de Tiempo:** `arima(y, order)`/`sarima` (Box-Jenkins), `var(vars, lags)` (Vector Autoregression), `garch`/`arch` (volatilidad condicional), filtro de Kalman / modelos de espacio de estados.
  - [ ] **Panel Avanzado:** Efectos Aleatorios (`feols` con GLS) y test de Hausman (RE vs FE), GMM dinámico de panel (Arellano-Bond / Blundell-Bond, `xtabond`-style).
  - [ ] **Elección Discreta y Conteo:** Logit/Probit ordinal, Logit multinomial, Regresión de Poisson y Binomial Negativa (datos de conteo).
  - [ ] **Robustez y Cuantiles:** Regresión Cuantílica (`rq(y ~ x, df, tau)`), errores estándar robustos por clúster (verificar si el sandwich HC ya cubre esto o falta la variante clúster), M-estimadores.
  - [ ] **Inferencia Causal Moderna:** Diferencias-en-Diferencias (incl. diseños escalonados tipo Callaway-Sant'Anna), Regresión Discontinua (RDD), Emparejamiento por Puntaje de Propensión (PSM), Control Sintético.
  - [ ] **Supervivencia:** Estimador Kaplan-Meier, Regresión de Cox (riesgos proporcionales).
  - [ ] **Reducción de Dimensionalidad:** PCA, Análisis Factorial Exploratorio (EFA) — complementa el CFA de la Parte A.
  - [ ] **Multinivel/Mixtos:** Modelos jerárquicos con intercepto/pendiente aleatorios (estilo `lme4`).
  - [ ] **No Lineal General:** Mínimos Cuadrados No Lineales (`nls`), splines/GAM.
- [x] **Parte D: Identidad Felina Dual y Dinámica de Personalidades (Gojo & Haru):**
  - [x] Corregir historia oficial en `AGENT.md` y RFCs (ambos son gatos reales del creador: Haru blanco/naranja y Gojo gris dominante; cero perros).
  - [x] Catálogo canónico de Kaomojis en `ghl-diagnostics::registry` y colores de terminal (`caps.haru` naranja cálido, `caps.gojo` gris pizarra).
  - [x] Reemplazo universal de `(U・ᴥ・U)` y tabla rota en runtime, REPL, CLI y diagnósticos por Haru y Gojo.
  - [x] Regla de oro *"Show, don't tell"*: jamás narrar acciones en texto ("Gojo te dio un zarpazo:"), dejar que el glifo se exprese por sí solo.

---

## Observaciones de UX / REPL e Interfaz Visual (Sesión en Vivo)

Puntos detectados durante el uso interactivo del REPL para pulir:

- [x] **Espaciado del Prompt REPL:** Separar `ghl` del kaomoji (`ghl ฅ(•⩊ •マ> ` en vez de `ghlฅ(•⩊ •マ> `) para mejorar la legibilidad del cursor.
- [x] **Comando `:var` como alias:** El REPL ahora acepta tanto `:vars`, `:var` como `:v` de manera indistinta para listar variables activas.
- [x] **Alineación tabular en `:vars`:** Reemplazado `CockpitPanel` por `CockpitTable` en `repl.rs` — nombres, tipos y valores ahora se alinean en columnas de ancho consistente por fila. De paso se corrigió un panic latente: el preview del valor truncaba por índice de byte crudo (`&s[..28]`), lo cual podía partir un carácter UTF-8 multibyte.
- [x] **Prompt con color ANSI en Rustyline:** Verificado contra el código fuente de `rustyline` 18.0.1 — `calculate_position` (matemática del cursor) siempre usa `prompt.raw()`, nunca el prompt estilizado, y `wrap_at_eol` (renderizado real, Unix y Windows) trata las secuencias CSI como ancho cero. El coloreado del kaomoji de Haru en `GhlPromptHelper::highlight_prompt` (`repl.rs`) ya es seguro tal cual está; no rompe el historial ni el cálculo de longitud de línea.
- [x] **Sugerencias de comandos desconocidos:** El error `C0005` ahora calcula distancia de Levenshtein (implementación propia, sin dependencia externa) contra la lista de comandos REPL conocidos y sugiere el más cercano cuando la distancia es ≤2 (ej. `:clera` → "Did you mean `:clear`?").



