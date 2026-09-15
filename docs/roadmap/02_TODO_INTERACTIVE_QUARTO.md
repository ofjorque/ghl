# Roadmap — Computación Interactiva en Positron (REPL, Quarto, Data Explorer)

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre la fase original 3, **sin** kernel Jupyter ni sistema de notebooks propio — decisión explícita de no construir esa pieza por ahora.

Habilitar la experiencia de computación científica y *Literate Programming* nativa en Positron, apoyándose en lo que Positron/VS Code ya ofrecen en lugar de reimplementar un protocolo de notebooks:

- [x] **Envío Interactivo al REPL (`Ctrl + Enter`):**
  - [x] Comando en la extensión para enviar línea actual o bloque seleccionado al REPL de GHL en la terminal integrada.
- [x] **Soporte Quarto (`.qmd`):**
  - [x] Habilitar chunks ejecutables ````{ghl}`` en Quarto.
  - [x] Generación de documentos reproducibles (HTML, PDF, Typst) combinando prosa, código, tablas y gráficos.
- [x] **Visor de Datos Nativo de Positron (*Data Explorer*):**
  - [x] Integración con la API de exploración tabular de Positron (`view(df)`).
  - [x] Transmisión de DataFrames mediante Apache Arrow IPC sin copia de memoria para exploración interactiva en cuadrícula con filtros y ordenamiento.
- [x] **Panel Gráfico Lateral (*Plots Pane*):**
  - [x] Salida de gráficos de *Grammar of Graphics* en SVG para visualización inmediata en la pestaña de Plots de Positron.
