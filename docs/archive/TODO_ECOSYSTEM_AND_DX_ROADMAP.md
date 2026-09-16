# TODO — Roadmap de Ecosistema, Herramientas y Experiencia de Desarrollo (DX) [ARCHIVADO]

Este documento contiene el registro histórico y consolidado de los 7 roadmaps temáticos de ecosistema, herramientas de desarrollo, validación empírica y documentación de usuario de **GHL**, completados al 100% el **16 de septiembre de 2026**.

> **Histórico Previo:**
> - El roadmap normativo de especificación y paridad de diseño por RFC (RFC 00 a RFC 13) se encuentra completado y archivado en [`docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md`](TODO_RFC_COMPLIANCE_ROADMAP.md).
> - El roadmap de aceleración de kernel, paralelismo y suites empíricas se encuentra archivado en [`docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md`](TODO_PHASES_0_TO_10_BENCHMARKS.md).

---

## Resumen de Fases Archivadas

| Roadmap | Archivo Temático | Estado | Hito Principal |
|---|---|:---:|---|
| **01** | [`01_TODO_EDITOR_LSP.md`](../roadmap/01_TODO_EDITOR_LSP.md) | **[x] 100%** | Extensión VSIX empaquetada, TextMate, snippets, servidor LSP stdio |
| **02** | [`02_TODO_INTERACTIVE_QUARTO.md`](../roadmap/02_TODO_INTERACTIVE_QUARTO.md) | **[x] 100%** | REPL multilínea con inspección, evaluación Quarto, Data Explorer |
| **03** | [`03_TODO_VALIDACION_NUMERICA_Y_PROFUNDA.md`](../roadmap/03_TODO_VALIDACION_NUMERICA_Y_PROFUNDA.md) | **[x] 100%** | NIST StRD, paridad OLS/GLM con R, contrastes, snapshots SVG |
| **04** | [`04_TODO_BENCHMARKS_RENDIMIENTO.md`](../roadmap/04_TODO_BENCHMARKS_RENDIMIENTO.md) | **[x] 100%** | TPC-H Q1/Q6, H2O GroupBy/Join, CLBG k-nucleotide / fasta |
| **05** | [`05_TODO_VALIDACION_DATOS_REALES.md`](../roadmap/05_TODO_VALIDACION_DATOS_REALES.md) | **[x] 100%** | 11M Higgs (UCI), 2.38M Airline (Harvard), scRNA-seq, IMDb, LOB |
| **06** | [`06_TODO_DOCS_COOKBOOK.md`](../roadmap/06_TODO_DOCS_COOKBOOK.md) | **[x] 100%** | Guías R/Python, Quickstart, Cookbook 5 recetas, `ghl doc`, hover |
| **07** | [`07_TODO_CALIDAD_DISTRIBUCION.md`](../roadmap/07_TODO_CALIDAD_DISTRIBUCION.md) | **[x] 100%** | `ghl fmt --check`, fuzzing AST/robustez, CI/CD de distribución |

---

## Detalle Consolidado de los Roadmaps Temáticos

### Roadmap 01: Editor (Positron / VS Code) + Language Server Protocol
- [x] Extensión empaquetada para Positron IDE y VS Code (`editors/vscode/ghl-lang-0.1.0.vsix`).
- [x] Gramática TextMate con resaltado léxico para palabras clave estadísticas, tipos, operadores y bloques de fórmulas (`grammar/ghl.tmLanguage.json`).
- [x] Snippets de código para flujos comunes: `df`, `fn`, `ols`, `glm`, `test`, `plot`.
- [x] Servidor LSP completo por stdio (`ghl lsp`): diagnóstico en tiempo real, autocompletado, hover tooltips y resolución de símbolos.

### Roadmap 02: Computación Interactiva en Positron
- [x] Shell interactivo REPL avanzado (`ghl repl`): entrada multilínea, coloreado de sintaxis ANSI, historial de comandos, comandos internos (`:help`, `:vars`, `:clear`, `:doc`) y consulta en vivo con `?<fun>`.
- [x] Soporte para cómputo reproducible y renderizado de bloques de código en Quarto (`.qmd`).
- [x] Data Explorer interactivo y exportación columnar en terminal para DataFrames y matrices.

### Roadmap 03: Validación Cruzada de Precisión Numérica y Pruebas Profundas
- [x] Certificación numérica NIST StRD con datasets de alta dificultad (Pontius, Wampler1, Wampler2, Longley, Filippelli) con error relativo $< 10^{-10}$.
- [x] Validación cruzada de estimadores OLS y GLM logísticos frente a referencias oficiales de R (`stats::lm`, `stats::glm`).
- [x] Matrices de contraste categórico (`Contrast::Treatment`, `Contrast::Sum`, `Contrast::Helmert`, `Contrast::Polynomial`) integradas con `factor()` y fórmulas.
- [x] Pruebas de regresión visual de snapshots SVG para gráficos estadísticos (`geom_point`, `geom_line`, `geom_histogram`, `geom_boxplot`, `geom_bar`) y temas (`theme_minimal`, `theme_classic`, `theme_dark`).

### Roadmap 04: Benchmarks de Rendimiento (DataFrames Abiertos + CLBG)
- [x] Benchmarks H2O GroupBy y Hash Joins evaluados en pipelines eager y lazy (`scan_csv`, `scan_parquet`).
- [x] Consultas analíticas estándar TPC-H (Q1 agregación masiva, Q6 escaneo con predicados selectivos).
- [x] Retos algorítmicos The Computer Language Benchmarks Game (CLBG): `fasta` y `k-nucleotide` multihilo con `rayon`.

### Roadmap 05: Validación Extensiva con Diversidad Real de DataFrames
- [x] **Higgs Boson (CERN / UCI)**: Ingesta del dataset auténtico de 11,000,000 filas × 29 columnas (8.03 GB en disco), normalización z-score y clasificación logística IRLS (`val_01_higgs_kinematics.ghl`).
- [x] **Airline On-Time (Harvard Dataverse / ASA Data Expo)**: Ingesta de 2,389,217 registros de vuelos reales con joins relacionales contra catálogos de 3,376 aeropuertos y 1,491 aerolíneas, y agrupaciones multi-clave a >8M filas/s (`val_02_airline_ontime.ghl`).
- [x] **Genómica scRNA-seq**: Manejo de matrices dispersas ultragrandes, dropouts, normalización CPM + $\ln(1 + \text{CPM})$, HVG y transposición invariante (`val_03_genomics_scrna.ghl`).
- [x] **IMDb Textual Reviews**: Vectorización de texto con Unicode arbitrario, acentos, tildes, signos y emojis multi-byte sin corrupción de memoria (`val_04_imdb_textual.ghl`).
- [x] **Datos Financieros LOB**: Series temporales a nivel de microsegundo, ventanas móviles, spreads bid-ask en bps y volatilidad realizada (`val_05_lob_financial.ghl`).

### Roadmap 06: Documentación de Usuario, Guías de Migración y Cookbook
- [x] Guía de migración para usuarios de R (`docs/guides/ghl_for_r_users.md`).
- [x] Guía de migración para usuarios de Python / Pandas (`docs/guides/ghl_for_python_users.md`).
- [x] Tutorial rápido "GHL en 15 Minutos" (`docs/guides/quickstart.md`).
- [x] Statistical Cookbook con 5 recetas completas (`docs/guides/cookbook.md`).
- [x] Motor generador de documentación `ghl doc` (Markdown y HTML interactivo con cobertura).
- [x] Tooltips de documentación enriquecidos en LSP hover y REPL `?fun`.

### Roadmap 07: Formateo Canónico, Fuzzing del Compilador, CI/CD y Distribución
- [x] Formateador canónico `ghl fmt` y verificador `ghl fmt --check` con emisión de paneles Cockpit Deck.
- [x] Fuzzing continuo del lexer, parser y lowering con generador pseudoaleatorio de mutaciones de bytes.
- [x] Scripts multiplataforma de instalación rápida (`install.sh`, `install.ps1`).
- [x] Pipeline automatizado de CI/CD para compilación, testing y empaquetado de binarios release.

---

## Brechas de Auditoría Cerradas (2026-09-15)
- [x] Matrices de contraste (`Contrast::Treatment/Sum/Helmert/Polynomial`) implementadas en Neko y probadas en modelos.
- [x] Backend PNG/SVG (`plotters`) para `geom_boxplot` y `geom_bar`.
- [x] Sistema de temas (`theme_minimal()`, `theme_classic()`, `theme_dark()`).
- [x] Telemetría Cockpit Deck en GMM/EM, operaciones tabulares largas y `fmt --check`.
- [x] Descarga automatizada de datasets masivos reales en carpeta gitignored (`validation/data/`).
