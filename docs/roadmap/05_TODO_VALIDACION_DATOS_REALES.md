# Roadmap — Validación Extensiva con Diversidad Real de DataFrames

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre la fase original 9, separada de los benchmarks sintéticos porque aquí el objetivo es volumen y diversidad estructural del mundo real, no un número de referencia estandarizado.

Auditar el comportamiento del motor columnar de GHL frente a la complejidad estructural y tipos de datos del mundo real:

- [ ] **Higgs Boson Dataset (CERN / UCI):**
  - [ ] Volumen: $11,000,000$ de filas densas de punto flotante `f64` (28 variables cinemáticas continuas).
  - [ ] Prueba: Ingesta masiva, normalización estadística y clasificación logística IRLS a gran escala.
- [ ] **Airline On-Time Performance (ASA Data Expo):**
  - [ ] Volumen: Más de $120,000,000$ de registros de vuelos históricos.
  - [ ] Prueba: Cardinalidad extrema en variables categóricas (aeropuertos de origen/destino, transportistas), `group_by` multi-clave y joins con tablas maestras.
- [ ] **Genómica y Single-Cell RNA-seq (Matrices Ultra-Anchas):**
  - [ ] Estructura: Matrices transponibles de alta dimensionalidad ($20,000$ genes $\times 50,000$ células).
  - [ ] Prueba: Manejo de matrices dispersas (sparse) con alta proporción de ceros estructurales y conteos enteros de expresión génica.
- [ ] **IMDb Reviews / Tabular Textual:**
  - [ ] Estructura: Columnas de texto de longitud variable con caracteres Unicode arbitrarios, tildes, signos de puntuación y emojis.
  - [ ] Prueba: Vectorización de cadenas con `str_contains`, `str_replace`, `str_lower` y filtrado sin corrupción de memoria.
- [ ] **Datos Financieros de Alta Frecuencia (LOB / Limit Order Book):**
  - [ ] Estructura: Timestamps a nivel de microsegundo con millones de ticks por día de negociación.
  - [ ] Prueba: Ventanas móviles y funciones de series temporales (`cumsum`, `lag`, `lead`, `between`, cálculo de spreads bid-ask y volatilidad realizada).
- [ ] **Validación en Entornos Host Reales:**
  - [ ] Pruebas locales de instalación limpia de Positron IDE en **Windows** (entorno nativo de trabajo) y **Linux** (vía WSL / contenedores).
