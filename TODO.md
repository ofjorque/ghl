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
  - [ ] Operadores `=~` (medición latente) y `~~` (covarianza/varianza residual) en `ghl-syntax`.
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

