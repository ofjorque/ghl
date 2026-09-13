# TODO — Roadmap de Conformidad y Paridad Integral por RFC

Este documento audita y contrasta sistemáticamente el estado de implementación de **GHL** frente a cada uno de los 14 documentos de diseño normativos (**RFC 00 a RFC 13** en `docs/design/`).

> **Histórico:** El roadmap original de optimización y benchmarks (Fases 0 a 10) se encuentra 100% completado y archivado en [`docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md`](file:///run/media/oscarjorquera/4c6467bc-3d2b-40af-81c2-e1ec18ac9825/Rust%20Projects/Lang/docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md).

---

## Índice de Auditoría por RFC

- [RFC 00: Visión y Filosofía](#rfc-00-visión-y-filosofía)
- [RFC 01: Sintaxis, Gramática y Ergonomía](#rfc-01-sintaxis-gramática-y-ergonomía)
- [RFC 02: Sistema de Tipos y Semántica](#rfc-02-sistema-de-tipos-y-semántica)
- [RFC 03: Modelo de Memoria y Ejecución](#rfc-03-modelo-de-memoria-y-ejecución)
- [RFC 04: Biblioteca Estándar y Primitivas](#rfc-04-biblioteca-estándar-y-primitivas)
- [RFC 05: Concurrencia, Paralelismo y GPU](#rfc-05-concurrencia-paralelismo-y-gpu)
- [RFC 06: Interoperabilidad y Ecosistema](#rfc-06-interoperabilidad-y-ecosistema)
- [RFC 07: Herramientas, DX y Diagnósticos](#rfc-07-herramientas-dx-y-diagnósticos)
- [RFC 08: Pila Tecnológica del Compilador en Rust](#rfc-08-pila-tecnológica-del-compilador-en-rust)
- [RFC 09: Factores Categóricos, Gráficos y NIST StRD](#rfc-09-factores-categóricos-gráficos-y-nist-strd)
- [RFC 10: Presupuestos de Rendimiento y SLAs](#rfc-10-presupuestos-de-rendimiento-y-slas)
- [RFC 11: Framework de Modelado Estadístico NEKO](#rfc-11-framework-de-modelado-estadístico-neko)
- [RFC 12: Cockpit Deck Telemetría y Diagnóstico Visual](#rfc-12-cockpit-deck-telemetría-y-diagnóstico-visual)
- [RFC 13: Compilación JIT Nativa con Cranelift](#rfc-13-compilación-jit-nativa-con-cranelift)

---

## RFC 00: Visión y Filosofía
- [x] **Identidad dual Gojo & Haru:** Diagnósticos de ciencias de la computación fríos y certeros junto con calidez empática y Kaomojis felinos en estadística.
- [x] **Cero latencia TTFX:** Arranque en frío < 15 ms (< 1 ms en modo AOT).
- [x] **Memoria determinista sin Tracing GC:** Modelo ARC + CoW + Arenas de liberación instantánea.
- [x] **Interoperabilidad C-ABI / Arrow pura:** Sin puentes pesados ni dependencias runtime obligatorias de C++.

---

## RFC 01: Sintaxis, Gramática y Ergonomía
- [x] **Operador de tubería (`|>`):** Reescritura funcional de primer argumento (`df |> filter(...)`) y soporte de marcador de posición `_`.
- [x] **Operador de fórmulas estadísticas (`~`):** `y ~ x1 + x2 + (1 | id)` con parsing y AST `ExprKind::Formula`.
- [x] **Literales de colecciones de primera clase:**
  - [x] Vectores 1D tipados: `[1.0, 2.0, NA]`.
  - [x] Matrices 2D nativas: `mat[1.0, 2.0; 3.0, 4.0]`.
  - [x] DataFrames columnares: `dataframe { col_a: [1, 2], col_b: ["x", "y"] }`.
- [x] **Estructuras de Control como Expresiones:**
  - [x] `if / else` con evaluación de expresiones y ramas tipadas.
  - [x] `match` exhaustivo con patrones literales, wildcards, guardas y desestructuración `NA:Reason`.
  - [x] `while cond { ... }` con mutabilidad y TCO.
  - [x] `for var in start..end { ... }` con reuso de scope y cero alocaciones de heap por iteración.
- [x] **Funciones y Expresiones Lambda:**
  - [x] `fn name(params) { body }`.
  - [x] Lambdas concisas `x => x * 2` y `(x, y) => x + y`.
- [x] **Verbos canónicos de transformación tabular (`std::dataframe`):**
  - [x] `filter`, `select`, `drop`, `mutate`, `arrange`, `group_by`, `summarize`.
  - [x] `inner_join`, `left_join`, `outer_join`, `distinct`, `slice`, `slice_min`, `slice_max`.
  - [x] `head`, `tail`, `sample_n`, `sample_frac`, `fill_na`, `rename`, `count`, `pull`, `glimpse`.
- [ ] **Gaps / Extensiones pendientes de RFC 01:**
  - [x] **`pivot_wider` y `pivot_longer`:** Verbos de transformación de forma tabular (RFC 01 §7.3).
  - [ ] **`impute(col, strategy: Mean, only_for: [NAReason::...])` y `filter_na_reason`:** Imputación condicional y filtrado de filas por motivo de NA (RFC 01 §7.3 y RFC 02 §2.2).
  - [ ] **Literales de Registros / Named Tuples (`{ a: 1, b: 2 }`):** Soporte sintáctico para records estructurales en AST/eval (RFC 01 §3.4).
  - [ ] **Indexación general por corchetes (`v[0..5]`, `mat[0..2, :]`, `v[v > 0.0]`):** Azúcar sintáctico para rebanado directo en expresiones de vectores y matrices (RFC 01 §4).

---

## RFC 02: Sistema de Tipos y Semántica
- [x] **Inferencia de tipos bidireccional local:** Sistema Hindley-Milner extendido para escalares, vectores, matrices y DataFrames (`ghl-types`).
- [x] **Semántica unificada de `NA` y `NA:Reason`:**
  - [x] Distinción formal entre `NaN` (IEEE-754) y `NA` (dato faltante estadístico).
  - [x] Sintaxis `NA:Reason` en lexer y AST (`NA:SensorDropout`, `NA:NoResponse`, etc.).
  - [x] Lógica ternaria de Kleene en comparaciones y operadores lógicos (`&&`, `||`, `!`).
  - [x] Propagación transparente en funciones estadísticas (`mean`, `sum`, etc.) con flag `skip_na`.
  - [x] Representación eficiente: bitmask de 1 bit de Arrow para NA base + tabla `NaReasonTable` con interning `Arc<str>` para motivos.
  - [x] Funciones de inspección semántica: `na_reason(x)` y `na_reasons(df, col)`.
- [x] **Manejo de errores algebraicos:** `Result<T, E>` y separación entre `ComputeError` y `StatisticalError`.
- [ ] **Gaps / Extensiones pendientes de RFC 02:**
  - [ ] **`filter_na_reason(col, drop: [...])`:** Filtrado de filas según el motivo semántico de ausencia (RFC 02 §2.2).
  - [ ] **Comprobación estática de dimensiones matriciales (`Matrix<f64, ROWS, COLS>`):** Validación de dimensiones en tiempo de compilación mediante const generics en el type-checker (RFC 02 §3).
  - [ ] **Sintaxis de Traits y polimorfismo de usuario (`trait Distribution`, `impl Trait for Struct`):** Exposición de traits definidos por el usuario en el frontend del lenguaje (RFC 02 §4).

---

## RFC 03: Modelo de Memoria y Ejecución
- [x] **Semántica de valor estructural con Copy-on-Write (CoW):**
  - [x] Clonado $O(1)$ de DataFrames mediante columnas Arrow envueltas en `Arc`.
  - [x] Clonado $O(1)$ de Matrices mediante `Value::Matrix { data: Arc<Vec<f64>> }`.
  - [x] Clonado $O(1)$ de Vectores mediante `VectorData`.
  - [x] Clonado $O(1)$ de tablas de motivos mediante `Arc<str>`.
- [x] **Invariante de asignaciones de Heap en bucles:** 0 allocaciones por iteración en `for` y `while` mediante reutilización de variables locales en scope.
- [x] **Arenas regionales (`bumpalo`):** Módulo `crates/ghl-runtime/src/arena.rs` con asignador bump de recolección instantánea $O(1)$.
- [x] **Niveles de ejecución (Tiering):**
  - [x] Tier-1 REPL / Scripting interactivo instantáneo (< 15 ms).
  - [x] Compilación nativa JIT / AOT con Cranelift (`ghl-codegen`).
  - [x] Binarios standalone livianos (< 1 MB, muy por debajo de la meta de 15 MB).
- [ ] **Gaps / Extensiones pendientes de RFC 03:**
  - [ ] **Sintaxis de superficie para arenas regionales:** Exponer `arena::scope(|arena| { ... })` directamente en el lenguaje para scripts de usuario (RFC 03 §2.2).

---

## RFC 04: Biblioteca Estándar y Primitivas
- [x] **`std::linalg`:**
  - [x] Matrices, vectores, operador `\` (solve), QR, Cholesky, SVD, autovalores/autovectores (eigen), matrices de varianza-covarianza.
- [x] **`std::dataframe`:**
  - [x] Lectura y escritura multihilo de CSV y Parquet.
  - [x] Suite completa de verbos relacionales y agregaciones.
- [x] **`std::stats`:**
  - [x] Distribuciones: `Normal`, `Poisson`, `Gamma`, `Bernoulli`, `Uniform`, `Exponential`, `Beta`.
  - [x] Métodos: `sample`, `sample_n`, `pdf`, `log_pdf`, `cdf`, `quantile`.
  - [x] PRNG determinista bit-for-bit: `Xoshiro256PlusPlus`.
- [x] **`std::core`:**
  - [x] Tipos primitivos, E/S estándar, operaciones de texto y funciones matemáticas avanzadas (`sqrt`, `log`, `exp`, `erf`, `gamma`).
- [ ] **Gaps / Extensiones pendientes de RFC 04:**
  - [ ] **`std::autodiff`:** Diferenciación automática modo reversa y forward (`grad(f)` para optimizadores y HMC) (RFC 04 §5).
  - [ ] **Cliente/servidor asíncrono en `std::core`:** Primitivas de red TCP/HTTP para servir modelos estadísticos como microservicios (RFC 04 §6).

---

## RFC 05: Concurrencia, Paralelismo y GPU
- [x] **Paralelismo de memoria compartida:** Uso transversal de `rayon` en lectura CSV, agregaciones de DataFrames y operaciones matriciales.
- [x] **Aceleración por GPU (`std::gpu`):**
  - [x] Crate `crates/ghl-runtime/src/gpu.rs` basado en `wgpu 24.0` + `bytemuck 1.21`.
  - [x] `GpuContext`, `GpuVector`, `GpuMatrix` con fallback automático a CPU/faer si no hay GPU disponible.
- [ ] **Gaps / Extensiones pendientes de RFC 05:**
  - [ ] **Iteradores paralelos en sintaxis de usuario:** Exponer `(0..N).par_iter().map(...)` como método callable desde scripts GHL (RFC 05 §2).
  - [ ] **Pipelines WGSL pre-compilados y cacheados:** Optimización de shaders GEMM/reducciones para aceleración en hardware dedicada (RFC 05 §4.2).

---

## RFC 06: Interoperabilidad y Ecosistema
- [x] **Apache Arrow C Data Interface:** Interoperabilidad nativa zero-copy mediante buffers Arrow.
- [x] **Herramienta de Línea de Comandos (`ghl`):**
  - [x] `ghl run`: Ejecución directa de scripts.
  - [x] `ghl repl`: Shell interactiva con resaltado y diagnóstico.
  - [x] `ghl test`: Ejecución de pruebas unitarias.
  - [x] `ghl bench`: Harness de micro-benchmarks y suites.
  - [x] `ghl check`: Verificación de sintaxis y tipos sin ejecutar.
  - [x] `ghl aot` / `ghl jit`: Compilación y ejecución nativa con Cranelift.
- [ ] **Gaps / Extensiones pendientes de RFC 06:**
  - [ ] **Gestor de paquetes completo (`ghl new`, `ghl fetch`, `ghl.lock`):** Resolución de dependencias externas reproducibles con hashes SHA-256 (RFC 06 §4).
  - [ ] **Directiva de exportación FFI (`#[export_ffi]` / `extern "C"`):** Generación automática de bibliotecas compartidas (`.so` / `.dylib`) consumibles desde Python y R (RFC 06 §3).

---

## RFC 07: Herramientas, DX y Diagnósticos
- [x] **Taxonomía dual de diagnósticos:**
  - [x] Errores computacionales `[Compute Error Cxxxx]` con Kaomojis de Gojo (`(ノ°□°)ノ`, `(╯°□°)╯︵ ┻━┻`).
  - [x] Errores estadísticos `[Statistical Error Sxxxx]` con Kaomojis de Haru/NEKO (`(=^･ω･^=)`, `ฅ(ﾐΦ ﻌ Φﾐ)ฅ`).
- [x] **Renderizado enriquecido con `ariadne`:** Snippets de código, subrayado multicolor de tokens infractores y sugerencias accionables.
- [x] **Shell interactiva con `reedline`:** Historial, autocompletado y renderizado visual.

---

## RFC 08: Pila Tecnológica del Compilador en Rust
- [x] **100% de crates integradas y verificadas:**
  - `logos` (lexer DFA), `chumsky` (parser con recuperación de errores), `ariadne` (diagnósticos).
  - `polars-core` + `polars-lazy` (DataFrames columnares y optimizador de consultas).
  - `faer` (álgebra lineal pura en Rust), `rayon` (paralelismo work-stealing).
  - `statrs` + `rand_distr` (distribuciones), `rand_xoshiro` (PRNG determinista), `bumpalo` (arenas).
  - `plotters` (exportación gráfica PNG/SVG), `cranelift` (JIT nativo), `reedline` (REPL).

---

## RFC 09: Factores Categóricos, Gráficos y NIST StRD
- [x] **Factores categóricos nominales y ordinales:**
  - [x] `Factor<T>` y `OrderedFactor<T>` con codificación de diccionario Arrow.
  - [x] Matrices de contraste: `Treatment` (Dummy), `Sum` (Deviation), `Helmert`, `Poly` (Polinomial).
- [x] **Gramática de Gráficos declarativa (`std::plot`):**
  - [x] Geometrías: `geom_point`, `geom_line`, `geom_bar`, `geom_histogram`, `geom_smooth` (OLS trend line), `geom_boxplot`.
  - [x] Mapeo estético `aes(x, y, color, size, fill)`.
  - [x] Salida dual: Renderizado en terminal interactiva Cockpit (Unicode/Braille) con `show()` y exportación a archivos alta resolución PNG/SVG con `save("path")`.
- [x] **Validación numérica NIST StRD:** Precisión de hasta 15 dígitos en estimaciones OLS.

---

## RFC 10: Presupuestos de Rendimiento y SLAs
- [x] **Revisión 2 formalmente auditada y calibrada:**
  - [x] Cold start: GHL AOT `0.9 ms` (SLA: $\le 15\text{ ms}$).
  - [x] Bucles escalares: Gibbs sampler `27.1 ms` — 18.1x más rápido que NumPy.
  - [x] Dot product: `153.1 ms` — 2.28x más rápido que NumPy.
  - [x] Ingestión CSV: 1M filas en `429.7 ms` (1.07x de Python Polars, 2.7x más rápido que R data.table).
  - [x] Clonado de DataFrames, Matrices y NA Reasons: estrictamente $O(1)$ CoW.
  - [x] Huella de memoria y tamaño de binario AOT: `~114.5 KB` (SLA: $\le 20\text{ MB}$).

---

## RFC 11: Framework de Modelado Estadístico NEKO
- [x] **Desacoplamiento en tres momentos:** `ModelSpec` $\to$ `Blueprint` $\to$ `FittedModel`.
- [x] **Sistema de Capacidades implementadas:**
  - [x] `ols(formula, data)` / `linear_model`.
  - [x] `glm(formula, data, family: "binomial")` (Logit / Probit).
  - [x] `gmm(data, k)` (Gaussian Mixture Models con algoritmo EM multivariado).
  - [x] `summary(m)`, `tidy(m)`, `glance(m)`, `augment(m, data)`, `predict(m, newdata)`.
  - [x] `vcov(m, kind)` con matrices clásicas y robustas heterocedásticas (`HC0`–`HC3`).
- [x] **Trazabilidad de datos ausentes (`RowDisposition`):** Columnas `.used_in_fit` y `.na_reason` generadas de forma determinista en `augment()`.

---

## RFC 12: Cockpit Deck Telemetría y Diagnóstico Visual
- [x] **`RenderCaps`:** Detección de capacidades de terminal (Color ANSI 24-bit / 256 / NO_COLOR, Unicode vs ASCII puro, TTY interactivo, geometría de columnas).
- [x] **`SymbolRegistry`:** Catálogo canónico de símbolos de compilador (Gojo) y estadística (Haru/NEKO).
- [x] **Componentes Cockpit Deck:** Tablas con bordes redondeados y alineación estricta, paneles de telemetría, sparklines y barras de progreso.

---

## RFC 13: Compilación JIT Nativa con Cranelift
- [x] **Representación Intermedia (`ghl-ir`):** Tipos escalares (`I64`, `F64`, `Bool`, `Unit`), expresiones, sentencias y funciones lowering tipado.
- [x] **Generador de código SSA (`ghl-codegen`):** Traducción a Cranelift IR, asignación de memoria ejecutable `RX` y ejecución directa en memoria a velocidad nativa.
- [x] **Comandos CLI:** `ghl jit` y `ghl aot` integrados en la interfaz de línea de comandos.

---

## Resumen Ejecutivo de Gaps para Decisión de Próximos Pasos

A continuación se resumen las características pendientes más relevantes identificadas en los RFCs, agrupadas por área técnica para facilitar la priorización:

1. **Transformaciones Avanzadas de DataFrames (RFC 01 §7.3 & RFC 02 §2.2):**
   - `pivot_wider` y `pivot_longer` (reshape tabular).
   - `impute(col, strategy, only_for)` y `filter_na_reason(col, drop)`.
2. **Diferenciación Automática (`std::autodiff`, RFC 04 §5):**
   - Implementación de `grad(f)` para optimizadores y algoritmos bayesianos.
3. **Ergonomía de Indexación y Colecciones (RFC 01 §3.4, §4 & RFC 03 §2.2):**
   - Slicing general por rangos `v[0..5]` y records `{ key: value }`.
   - Bloques explícitos de arena `arena::scope(|a| { ... })`.
4. **Iteradores Paralelos en el DSL (RFC 05 §2):**
   - Sintaxis `(0..N).par_iter().map(...)` para Monte Carlo en scripts de usuario.
5. **Ecosistema y Distribución de Paquetes (RFC 06 §3, §4):**
   - `ghl new`, `ghl fetch`, `ghl.lock`.
   - `#[export_ffi]` para compilar extensiones `.so`/`.dylib` para Python y R.

