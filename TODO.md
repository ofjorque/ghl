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
- [ ] **Comprensión de Vectores / Listas (*Vector Comprehensions*):**
  - Sintaxis canónica `[expr for var in iterable]` y `[expr for var in iterable if condition]`.
  - Soporte en `ghl-syntax` (parser de corchetes diferenciando literales de comprensión), AST (`ExprKind::Comprehension`), `ghl-types` (inferencia de tipo elemento) y `ghl-runtime` (evaluación eficiente con pre-reserva de capacidad).
- [ ] **Indexación por Máscaras Booleanas (*Boolean Mask Indexing*):**
  - Extracción directa mediante vectores de booleanos: `vec[vec > 0.0]` y filtrado de matrices por filas `mat[bool_mask, ..]`.
- [ ] **Asignación Indexada Mutable Directa:**
  - Sintaxis de actualización en-sitio `vec[i] = val` y `mat[i, j] = val` para variables mutables (`let mut`), eliminando la necesidad de llamar a `vec = set(vec, i, val)`.

### Parte B: Visualización Científica Nativa (`ghl_plot` / Grammar of Graphics)
- [ ] **Diseño del DSL de Gráficos:**
  - Sintaxis declarativa inspirada en *ggplot2* y *Vega-Lite*: `plot(df) |> geom_point(x: "th", y: "prob") |> geom_line() |> title("ICC Item 1")`.
- [ ] **Motor de Renderizado Vectorial Autónomo:**
  - Generación de SVG estático nítido y HTML interactivo autónomo sin dependencias pesadas de navegadores externos.

### Parte C: Interoperabilidad Python/R FFI y Empaquetado Dinámico
- [ ] **Generación Automática de Módulos C-ABI:**
  - Comando `ghl build <file.gh> --shared` para exportar structs y funciones `.gh` a librerías compartidas (`.so` / `.dll`).
  - Generación automática de wrappers para Python (`ctypes`/`cffi`) y R (`.Call`).

### Parte D: Cuadernos Científicos y Literate Computing
- [ ] **Formato de Documento Reproducible (`.ghmd`):**
  - Ejecutor de documentos Markdown con bloques de código ````ghl ... ```` que evalúa celdas e incrusta salidas de texto, tablas Cockpit y gráficos SVG.

---

## Observaciones de UX / REPL e Interfaz Visual (Sesión en Vivo)

Puntos detectados durante el uso interactivo del REPL para pulir:

- [x] **Espaciado del Prompt REPL:** Separar `ghl` del kaomoji (`ghl ฅ(•⩊ •マ> ` en vez de `ghlฅ(•⩊ •マ> `) para mejorar la legibilidad del cursor.
- [x] **Comando `:var` como alias:** El REPL ahora acepta tanto `:vars`, `:var` como `:v` de manera indistinta para listar variables activas.
- [x] **Alineación tabular en `:vars`:** Reemplazado `CockpitPanel` por `CockpitTable` en `repl.rs` — nombres, tipos y valores ahora se alinean en columnas de ancho consistente por fila.
- [x] **Prompt con color ANSI en Rustyline:** Verificado contra el código fuente de `rustyline` 18.0.1 — secuencias CSI tratadas con ancho cero.
- [x] **Sugerencias de comandos desconocidos:** Distancia de Levenshtein contra la lista de comandos REPL conocidos cuando la distancia es ≤2 (ej. `:clera` → "Did you mean `:clear`?").
