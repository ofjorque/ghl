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
- [ ] **Alineación tabular en `:vars`:** Cuando existan múltiples variables en sesión, alinear nombres, tipos (`[i64]`, `[f64]`, `[DataFrame]`) y valores en columnas con ancho consistente o `CockpitTable`.
- [x] **Prompt con color ANSI en Rustyline:** Verificado contra el código fuente de `rustyline` 18.0.1 — `calculate_position` (matemática del cursor) siempre usa `prompt.raw()`, nunca el prompt estilizado, y `wrap_at_eol` (renderizado real, Unix y Windows) trata las secuencias CSI como ancho cero. El coloreado del kaomoji de Haru en `GhlPromptHelper::highlight_prompt` (`repl.rs`) ya es seguro tal cual está; no rompe el historial ni el cálculo de longitud de línea.
- [ ] **Sugerencias de comandos desconocidos:** En error `C0005` ante `:rm` o comando con typo (ej. `:clera`), ofrecer sugerencia Levenshtein del comando REPL más cercano (ej. `¿Quisiste decir :clear?`).



