# Roadmap — Computación Interactiva en Positron (REPL, Quarto, Data Explorer)

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre la fase original 3, **sin** kernel Jupyter ni sistema de notebooks propio — decisión explícita de no construir esa pieza por ahora.

Habilitar la experiencia de computación científica y *Literate Programming* nativa en Positron, apoyándose en lo que Positron/VS Code ya ofrecen en lugar de reimplementar un protocolo de notebooks:

- [ ] **Envío Interactivo al REPL (`Ctrl + Enter`):**
  - [ ] Comando en la extensión para enviar línea actual o bloque seleccionado al REPL de GHL en la terminal integrada.
- [ ] **Soporte Quarto (`.qmd`):**
  - [ ] Habilitar chunks ejecutables ````{ghl}`` en Quarto.
  - [ ] Generación de documentos reproducibles (HTML, PDF, Typst) combinando prosa, código, tablas y gráficos.
- [ ] **Visor de Datos Nativo de Positron (*Data Explorer*):**
  - [ ] Integración con la API de exploración tabular de Positron (`View(df)`).
  - [ ] Transmisión de DataFrames mediante Apache Arrow IPC sin copia de memoria para exploración interactiva en cuadrícula con filtros y ordenamiento.
- [ ] **Panel Gráfico Lateral (*Plots Pane*):**
  - [ ] Salida de gráficos de *Grammar of Graphics* en SVG para visualización inmediata en la pestaña de Plots de Positron.
