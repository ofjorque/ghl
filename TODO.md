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
>   - [ ] **Pseudo-Gramática de Capas (Exclusión Mutua):** `draw_chart()` y `render()` usan un condicional en cascada (`if is_hist { ... } else if is_box { ... } else { scatter }`). Es imposible componer múltiples capas reales (e.g. histograma + densidad, boxplot + jitter de puntos, múltiples series superpuestas).
>   - [ ] **`geom_line` es un cascarón incompleto:** `GeomKind::Line` está definido en el enum pero nunca es procesado por el backend de `plotters` ni por el renderizador de terminal; el gráfico cartesiano siempre dibuja puntos incondicionalmente.
>   - [ ] **Canales Estéticos Rotos (`aes`):** Aunque `aes()` acepta `color`, la función `native_plot` nunca extrae los datos de la columna `color` del DataFrame; `PlotSpec` carece de almacenamiento para atributos de color/grupo; los puntos se dibujan siempre con un color fijo hardcodeado sin paletas ni agrupación.
>   - [ ] **Falta de Herencia y Datos Locales por Capa:** Las funciones `geom_*` solo reciben `PlotSpec` como primer argumento en la tubería. No pueden recibir parámetros propios (`alpha`, `size`, `shape`, `color`, `linetype`) ni un `DataFrame` local para sobreescribir los datos globales del gráfico.
>   - [ ] **Restricción a Datos Numéricos Continuos:** `PlotSpec` solo almacena `Vec<f64>`. No existe soporte para ejes cartesianos discretos basados en `Factor`, strings o fechas (salvo de forma aislada en el conteo simple de `geom_bar`).
>   - [ ] **Ausencia de Escalas, Leyendas y Coordenadas:** Cero soporte para `scale_*` (escalas continuas/discretas, Viridis, log10), generación de leyendas visuales (SVG/terminal), `coord_flip()` o transformaciones de coordenadas.
>   - [ ] **Falta de Paneles Múltiples (Facetting):** Sin soporte para `facet_wrap()` ni `facet_grid()` para particionar gráficos por categorías.
>   - [ ] **Acoplamiento Indebido en `ghl-diagnostics`:** El crate de diagnósticos y formateo de errores no debería albergar el motor de gráficos SVG ni compilar `plotters`; debe migrarse a `crates/ghl-plot` o paquete autónomo `packages/ghl_plot`.
>   - [ ] **Desconexión en el REPL:** La evaluación directa de una expresión de tipo `Plot` solo imprime la tarjeta de terminal; el archivo SVG para Positron solo se escribe si se invoca explícitamente `show(p)`.
>
> - **Plan de Refactorización y Evolución a `ghl_plot`:**
>   - [x] **Fase 1 (Modularización Limpia):** Mover el subsistema de gráficos desde `ghl-diagnostics` a un crate independiente `crates/ghl-plot`, desacoplando completamente `plotters` del núcleo de compilación y diagnósticos (tiempo de compilación de `ghl-diagnostics` reducido a <1s).
>   - [x] **Fase 2 (Compositor de Capas Multi-Layer):** Evaluador visual multi-capa composicional real en `crates/ghl-plot` y `eval.rs`: acumula series gráficas simultáneas sobre el mismo sistema cartesiano (`points` + `lines` + `smooth` fit) tanto con el operador `+` como con la tubería `|>`.
>   - [x] **Fase 3 (Extracción de Atributos Estéticos y Agrupación):** Conexión de `aes(color)` con los datos del DataFrame, agrupando en `DataSeries` dinámicas con la paleta accesible Okabe-Ito y leyendas automáticas.
>   - [ ] **Fase 4 (Ejes Discretos y Factores):** Permitir variables cualitativas (`Factor` y `String`) en los ejes $X$/$Y$, habilitando boxplots comparativos múltiples (`y ~ factor`) y gráficos de dispersión categórica.
>   - [x] **Fase 5 (Leyendas, Escalas y Exportación Dual):** Generación de leyendas en SVG y terminal, soporte para `scale_x_log10()`, `scale_y_log10()`, exportación a SVG nativo (`to_svg()`) y exportación interactiva a Vega-Lite v5 (`to_vega_json()`).
>   - [ ] **Fase 6 (Sincronización Automática con IDE):** Emitir automáticamente el plot SVG a `GHL_PLOTS_DIR` en cada evaluación de expresión `Plot` en el REPL.

### Parte C: Interoperabilidad Python/R FFI y Empaquetado Dinámico
- [ ] **Generación Automática de Módulos C-ABI:**
  - Comando `ghl build <file.gh> --shared` para exportar structs y funciones `.gh` a librerías compartidas (`.so` / `.dll`).
  - Generación automática de wrappers para Python (`ctypes`/`cffi`) y R (`.Call`).

### Parte D: Cuadernos Científicos y Literate Computing
- [ ] **Formato de Documento Reproducible (`.ghmd`):**
  - Ejecutor de documentos Markdown con bloques de código ````ghl ... ```` que evalúa celdas e incrusta salidas de texto, tablas Cockpit y gráficos SVG.

### Parte E: Formateador Canónico y Soporte de Editor / LSP (`Alt + Shift + F`)
- [ ] **Soporte Nativo de Formateo en el Language Server (`ghl-lsp`):**
  - Implementar el capability estándar `textDocument/formatting` (`document_formatting_provider: Some(OneOf::Left(true))`) y el handler asíncrono `formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>>` en `Backend`.
  - Habilitar formateo instantáneo en memoria al pulsar `Alt + Shift + F` o al guardar (`editor.formatOnSave`) en Positron, VS Code, Neovim, Zed y Helix, eliminando la necesidad de subprocesos externos síncronos (`execFileSync`).
- [ ] **Preservación de Comentarios y *Trivia* en `ghl-syntax/src/fmt.rs`:**
  - El formateador actual (`format_program`) genera código canónico reescribiendo desde el AST puro, lo que descarta comentarios de código (`//`, `/* ... */`) y saltos de línea manuales que no están fijados a nodos del AST.
  - Implementar preservación de *trivia* (comentarios y espaciado intencional) en el lexer/parser o formateo consciente de tokens para garantizar que el formateo nunca destruya anotaciones del desarrollador.
- [ ] **Formateo de Rango / Selección (*Range Formatting*):**
  - Implementar `textDocument/rangeFormatting` (`Ctrl + K, Ctrl + F`) para dar formato únicamente al fragmento de código resaltado por el cursor sin alterar el resto del archivo.

---

## Observaciones de UX / REPL e Interfaz Visual (Sesión en Vivo)

Puntos detectados durante el uso interactivo del REPL para pulir:

- [x] **Espaciado del Prompt REPL:** Separar `ghl` del kaomoji (`ghl ฅ(•⩊ •マ> ` en vez de `ghlฅ(•⩊ •マ> `) para mejorar la legibilidad del cursor.
- [x] **Comando `:var` como alias:** El REPL ahora acepta tanto `:vars`, `:var` como `:v` de manera indistinta para listar variables activas.
- [x] **Alineación tabular en `:vars`:** Reemplazado `CockpitPanel` por `CockpitTable` en `repl.rs` — nombres, tipos y valores ahora se alinean en columnas de ancho consistente por fila.
- [x] **Prompt con color ANSI en Rustyline:** Verificado contra el código fuente de `rustyline` 18.0.1 — secuencias CSI tratadas con ancho cero.
- [x] **Sugerencias de comandos desconocidos:** Distancia de Levenshtein contra la lista de comandos REPL conocidos cuando la distancia es ≤2 (ej. `:clera` → "Did you mean `:clear`?").
