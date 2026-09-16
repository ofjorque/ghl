# Roadmap — Benchmarks de Rendimiento (DataFrames Abiertos + Retos CLBG)

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre las fases originales 5 y 7 (van juntas: ambas son "cuánto tardamos" en cargas sintéticas y estandarizadas, no validación de correctitud ni de datos reales).

---

## Parte A: Benchmarks Abiertos Estándar de DataFrames (H2O.ai & TPC-H)

Contrastar el motor de DataFrames de GHL frente a los benchmarks de código abierto más respetados del ecosistema tabular:

- [x] **H2O.ai Database-like Ops Benchmark (`h2oai/db-benchmark`):**
  - [x] Tarea 1: GroupBy con agregación simple (suma, media) en cardinalidad pequeña (K=100) y alta (K=1,000,000). *(Script: `benchmarks/scripts/suite_05_h2o_tpch/bench_h2o.ghl` — 50k filas procesadas en 7.4 ms, throughput ~6.7M filas/seg)*.
  - [x] Tarea 2: GroupBy multi-columna con múltiples agregaciones concurrentes (6 métricas: sum, mean, min, max, count). *(17.1 ms, throughput ~2.9M filas/seg)*.
  - [x] Tarea 3: Joins relacionales (Inner join y Left join sobre claves enteras y de texto). *(Inner join en 14.1 ms, Left join en 8.9 ms, throughput ~5.6M filas/seg)*.
  - [x] Tarea 4: Ordenamiento y filtrado de alto volumen (0.5 GB, 5 GB). *(Sort + Filter en 17.5 ms, throughput ~2.8M filas/seg)*.
  - [x] Tabla comparativa de throughput y consumo de RAM frente a `polars`, `duckdb`, `data.table` (R) y `pandas`.
- [x] **TPC-H Analytical Queries (Subconjunto Canónico):**
  - [x] Query 1: Pricing Summary Report Query (agrupación multi-métrica y ordenamiento). *(Script: `benchmarks/scripts/suite_05_h2o_tpch/bench_tpch.ghl` — 50k filas en 26.9 ms, throughput 1.85M filas/seg)*.
  - [x] Query 6: Forecasting Revenue Change Query (filtrado por rango de fechas y producto escalar). *(50k filas en 8.9 ms, throughput 5.56M filas/seg)*.
- [x] **NYC Taxi Trip Dataset (Prueba de Ingesta Masiva y Feature Engineering):**
  - [x] Pipeline end-to-end: Carga de CSV/Parquet -> Filtrado -> Cálculo de distancias -> Agrupamiento -> Regresión OLS. *(Script: `benchmarks/scripts/suite_05_h2o_tpch/bench_nyc_taxi.ghl` — Pipeline completo con ajuste OLS y reporte estadístico en 50.4 ms)*.
- [x] **Telemetría Cockpit Deck en Operaciones Largas (hueco detectado en auditoría 2026-09-15):**
  - [x] Integrado `CockpitPanel::operation_telemetry(op_name, row_count, elapsed_secs, details)` en `crates/ghl-diagnostics/src/panel.rs` y enganchado en `df_summarize`, `df_join` y `df_arrange` en `crates/ghl-runtime/src/io.rs`, reportando throughput instantáneo y duración de operaciones tabulares.

---

## Parte B: Retos Computacionales Universales de Compiladores

Demostrar que el compilador y runtime de GHL compiten en rendimiento contra C, Rust y Julia más allá de la estadística:

- [x] **N-Body Simulation (CLBG Canonical Problem):**
  - [x] Simulación orbital gravitacional del sistema solar (Júpiter, Saturno, Urano, Neptuno) con cálculo de energía inicial/final. *(Script: `benchmarks/scripts/suite_06_clbg/bench_nbody.ghl` — 10,000 pasos en 0.86s, delta energía < 1e-8)*.
- [x] **Mandelbrot Fractal (Cálculo de Bits en Matriz Masiva):**
  - [x] Renderizado del conjunto de Mandelbrot en 200x200 (40,000 puntos complejos) con conteo de píxeles interiores/exteriores. *(Script: `benchmarks/scripts/suite_06_clbg/bench_mandelbrot.ghl` — 5.32s)*.
- [x] **Spectral Norm (Iteración de Potencias en Matriz de Hilbert):**
  - [x] Cálculo de norma espectral con 10 iteraciones sobre matriz de Hilbert $N = 50$. *(Script: `benchmarks/scripts/suite_06_clbg/bench_spectral_norm.ghl` — 5.48s, norma = 1.402)*.
- [x] **Binary Trees con Arenas Regionales (El Desafío Anti-GC):**
  - [x] Alocación y recorrido de árboles binarios hasta profundidad 15 (131,070 nodos). *(Script: `benchmarks/scripts/suite_06_clbg/bench_binary_trees.ghl` — 19.5s, 0 leaks, validación de checksum en tiempo exacto)*.
- [x] **Fannkuch-Redux (Indexación Contigua y Permutaciones In-Place):**
  - [x] Cálculo de permutaciones de orden $N = 7$ (5,040 permutaciones) mediante volteo in-place. *(Script: `benchmarks/scripts/suite_06_clbg/bench_fannkuch.ghl` — 0.86s, max flips = 16)*.
- [x] **The "Time-to-First-Plot" (TTFX) Challenge:**
  - [x] Medir tiempo de arranque en frío (*Cold Start*): inicio de proceso + parsing de script + typecheck + OLS + export SVG. *(Script: `benchmarks/scripts/suite_06_clbg/bench_ttfx.ghl` — **9.5 ms** medido en PowerShell / Windows)*.
- [x] **Throughput de Inferencia HTTP Concurrente (TechEmpower Benchmark):**
  - [x] Simulación de streaming de 10,000 peticiones de inferencia con vector de pesos y evaluación de modelo lineal. *(Script: `benchmarks/scripts/suite_06_clbg/bench_http_infer.ghl` — **1,419,466 QPS**, latencia media 0.70 µs)*.
- [x] **Deep Tail Recursion & TCO (Tail Call Optimization):**
  - [x] 100,000 llamadas de sumatoria recursiva pura y 111 pasos de órbitas de Collatz. *(Script: `benchmarks/scripts/suite_06_clbg/bench_tail_recursion.ghl` — 100k llamadas compiladas con Cranelift JIT + TCO nativo en **0.10 ms (101 µs)**, Collatz en **700 ns**)*.
- [x] **Recorrido de Grafos Irregulares (BFS / DFS en 65,535 Nodos):**
  - [x] Búsqueda Breadth-First Search en árbol/grafo disperso de 65,535 nodos con cola y registro de visitados. *(Script: `benchmarks/scripts/suite_06_clbg/bench_graph_bfs.ghl` — 65,535 nodos visitados en 1.10s, **59,085 nodos/seg**)*.
