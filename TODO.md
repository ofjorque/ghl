# TODO — Roadmap de Ecosistema, Herramientas y Experiencia de Desarrollo (DX)

Este documento es el índice activo del trabajo pendiente de GHL. Se mantiene deliberadamente corto: el backlog completo vive dividido por tema en [`docs/roadmap/`](docs/roadmap/), y las tareas se van "sacando" a la sección **En Trabajo Activo** de este archivo a medida que se empiezan.

> **Histórico de Fases Anteriores:**
> - El roadmap normativo de especificación y paridad de diseño por RFC (RFC 00 a RFC 13) se encuentra completado y archivado en [`docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md`](docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md).
> - El roadmap de aceleración de kernel, paralelismo y suites empíricas se encuentra archivado en [`docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md`](docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md).

---

## Backlog por Tema (`docs/roadmap/`)

1. [Editor (Positron / VS Code) + Language Server Protocol](docs/roadmap/01_TODO_EDITOR_LSP.md)
2. [Computación Interactiva en Positron (REPL, Quarto, Data Explorer)](docs/roadmap/02_TODO_INTERACTIVE_QUARTO.md)
3. [Validación Cruzada de Precisión Numérica y Pruebas Profundas del Sistema](docs/roadmap/03_TODO_VALIDACION_NUMERICA_Y_PROFUNDA.md)
4. [Benchmarks de Rendimiento (DataFrames Abiertos + Retos CLBG)](docs/roadmap/04_TODO_BENCHMARKS_RENDIMIENTO.md)
5. [Validación Extensiva con Diversidad Real de DataFrames](docs/roadmap/05_TODO_VALIDACION_DATOS_REALES.md)
6. [Documentación de Usuario, Guías de Migración y Cookbook](docs/roadmap/06_TODO_DOCS_COOKBOOK.md)
7. [Formateo Canónico, Fuzzing del Compilador, CI/CD y Distribución](docs/roadmap/07_TODO_CALIDAD_DISTRIBUCION.md)

> **Nota de alcance:** no se construirá un sistema de notebooks propio (ni kernel Jupyter). El documento 2 se apoya en lo que Positron/VS Code ya ofrecen (REPL, Quarto, Data Explorer, Plots Pane).

---

## En Trabajo Activo

**Auditoría 2026-09-15 — brechas de implementación en Neko, Cockpit y Gramática de Gráficos** (código vs. RFC 09 / RFC 11 / RFC 12):

- [ ] Matrices de contraste (`Contrast::Treatment/Sum/Helmert/Polynomial`) para fórmulas con `Factor`/`OrderedFactor` — no implementadas. Ver [`03_TODO_VALIDACION_NUMERICA_Y_PROFUNDA.md`](docs/roadmap/03_TODO_VALIDACION_NUMERICA_Y_PROFUNDA.md#parte-c-brechas-de-implementación-detectadas--neko-cockpit-y-gramática-de-gráficos-auditoría-2026-09-15).
- [ ] Backend PNG/SVG (`plotters`) incompleto: `geom_boxplot` y `geom_bar` solo renderizan en terminal. Mismo enlace que arriba.
- [ ] Sistema de temas (`theme_minimal()`) inexistente en `std::plot`. Mismo enlace que arriba.
- [ ] Cockpit Deck sin cobertura en: convergencia de GMM/EM (ver doc 03), operaciones largas de DataFrame para benchmarks H2O/TPC-H (ver [`04_TODO_BENCHMARKS_RENDIMIENTO.md`](docs/roadmap/04_TODO_BENCHMARKS_RENDIMIENTO.md)), y `ghl fmt --check`/fuzzing (ver [`07_TODO_CALIDAD_DISTRIBUCION.md`](docs/roadmap/07_TODO_CALIDAD_DISTRIBUCION.md)).
- [ ] Descarga externa de BBDD masivas (11M Higgs, 120M Airline) en carpeta gitignored (`validation/data/`) para estrés de disco/RAM a escala de producción (Roadmap 05).
