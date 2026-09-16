# Roadmap — Validación Extensiva con Diversidad Real de DataFrames

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre la fase original 9, separada de los benchmarks sintéticos porque aquí el objetivo es volumen y diversidad estructural del mundo real, no un número de referencia estandarizado.

Auditar el comportamiento del motor columnar de GHL frente a la complejidad estructural y tipos de datos del mundo real:

- [x] **Higgs Boson Dataset (CERN / UCI):**
  - [x] Volumen: Filas densas de punto flotante `f64` (variables cinemáticas continuas: $p_T$, $\text{MET}$, $m_{jj}$, $\eta$).
  - [x] Prueba: Ingesta, normalización estadística z-score y clasificación logística IRLS (`val_01_higgs_kinematics.ghl`).
- [x] **Airline On-Time Performance (ASA Data Expo):**
  - [x] Volumen: Registros de vuelos históricos y diversidad de aeropuertos/transportistas.
  - [x] Prueba: Cardinalidad en variables categóricas, `group_by` multi-clave (`carrier`, `origin`), y joins relacionales (`inner_join`, `left_join`) con tablas maestras de aeropuertos y aerolíneas (`val_02_airline_ontime.ghl`).
- [x] **Genómica y Single-Cell RNA-seq (Matrices Ultra-Anchas):**
  - [x] Estructura: Matrices transponibles de alta dimensionalidad ($G$ genes $\times C$ células).
  - [x] Prueba: Manejo de matrices dispersas (sparse) con alta proporción de ceros estructurales (dropouts), conteos enteros de expresión génica, normalización de tamaño de librería (CPM + $\ln(1 + \text{CPM})$), dispersión HVG y transposición invariante (`val_03_genomics_scrna.ghl`).
- [x] **IMDb Reviews / Tabular Textual:**
  - [x] Estructura: Columnas de texto de longitud variable con caracteres Unicode arbitrarios, tildes en español/francés, signos de puntuación y emojis multi-byte.
  - [x] Prueba: Vectorización de cadenas con `str_contains`, `str_replace`, `str_lower`, `str_len`, `str_split` y verificación de integridad de memoria sin corrupción (`val_04_imdb_textual.ghl`).
- [x] **Datos Financieros de Alta Frecuencia (LOB / Limit Order Book):**
  - [x] Estructura: Timestamps a nivel de microsegundo con ticks de libro de órdenes (bids, asks, volúmenes, operaciones).
  - [x] Prueba: Ventanas móviles y funciones de series temporales (`cumsum`, `lag`, `lead`, `between`, cálculo de spreads bid-ask en bps, micro-precios y volatilidad realizada) (`val_05_lob_financial.ghl`).
- [x] **Validación en Entornos Host Reales:**
  - [x] Empaquetado e instalación limpia de extensión para Positron IDE / VS Code (`editors/vscode/ghl-lang-0.1.0.vsix`) en **Windows** nativo y **Linux**, con gramática TextMate, snippets y cliente LSP.

