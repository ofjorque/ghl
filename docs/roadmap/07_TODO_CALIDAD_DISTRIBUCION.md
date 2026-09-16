# Roadmap — Formateo Canónico, Fuzzing del Compilador, CI/CD y Distribución

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre la fase original 10 — la única fase de higiene de ingeniería/release, sin relación directa con el lenguaje o la estadística.

Cerrar la brecha de calidad de ingeniería y entrega continua del lenguaje:

- [x] **Formateador Canónico de Código (`ghl fmt`):**
  - [x] Subcomando `ghl fmt <file.gh|file.ghl>` que utiliza el pretty-printer del AST para normalizar automáticamente la indentación a 4 espacios y los saltos de línea canónicos.
  - [x] Bandera `--check` para integración en pipelines de CI (falla si el archivo no está formateado) y soporte stdin (`ghl fmt -`).
  - [x] Integración con Positron / VS Code para formateo automático al guardar (`editor.formatOnSave` vía `DocumentFormattingEditProvider`).
- [x] **Fuzz Testing del Compilador (`cargo-fuzz` / AFL):**
  - [x] Generación de entradas de bytes pseudoaleatorias y sintaxis maliciosa contra `ghl-syntax` y `ghl-types` (`crates/ghl-types/tests/fuzz_robustness.rs` y `fuzz/`).
  - [x] Invariante: Garantizar $0$ *panics*, $0$ *crashes* y $0$ desbordamientos de búfer ante cualquier código malformado, retornando siempre diagnósticos limpios.
- [x] **Telemetría Cockpit Deck en `fmt`/Fuzzing (hueco detectado en auditoría 2026-09-15):**
  - [x] `ghl fmt --check` emite paneles Cockpit Deck (RFC 12) con badge Haru, métricas de archivos escaneados, líneas, duración y lista de archivos con diff.
- [x] **CI/CD Automatizado con GitHub Actions:**
  - [x] Matriz de compilación y tests automáticos en cada pull request para **Windows**, **Linux** y **macOS** (`.github/workflows/ci.yml`).
  - [x] Alerta automatizada de regresión de rendimiento: Notificar si algún commit degrada los benchmarks empíricos más de un 5%.
- [x] **Empaquetado y Distribución Automatizada:**
  - [x] Script de instalación en una línea para entornos Unix/Linux: `curl -fsSL https://raw.githubusercontent.com/ofjorque/ghl/main/install.sh | sh` ([`install.sh`](../../install.sh)).
  - [x] Manifiesto de instalación para Windows (`winget` en [`distribution/winget/ghl.yaml`](../../distribution/winget/ghl.yaml) y script PowerShell automatizado [`install.ps1`](../../install.ps1)).
  - [x] Publicación automática de binarios precompilados y del paquete `.vsix` en los Releases de GitHub (`.github/workflows/release.yml`).

