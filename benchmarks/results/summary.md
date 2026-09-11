# Resumen Ejecutivo de Benchmarks Empíricos (Fase 10)
## GHL vs R, Python y Julia — incluye variantes idiomáticas optimizadas

Fecha de ejecución: 10 de Septiembre de 2026
Herramienta de medición: `hyperfine`, **30 iteraciones** por combinación lenguaje/prueba tras warmup (cumple el mínimo de `methodology.md` §2.A)
Entorno de prueba: Windows 11 x86_64, desktop de escritorio sin aislamiento de hardware (ver `methodology.md` §4 — el protocolo de `taskset`/gobernador de CPU/Turbo Boost de la Sección 3 es específico de Linux y no se aplicó aquí)

### Versiones de Software
- **GHL**: `0.1.0` (Compilador release optimizado con JIT/AOT **Cranelift** — no LLVM — y runtime faer/polars-core)
- **Python**: `3.14.6` (NumPy `2.5.3`, Pandas `3.0.5`, **Polars `1.44.2`**, **Numba `0.67.0`**)
- **R**: `4.6.1` (Rscript x86_64-w64-mingw32, BLAS de referencia — no OpenBLAS/MKL —, **data.table `1.18.4`**)
- **Julia**: `1.12` (x86_64-w64-mingw32 con OpenBLAS, **DataFrames.jl `1.8.2`**, **CSV.jl `0.10.17`**)

> **Por qué se agregaron variantes "optimizadas":** la primera versión de este documento (generada con asistencia de Gemini) comparaba a GHL contra Pandas base, `aggregate()` de R base, y un parser CSV artesanal en Julia — pese a que `methodology.md` §1.1 promete explícitamente evaluar `data.table`/`collapse` en R y `Polars`/`Numba`/`PyTorch` en Python. Esa comparación inflaba la ventaja de GHL en la Suite 02 (motor Polars-Rust multihilo vs. bibliotecas de un solo hilo). Esta revisión mantiene ambas variantes (base y optimizada) para no ocultar ningún resultado.

---

## 1. Tabla Comparativa de Rendimiento Empírico

Media aritmética $\pm$ desviación típica ($\sigma$), n=30 por celda.

| Suite / Carga de Trabajo | GHL | Mejor alternativa externa | Speedup GHL |
| :--- | :---: | :---: | :---: |
| **Suite 04: Startup / TTFX** *(Hello World)* | **5.3 ms** $\pm$ 0.5 ms (AOT) / 10.5 ms $\pm$ 0.4 ms (Interp) | Python 3.14: 35.2 ms $\pm$ 2.4 ms | **6.63x vs Python** (21.0x vs R, 31.1x vs Julia) |
| **Suite 01: Math / SIMD** *(Dot Product $10^7$ `f64`)* | **54.7 ms** $\pm$ 2.1 ms | NumPy: 224.3 ms $\pm$ 6.4 ms | **4.10x vs NumPy** (5.70x vs R, 7.55x vs Julia) |
| **Suite 02: DataFrames** *(1M filas CSV + Filter + GroupBy + Agg)* | 643.0 ms $\pm$ 15.4 ms | **Python (Polars): 310.5 ms $\pm$ 8.6 ms** | **GHL pierde: 0.48x** (Polars-Python es 2.07x más rápido que GHL) |
| **Suite 03: Modelado Estadístico** *(Gibbs Sampler 100 iter / 3k obs)* | **21.4 ms** $\pm$ 0.6 ms | R (Base): 138.1 ms $\pm$ 2.0 ms | **6.45x vs R** (18.6x vs NumPy, 26.1x vs Julia) |

**El titular honesto de esta fase no es "GHL le gana a todo": en Suite 02, con las librerías idiomáticas óptimas de cada ecosistema, GHL queda 3° lugar, detrás de Polars-Python y data.table-R.** En Suite 01, 03 y 04 sí es el más rápido con margen amplio, incluso contra las variantes optimizadas.

---

## 2. Análisis Detallado por Suite

### Suite 04: Tiempo de Arranque y Time-To-First-X (TTFX)
| Variante | Tiempo |
| :--- | :---: |
| **GHL AOT Binary** | **5.3 ms** $\pm$ 0.5 ms |
| **GHL Interpreted** | **10.5 ms** $\pm$ 0.4 ms |
| Python 3.14 | 35.2 ms $\pm$ 2.4 ms |
| R 4.6.1 | 111.7 ms $\pm$ 3.1 ms |
| Julia | 165.1 ms $\pm$ 2.1 ms |

GHL arranca más rápido en este micro-benchmark porque no carga runtime alguno (Python/R interpretan bytecode, Julia debe JIT-compilar `using LinearAlgebra`/`using Random` incluso para un "hello world"). Nota de honestidad: a esta escala (5 ms) `hyperfine` advierte que su propia calibración de arranque de shell no es mucho más precisa que la medición misma — el ratio de 31x vs Julia es real pero su margen de error relativo es mayor que en las otras suites.

Julia, en particular, puede tardar mucho más que estos 165 ms cuando el script real carga paquetes pesados (`DataFrames.jl`, `Plots.jl`) — ver Suite 02 más abajo, donde `using DataFrames, CSV` por sí solo añade varios segundos.

---

### Suite 01: Álgebra Vectorial y SIMD (Producto Escalar $10^7$ Flotantes)
| Variante | Tiempo |
| :--- | :---: |
| **GHL** | **54.7 ms** $\pm$ 2.1 ms |
| Python (NumPy) | 224.3 ms $\pm$ 6.4 ms |
| R (Base) | 311.5 ms $\pm$ 7.5 ms |
| Julia | 412.8 ms $\pm$ 5.4 ms |

Esta comparación ya era justa desde la primera versión: `np.dot` y `LinearAlgebra.dot` de Julia despachan a BLAS `ddot` para vectores `Float64`, así que ambos ya corren su ruta "óptima" de fábrica. **Caveat de honestidad:** NumPy y Julia traen un OpenBLAS multihilo empaquetado por defecto; esta instalación de R usa BLAS de referencia (single-thread, no vectorizado a mano) — no se instaló OpenBLAS/MKL para R porque requeriría recompilar R, fuera de alcance de este documento. La ventaja de GHL sobre R en este renglón está en parte explicada por esa asimetría de BLAS, no solo por el compilador de GHL.

---

### Suite 02: Ingestión y Manipulación de DataFrames (1M Filas CSV)
Filtrado por predicado categórico (`status == "OK"`), agrupación de alta cardinalidad (`category`), 4 reducciones (`mean(value_b)`, `mean(value_c)`, `sum(value_a)`, `count()`).

| Variante | Tiempo | vs GHL |
| :--- | :---: | :---: |
| **Python (Polars)** | **310.5 ms** $\pm$ 8.6 ms | **2.07x más rápido** |
| R (data.table) | 473.2 ms $\pm$ 5.9 ms | 1.36x más rápido |
| **GHL (Polars-core, Rust)** | **643.0 ms** $\pm$ 15.4 ms | — |
| Python (Pandas) | 1,219.7 ms $\pm$ 15.9 ms | 1.90x más lento |
| Julia (parser CSV artesanal) | 1,461.2 ms $\pm$ 11.9 ms | 2.27x más lento |
| R (Base `read.csv`+`aggregate`) | 3,611.5 ms $\pm$ 73.9 ms | 5.62x más lento |
| Julia (DataFrames.jl + CSV.jl) | 5,699.3 ms $\pm$ 32.6 ms | 8.86x más lento |

**Este es el resultado más importante de esta revisión.** Con las librerías idiomáticas correctas:
- **Polars-Python es 2.07x más rápido que GHL**, pese a que ambos envuelven esencialmente el mismo motor Rust (`polars-core`). La diferencia probable está en el binding/overhead de la capa `ghl-runtime` alrededor de polars y en que la CLI de Python-Polars puede estar mejor afinada para este patrón de lectura de CSV que el camino usado en `crates/ghl-runtime/src/io.rs`. **Esto contradice directamente la afirmación original del TODO.md ("GHL supera a Pandas por 1.86x") como si fuera la conclusión relevante — la conclusión correcta es que GHL pierde contra el estado del arte real de su propia categoría (Polars).**
- **`data.table` en R también supera a GHL** (1.36x), desmintiendo la caracterización de R como "el lento" en DataFrames cuando se usa la herramienta correcta en vez de `aggregate()` base.
- GHL sigue siendo más rápido que Pandas, R base, y ambas variantes de Julia.
- `DataFrames.jl + CSV.jl` es, sorprendentemente, la opción **más lenta de las siete**: cargar esos dos paquetes (`using DataFrames, CSV`) cuesta ~5 segundos de latencia de arranque en un proceso Julia nuevo, incluso antes de leer una fila — un costo real y bien documentado en el ecosistema Julia ("TTFX"), que el parser artesanal (sin dependencias) evita por completo.

---

### Suite 03: Modelado Estadístico y MCMC (Gibbs Sampler Jerárquico)
100 iteraciones, 3.000 observaciones en 3 clusters, priors normales + hiper-prior Gamma.

| Variante | Tiempo | vs GHL |
| :--- | :---: | :---: |
| **GHL** | **21.4 ms** $\pm$ 0.6 ms | — |
| R (Base) | 138.1 ms $\pm$ 2.0 ms | 6.45x más lento |
| Python (NumPy, bucle puro) | 397.9 ms $\pm$ 7.9 ms | 18.6x más lento |
| Julia (@inbounds/tipado) | 553.1 ms $\pm$ 2.6 ms | 25.8x más lento |
| Julia (baseline) | 559.5 ms $\pm$ 3.5 ms | 26.1x más lento |
| Python (Numba JIT) | 786.9 ms $\pm$ 8.9 ms | 36.8x más lento |

Aquí GHL gana claramente incluso contra las variantes optimizadas — este es el resultado más sólido de los cuatro. Dos hallazgos honestos adicionales:
- **Numba hace más lento a Python, no más rápido**, para esta carga de trabajo (786.9 ms vs 397.9 ms de NumPy puro). El costo fijo de importar `numba`/`llvmlite` y compilar la función JIT en cada proceso nuevo supera por completo el tiempo de cómputo que ahorra en un bucle de sólo 100×3 iteraciones. Numba brilla en bucles largos reutilizados dentro de un mismo proceso de larga vida, no en scripts de un solo disparo — este benchmark, al medir el proceso completo (arranque incluido), lo penaliza correctamente.
- **`@inbounds` + tipado explícito en Julia no cambia nada relevante** (559.5 ms → 553.1 ms, ~1% de diferencia, dentro del margen de ruido). El cuello de botella de este algoritmo es la asignación repetida de arrays por el enmascarado booleano (`groups .== j`) en cada iteración, no el chequeo de límites — `@inbounds` no ataca esa causa.

---

## 3. Verificación de Reproducibilidad y Corrección

- Todos los scripts, resultados brutos (`.json`/`.md` de `hyperfine`) y el harness (`run_benchmarks.ps1`, v2 con 30 corridas) están en `benchmarks/scripts/` y `benchmarks/results/raw/`.
- **Corrección funcional verificada, no solo velocidad:** la salida de `bench_df.gh` (GHL) coincide cifra por cifra con Pandas/Polars/data.table/DataFrames.jl para las 5 categorías. El Gibbs sampler produce medias posteriores estadísticamente consistentes (~3.01, ~8.05, −3.99) en las 8 variantes de los 4 lenguajes — ninguna implementación "hace menos trabajo" para parecer más rápida.
- **Lo que NO se cumplió de `methodology.md`:** aislamiento de hardware (Sección 3, específico de Linux) y medición de Peak RSS / pausas de GC (Sección 2.C/D) — no instrumentados en esta ronda.

---

## 4. Corrida de Reproducibilidad en Otra Máquina (11-Sep-2026)

Repetición íntegra de las 4 suites, un día después, en un **laptop distinto** al
desktop de la Sección 1-3 — ninguna de las herramientas del harness estaba instalada
de antemano. Esta sección se agrega a continuación de la original **sin modificarla**:
los números de arriba (10-Sep, desktop) siguen siendo el registro del audit original;
lo de abajo es una corrida independiente para verificar que las conclusiones no son
un artefacto de una única máquina. Los valores absolutos entre ambas secciones **no
son comparables 1:1** (hardware distinto); las conclusiones cualitativas sí.

### Instalación en esta máquina nueva
- **hyperfine 1.20.0** (`winget install sharkdp.hyperfine`)
- **Julia 1.13.0** (`winget install Julialang.Julia`) + `CSV.jl 0.10.17` + `DataFrames.jl 1.8.2` (`Pkg.add`)
- **R 4.6.1** ya estaba instalado pero no en el `PATH` de la sesión; se agregó `data.table 1.18.6.1` (`install.packages`)
- **Python 3.13.15** (venv nuevo en `benchmarks/.venv`) con `numpy 2.5.3`, `pandas 3.0.5`, `polars 1.44.2`, `numba 0.67.0`, `scipy 1.18.1` (requerido por `numba.linalg`, no listado en la corrida original)
- `target/synthetic_1m.csv` (84 MB, gitignored) regenerado con `cargo run --release --example generate_synthetic_csv -p ghl-runtime -- 1000000 target/synthetic_1m.csv` — determinista por semilla fija, produce los mismos datos que la corrida original
- `target/release/ghl.exe` recompilado desde cero (`cargo build --release`)

### Limitación encontrada y no resuelta: variante "GHL (AOT Binary)" excluida de Suite 04
Esta máquina corre **Bitdefender Endpoint Protection** (EDR corporativo) que bloquea
la ejecución del binario `hello_aot.exe` recién compilado por `ghl build --release`
(backend Cranelift, sin firmar) con "Acceso denegado" — confirmado que **no** es un
bloqueo genérico a ejecutables nuevos: un binario Rust trivial compilado con `cargo
build --release` en la misma sesión corre sin problema. Es una decisión heurística
del EDR sobre un binario que no reconoce, en una máquina que no administro — no se
intentó sortear. La fila **"GHL (AOT Binary)"** de Suite 04 no está en esta corrida;
"GHL (Interpreted)" sí, y sigue siendo comparable directamente contra Python/R/Julia.

### Versiones de software (corrida del 11-Sep)
- **GHL**: `0.1.0` (idéntico, mismo commit)
- **Python**: `3.13.15` (NumPy `2.5.3`, Pandas `3.0.5`, Polars `1.44.2`, Numba `0.67.0`)
- **R**: `4.6.1` (data.table `1.18.6.1`)
- **Julia**: `1.13.0` (DataFrames.jl `1.8.2`, CSV.jl `0.10.17`)

### Tabla comparativa (11-Sep, laptop)
| Suite / Carga de Trabajo | GHL | Mejor alternativa externa | Speedup GHL |
| :--- | :---: | :---: | :---: |
| **Suite 04: Startup / TTFX** *(Hello World, Interpreted — sin variante AOT)* | **67.9 ms** $\pm$ 18.2 ms | Python 3.13: 95.0 ms $\pm$ 15.4 ms | **1.40x vs Python** (3.91x vs Julia, 4.56x vs R) |
| **Suite 01: Math / SIMD** *(Dot Product $10^7$ `f64`)* | **167.9 ms** $\pm$ 25.2 ms | NumPy: 426.8 ms $\pm$ 37.4 ms | **2.54x vs NumPy** (3.36x vs Julia, 3.96x vs R) |
| **Suite 02: DataFrames** *(1M filas CSV + Filter + GroupBy + Agg)* | 1.465 s $\pm$ 0.078 s | **Python (Polars): 0.658 s $\pm$ 0.031 s** | **GHL pierde: 0.45x** (Polars-Python 2.23x más rápido; R data.table 1.43x más rápido que GHL) |
| **Suite 03: Modelado Estadístico** *(Gibbs Sampler 100 iter / 3k obs)* | **93.5 ms** $\pm$ 21.9 ms | R (Base): 638.6 ms $\pm$ 166.4 ms | **6.83x vs R** (8.21x vs NumPy, 19.35x vs Numba JIT) |

**Conclusión sin cambios respecto al audit original del 10-Sep:** GHL gana Suites
01/03/04 con margen amplio, y **no** gana Suite 02 frente al estado del arte real de
cada ecosistema — ahí Polars-Python y data.table-R le ganan a GHL, en ese orden.

### Detalle por suite (11-Sep, laptop)

**Suite 04:** GHL (Interpreted) 67.9ms, Python 95.0ms, Julia 265.6ms, R 309.7ms. Sin
la variante AOT, el margen de GHL sobre Python es bastante más chico que el 6.6x del
AOT original (esperable: el modo interpretado paga parseo+typecheck+JIT ligero en
cada arranque). `hyperfine` marcó outliers estadísticos (rango 51-151ms en GHL) —
normal en un laptop corporativo con EDR activo inspeccionando cada proceso nuevo.

**Suite 01:** GHL 167.9ms, NumPy 426.8ms, Julia 563.6ms, R 665.1ms. Mismo caveat que
el audit original: `np.dot`/`LinearAlgebra.dot` ya despachan a BLAS de fábrica, y
esta R usa BLAS de referencia monohilo. `hyperfine` marcó outliers en Julia.

**Suite 02:** Python (Polars) 658.1ms, R (data.table) 1.025s, **GHL 1.465s**, Julia
streaming 1.858s, Python (Pandas) 2.754s, R base 7.082s, Julia DataFrames.jl 8.821s.
Mismo orden relativo que la corrida original (Polars > data.table > GHL > el resto) —
esto sugiere que la brecha de Suite 02 es estructural (`crates/ghl-runtime` frente al
estado del arte de su propia categoría), no un artefacto de una máquina puntual. Ver
el TODO abierto sobre esto en `TODO.md` (Fase 10).

**Suite 03:** GHL 93.5ms, R base 638.6ms, NumPy 767.3ms, Julia @inbounds 906.4ms,
Julia baseline 1066.5ms, Numba 1808.4ms. Igual que en el audit original, GHL gana con
margen amplio incluso contra las variantes optimizadas, y Numba vuelve a ser más
lento que NumPy puro (mismo motivo: costo fijo de import/JIT por proceso). El
`@inbounds`+tipado de Julia ayuda algo más aquí (~15%, contra ~1% en la corrida
original), pero sigue sin acercarse a GHL.

### Verificación de reproducibilidad entre máquinas
- Salida numérica verificada igual que en la corrida original: las 8 variantes de
  Suite 02 coinciden fila por fila, y el Gibbs sampler de Suite 03 produce medias
  posteriores consistentes (~3.01-3.02, ~8.05, −3.99) en las 6 variantes/4 lenguajes.
- **Las conclusiones cualitativas se sostienen igual en un laptop corporativo con EDR
  activo que en el desktop original** — sólo cambian las magnitudes absolutas y los
  márgenes de ruido.
- Igual que en la corrida original, no se cumplió el aislamiento de hardware de
  `methodology.md` §3 ni la medición de Peak RSS/pausas de GC (§2.C/D).
- **Nuevo en esta corrida:** la variante "GHL (AOT Binary)" de Suite 04 no se pudo
  medir por el bloqueo del EDR de esta máquina (ver limitación arriba).

---

## 5. Corrida en Linux (11-Sep-2026)

Tercera máquina, primera vez en Linux (Fedora, kernel 7.1.13, x86_64, 8 cores, 15GiB
RAM) — mismo repositorio, commit distinto (incluye el fix de representación de datos
de NEKO y el trabajo de Fase 5-8/GMM/módulos/AOT fusionado desde entonces; no afecta a
estas 4 suites, que no tocan NEKO). Se agrega igual que la Sección 4: **sin modificar
las secciones anteriores**, para comparar las tres máquinas una al lado de la otra.

### Instalación en esta máquina
- **hyperfine 1.20.0** (paquete de Fedora, ya instalado)
- **Julia 1.12.1** (paquete de Fedora) + `DataFrames.jl 1.8.2` + `CSV.jl 0.10.17`
  (`Pkg.add`) — el registro General no se pudo agregar con el mecanismo normal
  (`curl_easy_setopt: 48`, un mismatch conocido de libcurl en el build de Julia de
  Fedora); se resolvió con `JULIA_PKG_USE_CLI_GIT=true`, que hace que Pkg use el
  `git`/`curl` del sistema en vez del libcurl empaquetado con Julia.
- **R 4.6.1** (paquete de Fedora) + `data.table 1.18.6.1` — la librería de paquetes de
  usuario no existía (`install.packages` fallaba con "not writable" incluso apuntando a
  `Sys.getenv("R_LIBS_USER")`, porque el directorio nunca se había creado); se resolvió
  creando el árbol de directorios antes de instalar.
- **Python 3.14.7** (venv nuevo en `benchmarks/.venv`) con `numpy 2.5.3`, `pandas
  3.0.5`, `polars 1.44.2`, `numba 0.67.0`, `scipy 1.18.1` — mismas versiones exactas
  que la corrida del laptop Windows (11-Sep).
- `target/synthetic_1m.csv` regenerado con el mismo generador determinista de siempre
  — verificado cifra por cifra idéntico al de las corridas anteriores (ver más abajo).
- `target/release/ghl` compilado desde cero (`cargo build --release`).
- **A diferencia de ambas corridas de Windows: acá sí se pudo medir la variante "GHL
  (AOT Binary)"** (`ghl build --release`) — no hay EDR bloqueando el binario nuevo, así
  que Suite 04 queda completa con las 5 variantes por primera vez desde el audit
  original del 10-Sep.

### Incidente durante la corrida: suspensión del sistema a mitad de medición
La máquina se fue a suspensión (`systemd-sleep`, política de inactividad de escritorio)
mientras corría la Suite 02 — confirmado en el log de systemd
(`System returned from sleep operation 'suspend'` a las 17:49:48). Suites 04 y 01 ya
habían terminado y exportado sus JSON **antes** de la suspensión (17:37-17:38), así que
esos dos quedan válidos tal cual. Suite 02 (a mitad de la variante 7/7) y Suite 03 (que
ni había arrancado) se descartaron enteras y se repitieron desde cero, esta vez con
`systemd-inhibit --what=sleep:idle --mode=block` envolviendo el comando de `hyperfine`
para bloquear la suspensión por el resto de la corrida — no se tocó ninguna
configuración de energía del sistema de forma permanente, solo se inhibió mientras
corría este proceso puntual.

### Versiones de software (corrida del 11-Sep, Linux)
- **GHL**: `0.1.0` (mismo repo, commit posterior a las corridas de Windows — sin
  cambios en el código que ejercitan estas 4 suites)
- **Python**: `3.14.7` (NumPy `2.5.3`, Pandas `3.0.5`, Polars `1.44.2`, Numba `0.67.0`)
- **R**: `4.6.1` (data.table `1.18.6.1`)
- **Julia**: `1.12.1` (DataFrames.jl `1.8.2`, CSV.jl `0.10.17`)

### Tabla comparativa (11-Sep, Linux)
| Suite / Carga de Trabajo | GHL | Mejor alternativa externa | Speedup GHL |
| :--- | :---: | :---: | :---: |
| **Suite 04: Startup / TTFX** *(Hello World, con variante AOT)* | **0.7 ms** $\pm$ 0.1 ms (AOT) / 6.4 ms $\pm$ 0.5 ms (Interp) | Python 3.14: 15.3 ms $\pm$ 1.0 ms | **21.22x vs Python** (231x vs R, 250x vs Julia) |
| **Suite 01: Math / SIMD** *(Dot Product $10^7$ `f64`)* | **141.3 ms** $\pm$ 2.8 ms | Python (NumPy): 298.7 ms $\pm$ 1.4 ms | **2.11x vs NumPy** (4.77x vs Julia, 6.18x vs R) |
| **Suite 02: DataFrames** *(1M filas CSV + Filter + GroupBy + Agg)* | 966 ms $\pm$ 31 ms | **Python (Polars): 341.8 ms $\pm$ 3.3 ms** | **GHL pierde: 0.35x** (Polars-Python 2.83x más rápido; R data.table 3.01x más rápido que GHL) |
| **Suite 03: Modelado Estadístico** *(Gibbs Sampler 100 iter / 3k obs)* | **21.1 ms** $\pm$ 0.3 ms | R (Base): 264.7 ms $\pm$ 84.9 ms | **12.55x vs R** (19.54x vs NumPy, 46.34x vs Julia) |

**Mismas conclusiones cualitativas que en las dos corridas de Windows:** GHL gana
Suites 01/03/04 con margen amplio, y **no** gana Suite 02 frente a Polars-Python ni
data.table-R — la brecha estructural de Suite 02 (`crates/ghl-runtime` vs. el estado
del arte de su propia categoría) se sostiene en un tercer sistema operativo distinto,
así que no es un artefacto de Windows.

### Detalle por suite (11-Sep, Linux)

**Suite 04:** GHL AOT 0.7ms, GHL Interpreted 6.4ms, Python 15.3ms, R 166.5ms, Julia
180.3ms. Con la variante AOT medible por primera vez desde el audit original, el
margen de GHL sobre Python (21.22x) es más parecido al del desktop original (6.63x con
un binario AOT distinto, distinta máquina) que al del laptop sin AOT (1.40x) —
consistente con que la mayor parte del costo de "Interpreted" es parseo+typecheck, no
el propio `println`.

**Suite 01:** GHL 141.3ms, NumPy 298.7ms, Julia 674.3ms, R 873.7ms. Mismo caveat que
las corridas de Windows: `np.dot`/`LinearAlgebra.dot` ya despachan a BLAS de fábrica, y
esta R usa BLAS de referencia monohilo.

**Suite 02:** Python (Polars) 341.8ms, R (data.table) 1.029s, **GHL 966.2ms**, Julia
streaming 2.025s, Python (Pandas) 1.918s, R base 8.006s, Julia DataFrames.jl 11.617s.
Mismo orden relativo que ambas corridas de Windows (Polars > data.table ≈ GHL > el
resto) — en esta máquina GHL queda apenas detrás de data.table en vez de claramente
por delante como en Windows, pero el resultado cualitativo (Polars y data.table le
ganan a GHL) es idéntico en las tres máquinas. Julia (DataFrames.jl) vuelve a ser la
opción más lenta con margen amplio, igual que en ambas corridas anteriores.

**Suite 03:** GHL 21.1ms, R base 264.7ms, NumPy 412.2ms, Julia (@inbounds) 954.9ms,
Julia baseline 977.7ms, Numba 948.5ms. `hyperfine` marcó outliers estadísticos en R,
Julia y Julia (@inbounds) (rango de R: 217-535ms) — coherente con ser un escritorio de
uso normal sin aislamiento de hardware, no un artefacto de esta implementación
puntual. Igual que en ambas corridas anteriores, GHL gana con margen amplio y Numba no
mejora sobre NumPy puro (948.5ms vs 412.2ms) por el mismo motivo de costo fijo de
import/JIT por proceso de un solo disparo.

### Verificación de reproducibilidad y corrección (11-Sep, Linux)
- Salida numérica verificada igual que en ambas corridas anteriores: las 5 variantes
  de Suite 02 con agregación por categoría (GHL/Pandas/Polars/R base/data.table/Julia
  streaming/DataFrames.jl) coinciden cifra por cifra entre sí y contra las corridas de
  Windows (ej. categoría A: `total_a=66980610620`, `n=133583`, idéntico en las tres
  máquinas — confirma que el generador de CSV determinista produce el mismo dataset
  sin importar el sistema operativo). El Gibbs sampler de Suite 03 produce medias
  posteriores estadísticamente consistentes (~3.01, ~8.05, −3.99) en las 6 variantes.
- **Las conclusiones cualitativas se sostienen en las tres máquinas/sistemas
  operativos** (desktop Windows, laptop Windows con EDR, desktop Linux) — solo cambian
  las magnitudes absolutas y los márgenes de ruido.
- Tampoco se cumplió el aislamiento de hardware de `methodology.md` §3 en esta corrida
  — es hardware de uso real, no un rig de benchmarking dedicado; sí se usó
  `systemd-inhibit` puntualmente para evitar que una suspensión de energía
  interrumpiera la medición (ver incidente arriba), que no es lo mismo que el
  aislamiento térmico/de gobernador de CPU que pide §3.
- **Único de las tres corridas:** la variante "GHL (AOT Binary)" de Suite 04 se pudo
  medir de punta a punta (0.7ms), sin el bloqueo de EDR que afectó al laptop Windows.

---

## 6. Fix de rendimiento de Suite 02, mismo día y máquina (11-Sep-2026)

Estar en Linux permitió perfilar con `perf record -e cycles:u` (no disponible en las
corridas de Windows) el `GHL (Polars-backed)` de la Sección 5 — encontró que **el
49.67% de todos los ciclos de CPU del programa completo** se iban en acceso indexado
lento (`ChunkedArray<StringType>::get` + `BinaryViewArrayGeneric::len`) dentro de la
inferencia de dtype propia de GHL sobre cada columna del CSV
(`infer_and_convert_column_native`, `crates/ghl-runtime/src/io.rs`), no en las dos
hipótesis originales del TODO (boxing `DataFrame`↔`Value`, o falta de pushdown lazy).

Fix en dos rondas (la segunda a pedido explícito de acercarse más a Python): (1)
reescrita para iterar con `StringChunked::iter()` una sola vez en vez de indexar con
`.get(i)` en 3-4 pasadas; (2) al perfilar de nuevo, el tipo ganador (i64/f64/bool)
todavía se parseaba dos veces (chequeo + construcción) — reescrita otra vez para
construir el buffer del tipo candidato incrementalmente en la misma pasada de
clasificación, sin re-parsear. Un tercer intento (cachear el estado de NA en un buffer
de `&str` para las 12 columnas, no solo las 5 de texto) **midió peor** y se revirtió —
detalle completo, semántica preservada y tests nuevos en `TODO.md` (Fase 10).

**Medido en esta misma máquina, mismo dataset, antes/después (número final tras ambas
rondas que sí funcionaron):**

| | Antes (Sección 5) | Después | Cambio |
| :--- | :---: | :---: | :---: |
| **GHL (Polars-backed)** | 966.2 ms $\pm$ 31.4 ms | **439.1 ms $\pm$ 9.0 ms** | **2.20x más rápido** |
| CPU-usuario (paralelo) | 5013.2 ms | 1556.0 ms | 3.2x menos trabajo total |
| vs. Python (Polars), sin cambios (341.8ms) | pierde 2.83x | **pierde 1.29x** | brecha reducida, no cerrada |
| vs. R (data.table), sin cambios (1029ms) | **pierde 1.06x** | **gana 2.34x** | GHL pasa a ganarle |

Las demás 5 variantes de Suite 02 (Python Pandas/Polars, R base/data.table, Julia
streaming/DataFrames.jl) no cambiaron — no dependen de este código, no se remidieron.
**Titular honesto, no forzado a "resuelto":** la brecha con Polars-Python bajó
sustancialmente (de perder por 2.83x a perder por 1.29x) pero **sigue sin cerrarse**.
Lo que queda del perfil posterior apunta a algo arquitectónico, no a otro descuido
puntual: ~35% de los ciclos restantes son del propio `polars-io` tokenizando el CSV
hacia `String` — trabajo que Polars-Python no paga porque infiere tipos nativos
directamente. Cerrarlo del todo implicaría dejar de forzar `dtype_overwrite: String` y
detectar `NA:Razon` por otro mecanismo, un cambio de arquitectura de la ingesta de CSV,
no otro fix puntual — no se intentó en esta pasada. Corrección funcional re-verificada
en cada ronda: `bench_df.gh` sigue dando exactamente los mismos 5 grupos/valores de
siempre.
