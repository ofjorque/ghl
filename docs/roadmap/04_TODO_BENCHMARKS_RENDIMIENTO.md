# Roadmap — Benchmarks de Rendimiento (DataFrames Abiertos + Retos CLBG)

> Parte del backlog temático derivado de [`TODO.md`](../../TODO.md). Cubre las fases originales 5 y 7 (van juntas: ambas son "cuánto tardamos" en cargas sintéticas y estandarizadas, no validación de correctitud ni de datos reales).

---

## Parte A: Benchmarks Abiertos Estándar de DataFrames (H2O.ai & TPC-H)

Contrastar el motor de DataFrames de GHL frente a los benchmarks de código abierto más respetados del ecosistema tabular:

- [ ] **H2O.ai Database-like Ops Benchmark (`h2oai/db-benchmark`):**
  - [ ] Tarea 1: GroupBy con agregación simple (suma, media) en cardinalidad pequeña (K=100) y alta (K=1,000,000).
  - [ ] Tarea 2: GroupBy multi-columna con múltiples agregaciones concurrentes.
  - [ ] Tarea 3: Joins relacionales (Inner join y Left join sobre claves enteras y de texto).
  - [ ] Tarea 4: Ordenamiento y filtrado de alto volumen (0.5 GB, 5 GB).
  - [ ] Tabla comparativa de throughput y consumo de RAM frente a `polars`, `duckdb`, `data.table` (R) y `pandas`.
- [ ] **TPC-H Analytical Queries (Subconjunto Canónico):**
  - [ ] Query 1: Pricing Summary Report Query (agrupación multi-métrica y ordenamiento).
  - [ ] Query 6: Forecasting Revenue Change Query (filtrado por rango de fechas y producto escalar).
- [ ] **NYC Taxi Trip Dataset (Prueba de Ingesta Masiva y Feature Engineering):**
  - [ ] Pipeline end-to-end: Carga de CSV/Parquet -> Filtrado -> Cálculo de distancias -> Agrupamiento -> Regresión OLS.

---

## Parte B: Retos Computacionales Universales de Compiladores

Demostrar que el compilador y runtime de GHL compiten en rendimiento contra C, Rust y Julia más allá de la estadística:

- [ ] **N-Body Simulation (CLBG Canonical Problem):**
  - [ ] Simulación orbital gravitacional del sistema solar (Júpiter, Saturno, Urano, Neptuno) con $50,000,000$ de iteraciones.
  - [ ] Auditar vectorización SIMD, alocación de registros de punto flotante `f64` de Cranelift y ausencia de *boxing*.
  - [ ] Meta de rendimiento: Estar a menos de $1.5\times$ del tiempo de ejecución de C (`gcc -O3`) y Rust (`rustc --release`).
- [ ] **Mandelbrot Fractal (Cálculo de Bits en Matriz Masiva):**
  - [ ] Renderizado del conjunto de Mandelbrot en $16,000 \times 16,000$ puntos con salida de bytes empaquetados.
  - [ ] Auditar optimización de bucles sin predicción fallida de ramas (*branch-free execution*).
- [ ] **Spectral Norm (Iteración de Potencias en Matriz de Hilbert):**
  - [ ] Cálculo de norma espectral con 20 iteraciones sobre matriz de orden $N = 5,500$.
  - [ ] Medición de escalabilidad de paralelismo con Rayon vs OpenMP en C y Julia Threads.
- [ ] **Binary Trees con Arenas Regionales (El Desafío Anti-GC):**
  - [ ] Alocación y destrucción de millones de árboles binarios de profundidad 20.
  - [ ] Demostrar que el modelo de Arenas de GHL (`bumpalo`) supera a lenguajes con Garbage Collection (Java, Julia, R) al liberar árboles enteros en tiempo $O(1)$.
- [ ] **Fannkuch-Redux (Indexación Contigua y Permutaciones In-Place):**
  - [ ] Cálculo de permutaciones de orden $N = 12$ mediante volteo de vectores en memoria contigua.
  - [ ] Auditar cero alocaciones de heap intermedias y latencia mínima de indexación vectorial.
- [ ] **The "Time-to-First-Plot" (TTFX) Challenge:**
  - [ ] Medir tiempo de arranque en frío (*Cold Start*): inicio de proceso + parsing de script + JIT Cranelift + OLS + renderizado gráfico.
  - [ ] Demostrar latencia $< 20\text{ ms}$, superando contundentemente a Julia ($> 3\text{ s}$) y Python ($> 500\text{ ms}$).
- [ ] **Throughput de Inferencia HTTP Concurrente (TechEmpower Benchmark):**
  - [ ] Endpoint de microservicio con `http::serve` bajo $10,000$ peticiones concurrentes (ingesta de JSON, evaluación de modelo OLS/Logit y respuesta JSON).
  - [ ] Medir latencia de percentil $P_{99} < 2\text{ ms}$ y saturación de conexiones TCP sin fugas.
- [ ] **Deep Tail Recursion & TCO (Tail Call Optimization):**
  - [ ] Ejecución de función de Ackermann y Collatz con $10,000,000$ de llamadas recursivas anidadas.
  - [ ] Garantizar consumo de memoria de stack plano ($0$ crecimiento del call stack, $0$ stack overflow).
- [ ] **Recorrido de Grafos Irregulares (BFS / DFS en 10M de Nodos):**
  - [ ] Búsqueda en grafos dispersos con acceso no contiguo a memoria (*pointer chasing*).
  - [ ] Auditar el rendimiento de las referencias CoW y ARC frente a la fragmentación de caché L1/L2/L3.
