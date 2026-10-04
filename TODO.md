# TODO — Índice Activo del Proyecto GHL

Este documento es el índice activo del trabajo pendiente de GHL. Se mantiene deliberadamente conciso: las fases completadas se archivan en [`docs/archive/`](docs/archive/), y el backlog temático de referencia vive en [`docs/roadmap/`](docs/roadmap/).

> **Histórico de Fases Archivadas (100% Completadas):**
> - **Roadmap Normativo por RFC (RFC 00 a RFC 13):** Especificación completa, semántica y paridad arquitectónica. Archivado en [`docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md`](docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md).
> - **Roadmap de Motor, Paralelismo y Aceleración (Fases 0 a 10):** Optimizaciones de bajo nivel, JIT Cranelift, iteradores Rayon y benchmarks empíricos. Archivado en [`docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md`](docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md).
> - **Roadmap de Ecosistema, Herramientas y DX (Roadmaps 01 a 07):** Editor Positron/VS Code VSIX, LSP, REPL/Quarto, validación numérica NIST StRD/R, benchmarks TPC-H/H2O/CLBG, validación masiva con datos reales, documentación `ghl doc`, guías de usuario y calidad/fuzzing. Archivado en [`docs/archive/TODO_ECOSYSTEM_AND_DX_ROADMAP.md`](docs/archive/TODO_ECOSYSTEM_AND_DX_ROADMAP.md).
> - **Roadmaps 08 y 09 — Econometría Avanzada, SEM, Kernel Stdlib y Psicometría IRT:** Ecuaciones estructurales (`sem`), estimadores clave (`feols`, `iv_regress`, `lasso`), librería `spring_pact`, suite psicométrica `ghl_irt` con MHRM, bi-factor, LRT DIF, tensores fusionados (`sigmoid_matmul`, `log_sum_exp`, `softmax`), JIT Cranelift profundo para bucles, notación científica y documentación semántica. Archivado en [`docs/archive/TODO_ROADMAPS_08_09_ADVANCED_MODELS_AND_MIRT.md`](docs/archive/TODO_ROADMAPS_08_09_ADVANCED_MODELS_AND_MIRT.md).

---

## En Trabajo Activo: Roadmap 10 — Ergonomía Sintáctica de Colecciones, Visualización Científica, Interop y Cuadernos

Backlog temático para consolidar la ergonomía de última generación del lenguaje, gráficos científicos nativos y computación reproducible:

### Parte A: Ergonomía Sintáctica de Colecciones *(En Desarrollo Prioritario)*
- [x] **Comprensión Multidimensional de Vectores y Matrices (*Vector & Matrix Comprehensions*):**
  - Soporte canónico $N$-dimensional: 1D `[expr for var in iterable (if cond)?]` (Vector), 2D `[expr for r in rows, c in cols]` (Matriz en estándar `[fila, columna]` row-major), 2D filtrado con `if` (aplana a Vector para evitar matrices deformes), y $N \ge 3$ (producto cartesiano).
  - Soporte sintáctico dual (separador por coma `,` estilo Julia y por `for` estilo Python).
  - Aislamiento léxico estricto de variables iteradoras (anti-leak de Python) y preservación de tipado/NA homogéneo sin coerciones silenciosas (anti-R).
  - Pipeline completo verificado: `ghl-syntax` (AST, parser, fmt), `ghl-types` (inferencia estricta y chequeo de tipos), `ghl-runtime` (evaluación y pre-reserva) y suite de integración `eval_comprehensions.rs`.
- [x] **Indexación por Máscaras Booleanas y Vectores de Índices (*Boolean Mask & Fancy Indexing*):**
  - Extracción directa en vectores: `vec[vec > 0.0]` y `vec[bool_mask]` con respaldo Arrow/Polars y preservación de razones `NA`.
  - Filtrado multidimensional en matrices: `mat[bool_mask, ..]` (filas), `mat[.., bool_mask]` (columnas), `mat[bool_mask, col]` (extracción a Vector) y `mat[bool_mask_r, bool_mask_c]` (submatriz).
  - Indexación arbitraria por vectores de enteros: `mat[[2, 0], ..]` y `mat[.., [1, 3]]`.
  - Validación dimensional estricta con diagnóstico `C0202` en discrepancias de longitud.
  - Verificado en suite de integración [`eval_boolean_mask_indexing.rs`](crates/ghl-runtime/tests/eval_boolean_mask_indexing.rs).
- [x] **Sistema Unificado de Asignación Mutable In-Place (*Generalized L-Values*):**
  - [x] **Fase 1 (Colecciones Base):** Actualización en-sitio `vec[i] = val`, `mat[r, c] = val` y reemplazo de rebanadas/filas/columnas `mat[r, ..] = row_vec`, `mat[.., c] = col_vec` para variables declaradas `let mut`. Chequeo estricto de mutabilidad `C0104` en `ghl-types` y modelo CoW con `Arc::make_mut` en runtime.
  - [x] **Fase 2 (Máscaras Condicionales):** Asignación/clamping por máscara booleana `vec[vec < 0.0] = 0.0` y `mat[bool_mask, ..] = submat`, con difusión escalar (*scalar broadcast*) integrada. Verificado en [`eval_mutable_indexing.rs`](crates/ghl-runtime/tests/eval_mutable_indexing.rs).
  - [x] **Fase 3 (Campos y DataFrames):** Mutación directa de campos en structs/records `target.field = val`, columnas de DataFrame `df["col"] = vec`, `df.col = vec`, y celdas condicionales `df[mask, col] = val` con difusión escalar y preservación de `NA:reasons`.
  - [x] **Fase 4 (Operadores Compuestos):** Soporte unificado de asignación compuesta `+=`, `-=`, `*=`, `/=` aplicable a todos los L-values anteriores (escalares, vectores, rebanadas, máscaras, matrices, submatrices, campos y DataFrames). Verificado en [`eval_mutable_indexing.rs`](crates/ghl-runtime/tests/eval_mutable_indexing.rs).

### Parte B: Visualización Científica Nativa (`ghl_plot` / Grammar of Graphics)
> **Especificación de Diseño Canónica:** Ver [RFC 16: Gramática de Gráficos Reactiva, Tipada y Composicional (`ghl_plot`)](docs/design/16-reactive-typed-grammar-of-graphics.md).
>
> **Diagnóstico de Estado Actual (Auditoría del Código):**
> El sistema de gráficos existe actualmente como funciones primitivas sueltas inyectadas en el preludio (`native_plot.rs`) y con motor de dibujo acoplado monolíticamente dentro de `crates/ghl-diagnostics/src/plot.rs` (1.144 líneas con dependencia externa de `plotters`), en lugar de residir en un crate dedicado `ghl-plot` o paquete estándar `packages/ghl_plot`.
>
> - **Aspectos Bien Implementados (Aciertos):**
>   - **Renderizado Dual:** Coexistencia de renderizado vectorial/raster (`plotters` para SVG/PNG de publicación) y visualización terminal en texto/Unicode (tarjetas Cockpit Deck de alta fidelidad).
>   - **Estética de Terminal:** Histogramas con bloques Unicode verticales de 8 niveles (` `, `▂`, ..., `█`), boxplots horizontales Tukey con rango intercuartílico y detección de outliers (`*`), y scatter plots con marcos, ejes y ticks mínimos/máximos.
>   - **Pipeline Funcional Nativo:** Tuberías declarativas limpias mediante el operador `|>`.
>   - **Desacoplamiento Estadístico:** Módulo auxiliar `plot_stats.rs` que calcula de forma limpia la pendiente OLS y el resumen de 5 números fuera del motor visual.
>   - **Integración IDE:** Detección automática de variables de entorno `POSITRON_PLOTS_DIR` / `GHL_PLOTS_DIR` para exportar SVGs capturados por el visor de gráficos del IDE.
>
> - **Aspectos Mal Implementados / Deficiencias Críticas (A Corregir):**
>   - [x] **Pseudo-Gramática de Capas (Exclusión Mutua):** Resuelto en el motor cartesiano de `crates/ghl-plot`. Soporta capas simultáneas (`points` + `lines` + `smooth`) componibles mediante `+` y `|>`.
>   - [x] **`geom_line` es un cascarón incompleto:** Implementado con `LineSeries` conectadas, grosores configurables y ordenamiento por $x$.
>   - [x] **Canales Estéticos Rotos (`aes`):** `aes(color)` particiona en `DataSeries` independientes con paleta accesible Okabe-Ito y leyendas automáticas.
>   - [x] **Herencia y Datos Locales por Capa:** Permitir que capas individuales reciban su propio DataFrame local `geom_point(data = df2, aes(...))`.
>   - [x] **Ejes Discretos y Factores (Boxplots Comparativos):** Resuelto para variables categóricas (`Factor` y `String`) en boxplots comparativos múltiples (`aes(x = factor, y = num)`), calculando Tukey 5-números por grupo en Plotters, terminal y Vega.
>   - [x] **Escalas, Leyendas y Exportación Dual:** Implementado con `scale_x_log10()`, `scale_y_log10()`, `scale_x_sqrt()`, `scale_y_sqrt()`, leyendas automáticas, exportación a SVG vectorial y Vega-Lite v5 interactivo.
>   - [x] **Paneles Múltiples (Facetting):** Soporte completo para `facet_wrap()` y `facet_grid()` con fórmulas (`drv ~ cyl`) o cadenas, escalas libres (`scales = "free"`), y renderizado dual en Plotters, terminal y Vega-Lite.
>   - [x] **Acoplamiento Indebido en `ghl-diagnostics`:** Migrado 100% a `crates/ghl-plot`; `ghl-diagnostics` ya no compila `plotters` ni alberga `plot.rs`.
>   - [x] **Desconexión en el REPL:** Conectado en `crates/ghl-cli/src/repl.rs`; emite automáticamente el plot SVG a `POSITRON_PLOTS_DIR` / `GHL_PLOTS_DIR` en cada evaluación de expresión `Plot`.
>
> - **Plan de Refactorización y Evolución a `ghl_plot`:**
>   - [x] **Fase 1 (Modularización Limpia):** Mover el subsistema de gráficos desde `ghl-diagnostics` a un crate independiente `crates/ghl-plot`, desacoplando completamente `plotters` del núcleo de compilación y diagnósticos (tiempo de compilación de `ghl-diagnostics` reducido a <1s).
>   - [x] **Fase 2 (Compositor de Capas Multi-Layer):** Evaluador visual multi-capa composicional real en `crates/ghl-plot` y `eval.rs`: acumula series gráficas simultáneas sobre el mismo sistema cartesiano (`points` + `lines` + `smooth` fit) tanto con el operador `+` como con la tubería `|>`.
>   - [x] **Fase 3 (Extracción de Atributos Estéticos y Agrupación):** Conexión de `aes(color)` con los datos del DataFrame, agrupando en `DataSeries` dinámicas con la paleta accesible Okabe-Ito y leyendas automáticas.
>   - [x] **Fase 4 (Ejes Discretos y Factores):** Variables cualitativas (`Factor` y `String`) en los ejes $X$/$Y$, habilitando boxplots comparativos múltiples agrupados por categorías en Plotters, terminal y Vega.
>   - [x] **Fase 5 (Leyendas, Escalas y Exportación Dual):** Generación de leyendas en SVG y terminal, soporte para `scale_x_log10()`, `scale_y_log10()`, exportación a SVG nativo (`to_svg()`) y exportación interactiva a Vega-Lite v5 (`to_vega_json()`).
>   - [x] **Fase 6 (Sincronización Automática con IDE):** Emitir automáticamente el plot SVG a `GHL_PLOTS_DIR` / `POSITRON_PLOTS_DIR` en cada evaluación de expresión `Plot` en el REPL.
>   - [x] **Fase 7 (Cierre del Core — Datos Locales por Capa):**
>     - [x] Paneles múltiples / subgráficos categóricos (`facet_wrap("categoria")` y `facet_grid("fila ~ col")`).
>     - [x] Herencia y DataFrames locales por capa (`geom_point(data = df2, aes(...))`).

### Parte C: Interoperabilidad Python/R/Julia FFI y Empaquetado Dinámico
- [x] **Generación Automática de Módulos C-ABI ("Santísima Trinidad" Científica):**
  - Comando `ghl build <file.gh> --shared` (con selector opcional `--bridges=<py,r,jl,all>`) para exportar structs y funciones `.gh` a librerías compartidas nativas (`.so` / `.dll` / `.dylib`).
  - Generación automática de wrappers para Python (`ctypes.Structure` y funciones tipadas).
  - Generación automática de wrappers para R (constructores S3 y adaptadores `.C` con paso de punteros y desempaquetado de tipos).
  - Generación automática de módulos nativos para Julia (`module ...Bridge`, `struct` isbits inmutables, `ccall` directo LLVM con `LIB_PATH` y `export`).

### Parte D: Cuadernos Científicos y Literate Computing
- [x] **Integración Nativa con Quarto (`.qmd`) en Positron:**
  - Adopción de Quarto como el estándar oficial de computación científica reproducible en lugar de un formato propietario `.ghmd` (decisión de diseño en [`docs/roadmap/02_TODO_INTERACTIVE_QUARTO.md`](docs/roadmap/02_TODO_INTERACTIVE_QUARTO.md)).
  - Extensión oficial y filtro Lua en [`_extensions/ghl/ghl.lua`](_extensions/ghl/ghl.lua) para ejecutar chunks ````{ghl}` preservando estado entre celdas e incrustando salidas y figuras SVG.
  - Ejemplo científico completo en [`examples/quarto_report.qmd`](examples/quarto_report.qmd).
  - Soporte interactivo en Positron/VS Code para evaluar chunks con `Ctrl + Enter` hacia el REPL, Data Explorer (`view(df)`) y panel lateral de Plots.

### Parte E: Formateador Canónico y Soporte de Editor / LSP (`Alt + Shift + F`)
- [x] **Soporte Nativo de Formateo en el Language Server (`ghl-lsp`):**
  - Implementado el capability estándar `textDocument/formatting` (`document_formatting_provider: Some(OneOf::Left(true))`) y el handler asíncrono `formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>>` en `Backend`.
  - Habilitado formateo instantáneo en memoria al pulsar `Alt + Shift + F` o al guardar (`editor.formatOnSave`) en Positron, VS Code, Neovim, Zed y Helix, eliminando la necesidad de subprocesos externos síncronos (`execFileSync`).
- [x] **Preservación de Comentarios y *Trivia* en `ghl-syntax/src/fmt.rs`:**
  - Implementada extracción de *trivia* (`extract_comments`) y seguimiento de estado (`CommentState`) con conciencia de cadenas de texto y delimitadores.
  - Preserva comentarios previos de documentación (`///`), comentarios de línea (`//`), comentarios inline/posteriores en la misma línea (`let x = 10; // note`), comentarios de bloque (`/* ... */`), comentarios internos en bloques (`{ ... }`), structs, traits e impls, y comentarios al final del archivo sin pérdida de información.
  - Preservación canónica de líneas en blanco intencionales (`has_blank_line_before`), colapsando múltiples saltos redundantes a exactamente 1 línea en blanco. Idempotencia garantizada: `format(format(x)) == format(x)`.
- [x] **Formateo de Rango / Selección (*Range Formatting*):**
  - Implementado `textDocument/rangeFormatting` (`Ctrl + K, Ctrl + F` / `format_range`) mapeando rangos de líneas a sentencias AST y comentarios asociados vía `SourceIndex`, permitiendo formatear fragmentos seleccionados sin alterar el resto del archivo.

---

## Observaciones de UX / REPL e Interfaz Visual (Sesión en Vivo)

Puntos detectados durante el uso interactivo del REPL para pulir:

- [x] **Espaciado del Prompt REPL:** Separar `ghl` del kaomoji (`ghl ฅ(•⩊ •マ> ` en vez de `ghlฅ(•⩊ •マ> `) para mejorar la legibilidad del cursor.
- [x] **Comando `:var` como alias:** El REPL ahora acepta tanto `:vars`, `:var` como `:v` de manera indistinta para listar variables activas.
- [x] **Alineación tabular en `:vars`:** Reemplazado `CockpitPanel` por `CockpitTable` en `repl.rs` — nombres, tipos y valores ahora se alinean en columnas de ancho consistente por fila.
- [x] **Prompt con color ANSI en Rustyline:** Verificado contra el código fuente de `rustyline` 18.0.1 — secuencias CSI tratadas con ancho cero.
- [x] **Sugerencias de comandos desconocidos:** Distancia de Levenshtein contra la lista de comandos REPL conocidos cuando la distancia es ≤2 (ej. `:clera` → "Did you mean `:clear`?").

---

## Roadmap de Ecosistema Científico y Paquetes de Utilidad

> **Sincronización con GitHub:** Este backlog está estructurado y rastreado activamente en los [Milestones](https://github.com/ofjorque/ghl/milestones) e [Issues](https://github.com/ofjorque/ghl/issues) de GitHub.

### 1. Validación y Suites de Prueba en Paquetes de Dominio (`packages/`) — [Milestone v0.2.0](https://github.com/ofjorque/ghl/milestone/1)
- [x] **`packages/ghl_irt` (Psicometría MIRT & CAT) — [#1](https://github.com/ofjorque/ghl/issues/1):**
  - Ejecutar y consolidar tests (`ghl test`) sobre los 17 módulos de IRT (1PL, 2PL, 3PL, Graded Response, Nominal Response, MHRM, DIF, Information Curves, CAT). Validado: 22/22 tests superados.
  - Validar paridad numérica contra benchmarks de R (`mirt`) y Python (`mirt`). Documentado en [`packages/ghl_irt/GUIDE.md`](packages/ghl_irt/GUIDE.md).
  - Publicar cuaderno reproducible Quarto (`.qmd`) de calibración psicométrica completa en [`examples/irt_calibration.qmd`](examples/irt_calibration.qmd).
- [x] **`packages/ghl_causal` (Inferencia Causal y Evaluación de Impacto) — [#2](https://github.com/ofjorque/ghl/issues/2):**
  - Consolidar tests para 2x2 DiD, Event Studies dinámicos, Staggered Adoption DiD (Callaway & Sant'Anna 2021), Descomposición de Goodman-Bacon (2021), Control Sintético (Abadie et al. 2010), DiD Doblemente Robusto (Sant'Anna & Zhao 2020), IV/2SLS (Angrist-Imbens LATE) y Sensibilidad (Cinelli & Hazlett 2020). Validado: 10/10 bloques de prueba superados.
  - Validar paridad numérica y benchmarking exhaustivo contra R (`did`, `fixest`, `Synth`, `DRDID`, `AER`, `bacondecomp`), Python (`DoWhy`, `EconML`, `linearmodels`) y Julia (`CausalInference.jl`). Documentado en [`packages/ghl_causal/GUIDE.md`](packages/ghl_causal/GUIDE.md).
  - Publicar cuaderno reproducible Quarto (`.qmd`) de evaluación de impacto cuasi-experimental en [`examples/causal_inference_evaluation.qmd`](examples/causal_inference_evaluation.qmd).
- [x] **`packages/ghl_timeseries` (Series Temporales y Filtrado Dinámico) — [#3](https://github.com/ofjorque/ghl/issues/3):**
  - Consolidar tests para Raíces Unitarias (ADF, KPSS), Descomposición Estacional Clásica, Holt-Winters, Auto-ARIMA con intervalos analíticos al 95%, GARCH/GJR-GARCH con apalancamiento asimétrico, métricas de riesgo (VaR/ES), Filtro de Kalman y Suavizador Retrospectivo RTS, y VAR con causalidad de Granger. Validado: 10/10 bloques de prueba superados.
  - Validar paridad numérica y benchmarking contra R (`forecast`, `rugarch`, `KFAS`, `vars`), Python (`statsmodels`, `arch`, `filterpy`) y Julia (`StateSpaceModels.jl`, `ARCHModels.jl`, `TSAnalysis.jl`). Documentado en [`packages/ghl_timeseries/GUIDE.md`](packages/ghl_timeseries/GUIDE.md).
  - Publicar cuaderno reproducible Quarto (`.qmd`) de pronóstico temporal y filtrado dinámico en [`examples/financial_timeseries_forecasting.qmd`](examples/financial_timeseries_forecasting.qmd).

- [x] **`packages/ghl_panel` (Econometría de Datos de Panel) — [#4](https://github.com/ofjorque/ghl/issues/4):**
  - Consolidar tests para Pooled OLS, Efectos Fijos (Within), Efectos Aleatorios (GLS/Swamy-Arora), Test de Hausman, Test LM de Breusch-Pagan, Arellano-Bond Difference GMM (1991), Blundell-Bond System GMM (1998), contrastes AR(1)/AR(2), sobreidentificación de Sargan y test CD de Pesaran (2004). Validado: 10/10 bloques de prueba superados.
  - Validar paridad numérica y benchmarking contra R (`plm`, `fixest`), Python (`linearmodels`) y Julia (`FixedEffectModels.jl`). Documentado en [`packages/ghl_panel/GUIDE.md`](packages/ghl_panel/GUIDE.md).
  - Publicar cuaderno reproducible Quarto (`.qmd`) de econometría de panel dinámico en [`examples/dynamic_panel_evaluation.qmd`](examples/dynamic_panel_evaluation.qmd).

- [x] **`packages/ghl_survival` (Bioestadística y Análisis de Supervivencia) — [#5](https://github.com/ofjorque/ghl/issues/5):**
  - Consolidar tests para curvas Kaplan-Meier con intervalos de Greenwood, estimador de riesgo acumulado de Nelson-Aalen, test de Log-Rank (Mantel-Cox), modelo univariado y multivariado de Cox con Harrell C-index y LRT, test de Schoenfeld/Grambsch-Therneau para riesgos proporcionales, y modelo paramétrico de Weibull AFT. Validado: 9/9 bloques de prueba superados.
  - Validar paridad numérica y benchmarking exhaustivo contra R (`survival`, `survminer`), Python (`lifelines`, `scikit-survival`) y Julia (`Survival.jl`). Documentado en [`packages/ghl_survival/GUIDE.md`](packages/ghl_survival/GUIDE.md).
  - Publicar cuaderno reproducible Quarto (`.qmd`) de bioestadística y oncología clínica en [`examples/clinical_survival_analysis.qmd`](examples/clinical_survival_analysis.qmd).

- [x] **`packages/ghl_multilevel` (Modelos Lineales Mixtos / HLM) — [#6](https://github.com/ofjorque/ghl/issues/6):**
  - Consolidar tests para modelos con Interceptos Aleatorios, Pendientes Aleatorias (*Random Slopes*), estimación de covarianza intercepto-pendiente ($\tau_{01}$) y correlación ($\rho$), BLUPs empíricos de Bayes, $R^2$ Marginal y Condicional de Nakagawa & Schielzeth (2013), test LRT de estructura aleatoria $\chi^2(2)$, y Efecto de Diseño de Kish (Deff). Validado: 8/8 bloques de prueba superados.
  - Validar paridad numérica y benchmarking exhaustivo contra R (`lme4`, `nlme`), Python (`statsmodels.mixedlm`) y Julia (`MixedModels.jl`). Documentado en [`packages/ghl_multilevel/GUIDE.md`](packages/ghl_multilevel/GUIDE.md).
  - Publicar cuaderno reproducible Quarto (`.qmd`) de modelado jerárquico longitudinal en [`examples/multilevel_hierarchical_modeling.qmd`](examples/multilevel_hierarchical_modeling.qmd).

- [x] **`packages/spring_pact` (Protocolos de Expectativas y Contratos Estadísticos - RFC 14) — [#7](https://github.com/ofjorque/ghl/issues/7):**
  - Consolidar protocolos de expectativas sobre datos (integridad de esquema, unicidad de identificadores, rangos, NA, outliers via IQR) y modelos (tamaño muestral, VIF, vcov robusta), hashing criptográfico SHA-256 del contrato, saneamiento `enforce_data_rules`, verificación formal `verify_contract` y certificados Cockpit Deck con Haru (`/ᐠ˵- ⩊ -˵マ ✧ CERTIFIED`). Validado: 6/6 bloques de prueba superados.
  - Validar paridad funcional y benchmarking contra R (`pointblank`, `assertr`) y Python (`Great Expectations`, `Pandera`). Documentado en [`packages/spring_pact/GUIDE.md`](packages/spring_pact/GUIDE.md).
  - Publicar cuaderno reproducible Quarto (`.qmd`) de auditoría estadística y contratos de datos en [`examples/data_contracts_and_expectations.qmd`](examples/data_contracts_and_expectations.qmd).



### 2. Nuevos Paquetes de Utilidad Propuestos para el Ecosistema — [Milestone v0.3.0](https://github.com/ofjorque/ghl/milestone/2)
- [x] **`packages/ghl_optim` (Optimización Numérica No Lineal General) — [#8](https://github.com/ofjorque/ghl/issues/8):**
  - Implementar biblioteca de algoritmos de optimización matemática (BFGS, L-BFGS-B con cotas de caja, Nelder-Mead simplex, Levenberg-Marquardt para mínimos cuadrados no lineales). Validado: 5/5 bloques de prueba superados.
  - Exponer interfaz canónica para estimación de funciones de pérdida personalizadas, máxima verosimilitud (MLE) con errores estándar asintóticos vía matriz de información de Fisher, AIC/BIC, y paneles terminales Cockpit Deck (`optim_cockpit`, `mle_cockpit`, `nls_cockpit`). Documentado en [`packages/ghl_optim/GUIDE.md`](packages/ghl_optim/GUIDE.md).
  - Publicar cuaderno reproducible Quarto (`.qmd`) de optimización y calibración MLE en [`examples/nonlinear_optimization_and_mle.qmd`](examples/nonlinear_optimization_and_mle.qmd).
- [ ] **`packages/ghl_impute` (Limpieza Científica y Datos Faltantes) — [#9](https://github.com/ofjorque/ghl/issues/9):**
  - Algoritmos de imputación múltiple (tipo MICE y k-NN) integrados nativamente con la semántica de `NA:reason` de GHL.
- [ ] **`packages/ghl_db` / Conectores SQL Extendido — [#10](https://github.com/ofjorque/ghl/issues/10):**
  - Conector nativo hacia DuckDB / SQLite para consultas SQL analíticas directamente sobre DataFrames y archivos Parquet.

### 3. Developer Experience (DX), CI/CD y Distribución — [Milestone v0.4.0](https://github.com/ofjorque/ghl/milestone/3)
- [ ] **GitHub Actions Matrix Multi-Plataforma — [#11](https://github.com/ofjorque/ghl/issues/11):**
  - Workflows automatizados de compilación y pruebas para Ubuntu, macOS y Windows con cacheo de dependencias.
- [ ] **Pipeline de Releases Automatizados y Empaquetado VSIX — [#12](https://github.com/ofjorque/ghl/issues/12):**
  - Empaquetado automático de binarios optimizados (Linux, macOS, Windows) y extensión VS Code / Positron al publicar tags `v*.*.*`.
- [ ] **Generador de Documentación HTML Estática (`ghl doc`) — [#13](https://github.com/ofjorque/ghl/issues/13):**
  - Extracción de comentarios `///` y renderizado de sitio web estático moderno con fórmulas KaTeX y buscador en cliente.
