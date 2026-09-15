# Roadmap — Formateo Canónico, Fuzzing del Compilador, CI/CD y Distribución

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre la fase original 10 — la única fase de higiene de ingeniería/release, sin relación directa con el lenguaje o la estadística.

Cerrar la brecha de calidad de ingeniería y entrega continua del lenguaje:

- [ ] **Formateador Canónico de Código (`ghl fmt`):**
  - [ ] Subcomando `ghl fmt <file.gh|file.ghl>` que utiliza el pretty-printer del AST para normalizar automáticamente la indentación a 4 espacios y los saltos de línea canónicos.
  - [ ] Bandera `--check` para integración en pipelines de CI (falla si el archivo no está formateado).
  - [ ] Integración con Positron / VS Code para formateo automático al guardar (`editor.formatOnSave`).
- [ ] **Fuzz Testing del Compilador (`cargo-fuzz` / AFL):**
  - [ ] Generación de entradas de bytes pseudoaleatorias y sintaxis maliciosa contra `ghl-syntax` y `ghl-types`.
  - [ ] Invariante: Garantizar $0$ *panics*, $0$ *crashes* y $0$ desbordamientos de búfer ante cualquier código malformado, retornando siempre diagnósticos limpios.
- [ ] **Telemetría Cockpit Deck en `fmt`/Fuzzing (hueco detectado en auditoría 2026-09-15):**
  - [ ] `ghl fmt --check` y las corridas de `cargo-fuzz`/AFL no emiten paneles Cockpit Deck (RFC 12) — hoy los únicos consumidores son `ghl run`, `ghl check`, `summary(model)` y `ghl repl`. Agregar telemetría consistente ayuda a diagnosticar fallos de formateo/fuzzing con el mismo lenguaje visual del resto del CLI.
- [ ] **CI/CD Automatizado con GitHub Actions:**
  - [ ] Matriz de compilación y tests automáticos en cada pull request para **Windows** y **Linux** (con runners automáticos de macOS en la nube para releases sin requerir hardware Mac local).
  - [ ] Alerta automatizada de regresión de rendimiento: Notificar si algún commit degrada los benchmarks empíricos más de un 5%.
- [ ] **Empaquetado y Distribución Automatizada:**
  - [ ] Script de instalación en una línea para entornos Unix/Linux: `curl -fsSL https://ghl-lang.org/install.sh | sh`.
  - [ ] Manifiesto de instalación para Windows (`winget` o script PowerShell automatizado).
  - [ ] Publicación automática de binarios precompilados y del paquete `.vsix` en los Releases de GitHub.
