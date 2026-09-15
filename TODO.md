# TODO — Roadmap de Ecosistema, Herramientas y Experiencia de Desarrollo (DX)

Este documento define la hoja de ruta integral para transformar el motor de **GHL** en un entorno de desarrollo productivo y ergonómico, enfocado en la integración con **Positron IDE**, soporte de **Quarto & Notebooks**, validación cruzada frente a **R, Python y Julia**, benchmarks abiertos de **DataFrames (H2O.ai / TPC-H)**, **pruebas profundas del sistema (gráficos, autodiff, GPU, PRNG, lógica Kleene y arenas)**, **retos computacionales universales de compiladores (CLBG)**, **validación con diversidad real de DataFrames masivos** y **tooling de producción (`ghl fmt`, fuzzing, CI/CD)**.

> **Histórico de Fases Anteriores:**
> - El roadmap normativo de especificación y paridad de diseño por RFC (RFC 00 a RFC 13) se encuentra completado y archivado en [`docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md`](file:///docs/archive/TODO_RFC_COMPLIANCE_ROADMAP.md).
> - El roadmap de aceleración de kernel, paralelismo y suites empíricas se encuentra archivado en [`docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md`](file:///docs/archive/TODO_PHASES_0_TO_10_BENCHMARKS.md).

---

## Índice de Trabajo

1. [Fase 1: Extensión de Editor para Positron / VS Code (`editors/vscode/`)](#fase-1-extensión-de-editor-para-positron--vs-code)
2. [Fase 2: Language Server Protocol (`ghl lsp`)](#fase-2-language-server-protocol-ghl-lsp)
3. [Fase 3: Soporte Quarto, Notebooks y Flujo Interactivo en Positron](#fase-3-soporte-quarto-notebooks-y-flujo-interactivo-en-positron)
4. [Fase 4: Validación Cruzada de Precisión Numérica (R, Python, Julia & NIST StRD)](#fase-4-validación-cruzada-de-precisión-numérica-r-python-julia--nist-strd)
5. [Fase 5: Benchmarks Abiertos Estándar de DataFrames (H2O.ai & TPC-H)](#fase-5-benchmarks-abiertos-estándar-de-dataframes-h2oai--tpc-h)
6. [Fase 6: Batería de Pruebas Profundas del Sistema (Gráficos, Autodiff, GPU, PRNG, Kleene, Arenas y Cero Fugas)](#fase-6-batería-de-pruebas-profundas-del-sistema)
7. [Fase 7: Retos Computacionales Universales de Compiladores (CLBG & Systems Benchmarks)](#fase-7-retos-computacionales-universales-de-compiladores)
8. [Fase 8: Documentación de Usuario, Guías de Migración y Cookbook](#fase-8-documentación-de-usuario-guías-de-migración-y-cookbook)
9. [Fase 9: Validación Extensiva con Diversidad Real de DataFrames (Stress & Real-World Schemas)](#fase-9-validación-extensiva-con-diversidad-real-de-dataframes)
10. [Fase 10: Formateo Canónico (`ghl fmt`), Fuzzing del Compilador, CI/CD y Distribución](#fase-10-formateo-canónico-ghl-fmt-fuzzing-del-compilador-cicd-y-distribución)

---

## Fase 1: Extensión de Editor para Positron / VS Code

Ubicación: `editors/vscode/` (mismo repositorio, sincronización atómica).

- [ ] **Configuración de Lenguaje (`language-configuration.json`):**
  - [ ] Definir comentarios de línea (`//`) y bloque (`/* ... */`) para habilitar atajos `Ctrl + /` y `Shift + Alt + A`.
  - [ ] Configurar autocierre y coincidencia de parejas: `()`, `[]`, `{}`, `#[ ... ]` y `""`.
  - [ ] Reglas de auto-indentación tras apertura de llaves y delimitadores tabulares (`dataframe [`, `mat [`).
  - [ ] Reglas de plegado de código (*code folding*) para bloques `fn`, `struct`, `impl` y comentarios.
- [ ] **Gramática TextMate (`syntaxes/ghl.tmLanguage.json`):**
  - [ ] Palabras clave y control de flujo: `let`, `mut`, `fn`, `struct`, `trait`, `impl`, `use`, `if`, `else`, `match`, `while`, `for`, `in`, `return`.
  - [ ] Literales semánticos Kleene: `NA` y valores con motivo `NA:Reason` (`constant.language.na.ghl`).
  - [ ] Operadores pipeline y fórmulas estadísticas: `|>` y `~`.
  - [ ] Operadores matriciales y vectorizados: `\`, `.*`, `.+`, `.-`, `./`.
  - [ ] Funciones nativas y verbos estándar destacados: `ols`, `fit`, `summary`, `filter`, `select`, `mutate`, `plot`, `mean`, etc.
  - [ ] Anotaciones de tipos: `f64`, `i64`, `String`, `Bool`, `Vector[T]`, `Matrix`, `DataFrame`, `ModelFit`.
- [ ] **Snippets Ergonómicos (`snippets/ghl.json`):**
  - [ ] Plantilla de función `fn`.
  - [ ] Plantilla de modelo estadístico `ols(y ~ x, df)`.
  - [ ] Plantilla de pipeline de datos `df |> filter(...) |> summarize(...)`.
  - [ ] Plantilla de gráfico `plot(...) |> geom_point() |> show()`.
- [ ] **Manifiesto y Empaquetado VSIX:**
  - [ ] `package.json` con asociación para archivos `.gh` y `.ghl`.
  - [ ] Generación del paquete `ghl-0.1.0.vsix` listo para instalar en Positron (*Install from VSIX*).

---

## Fase 2: Language Server Protocol (`ghl lsp`)

Creación de crate dedicado o subcomando `ghl lsp` para comunicación estándar JSON-RPC sobre stdio.

- [ ] **Infraestructura del Servidor:**
  - [ ] Integración de biblioteca LSP (ej. `tower-lsp`).
  - [ ] Comando CLI `ghl lsp` para iniciar el servidor de lenguaje.
  - [ ] Conexión del cliente en la extensión de VS Code/Positron al binario `ghl`.
- [ ] **Diagnósticos en Tiempo Real (as-you-type):**
  - [ ] Notificación de errores de sintaxis (`textDocument/publishDiagnostics`) en cada cambio de documento.
  - [ ] Subrayado ondulado rojo/amarillo para errores de tipos e inconsistencias de dimensiones matriciales.
- [ ] **Hover Documentation:**
  - [ ] Respuesta a `textDocument/hover`: renderizar tooltip flotante en Markdown con:
    - Firma de la función.
    - Fórmula matemática en notación limpia (ej. $\hat{\beta} = (X^TX)^{-1}X^Ty$).
    - Descripción de parámetros y ejemplo de uso proveniente de `FunctionDoc`.
- [ ] **Autocompletado Contextual:**
  - [ ] Autocompletado de funciones de la biblioteca estándar (`mean`, `ols`, `filter`, etc.).
  - [ ] Autocompletado de variables locales y funciones declaradas en el archivo actual.
  - [ ] Autocompletado inteligente de nombres de columnas en contextos de verbos DataFrames (`filter`, `select`, `mutate`).
- [ ] **Navegación:**
  - [ ] *Go to Definition* (`textDocument/definition`) para saltar a la declaración de funciones y variables de usuario.

---

## Fase 3: Soporte Quarto, Notebooks y Flujo Interactivo en Positron

Habilitar la experiencia de computación científica y *Literate Programming* nativa en Positron:

- [ ] **Envío Interactivo al REPL (`Ctrl + Enter`):**
  - [ ] Comando en la extensión para enviar línea actual o bloque seleccionado al REPL de GHL en la terminal integrada.
- [ ] **Jupyter Kernel de GHL (`ghl kernel`):**
  - [ ] Implementar un kernel ligero que responda al protocolo Jupyter (ZeroMQ / JSON).
  - [ ] Soporte para ejecutar archivos `.ipynb` con kernel GHL dentro de Positron y VS Code.
- [ ] **Soporte Quarto (`.qmd`):**
  - [ ] Habilitar chunks ejecutables ````{ghl}`` en Quarto.
  - [ ] Generación de documentos reproducibles (HTML, PDF, Typst) combinando prosa, código, tablas y gráficos.
- [ ] **Visor de Datos Nativo de Positron (*Data Explorer*):**
  - [ ] Integración con la API de exploración tabular de Positron (`View(df)`).
  - [ ] Transmisión de DataFrames mediante Apache Arrow IPC sin copia de memoria para exploración interactiva en cuadrícula con filtros y ordenamiento.
- [ ] **Panel Gráfico Lateral (*Plots Pane*):**
  - [ ] Salida de gráficos de *Grammar of Graphics* en SVG para visualización inmediata en la pestaña de Plots de Positron.

---

## Fase 4: Validación Cruzada de Precisión Numérica (R, Python, Julia & NIST StRD)

Creación del harness automatizado `tests/cross_validation/` para auditar la precisión de GHL frente a los estándares de referencia de la industria:

- [ ] **Paridad con R (`stats::lm`, `glm`, `MASS`):**
  - [ ] OLS: Coeficientes $\hat{\beta}$, errores estándar, $t$-stat, $p$-values, $R^2$, $R^2$ ajustado, F-statistic, residuos ($|error| < 10^{-10}$).
  - [ ] GLM Logit: Estimaciones IRLS, deviance, log-likelihood, matrices de dispersión ($|error| < 10^{-9}$).
  - [ ] Resúmenes de modelo con paridad exacta en salidas de `summary()`.
- [ ] **Paridad con Python (`statsmodels`, `scipy.stats`):**
  - [ ] Matrices de covarianza robustas frente a heterocedasticidad (HC0, HC1, HC2, HC3).
  - [ ] Álgebra lineal: Singular Value Decomposition (SVD), factorización Cholesky ($|error| < 10^{-10}$).
- [ ] **Paridad con Julia (`GLM.jl`, `LinearAlgebra`):**
  - [ ] Descomposición QR y valores/vectores propios (Eigenvalues / Eigenvectors) en matrices simétricas.
  - [ ] Desempeño y precisión en resolución de sistemas lineales (`A \ b`).
- [ ] **Certificación NIST StRD (Datasets Canónicos de Referencia Estadística):**
  - [ ] Dataset *Pontius* (polinomio de segundo grado).
  - [ ] Dataset *Filippelli* (polinomio de décimo grado).
  - [ ] Dataset *Longley* (problema clásico de multicolinealidad extrema).
  - [ ] Dataset *Wampler* (evaluación de estabilidad numérica frente a errores de redondeo).

---

## Fase 5: Benchmarks Abiertos Estándar de DataFrames (H2O.ai & TPC-H)

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

## Fase 6: Batería de Pruebas Profundas del Sistema

Pruebas especializadas para subsistemas críticos de GHL:

- [ ] **Visual Regression & Renderizado Gráfico:**
  - [ ] *SVG Snapshot Testing*: Comparación de árboles SVG generados contra archivos "golden" canónicos (puntos, líneas, histogramas, boxplots).
  - [ ] *Stress Rendering*: Medición de latencia y uso de memoria graficando $100,000$ puntos en scatter plot y $1,000,000$ en histograma vs `ggplot2` y `matplotlib`.
  - [ ] Validación de mapeo estético exacto (`aes`) para escalas continuas, discretas y paletas de color.
- [ ] **Diferenciación Automática (Taylor Test / Gradient Checking):**
  - [ ] Verificación de gradientes exactos de `grad()` y `value_and_grad()` contra aproximación numérica central de Taylor:
    $$\frac{f(x + \epsilon) - f(x - \epsilon)}{2\epsilon} \approx \nabla f(x) \quad (\text{tolerancia } \epsilon = 10^{-7}, \text{error relativo} < 10^{-6})$$
  - [ ] Pruebas de convergencia de descenso de gradiente en funciones de prueba clásicas (*Rosenbrock*, *Rastrigin*) contra JAX y Julia `ForwardDiff.jl`.
- [ ] **Arenas Regionales de Memoria (`bumpalo` / RFC 03):**
  - [ ] *Desalocación Instantánea O(1)*: Comprobar que tras alocar 10,000 vectores y matrices en un `arena::scope`, la liberación sea en tiempo constante $O(1)$ sin destructores per-objeto.
  - [ ] *Cero Fragmentación de Heap*: Ejecutar 100,000 ciclos continuos de `reset()` en bucle verificando que la memoria RSS no crezca.
  - [ ] *Aislamiento y Seguridad de Punteros*: Garantizar que los valores asignados dentro de una arena no puedan escapar del scope ni provocar *use-after-free*.
  - [ ] *Arenas Thread-Local en Rayon*: Comprobar que cada hilo de paralelismo tenga su propia arena sin contención de locks ni condiciones de carrera.
- [ ] **Generación Pseudoaleatoria y Distribuciones (Batería PRNG):**
  - [ ] Test formal de bondad de ajuste Kolmogorov-Smirnov y $\chi^2$ sobre $1,000,000$ de muestras de `random_normal` y `random_gamma` ($p > 0.05$).
  - [ ] Verificación de independencia estadística y ausencia de autocorrelación serial entre streams paralelos generados con `Xoshiro256PlusPlus::jump()`.
- [ ] **GPU vs CPU (WGPU & Shaders):**
  - [ ] Paridad numérica GPU-CPU en `matmul` para matrices de $1024 \times 1024$ y $2048 \times 2048$ ($\|C_{\text{gpu}} - C_{\text{cpu}}\| < 10^{-5}$).
  - [ ] Determinación del punto de cruce de rendimiento (*latency crossover point*) para transferencias de datos CPU <-> GPU.
- [ ] **Lógica Kleene 3VL y Semántica de Valores Faltantes:**
  - [ ] Matriz exhaustiva $3 \times 3$ ($\text{True}, \text{False}, \text{NA}$) para todos los operadores lógicos (`&&`, `||`, `!`) y relacionales (`==`, `!=`, `<`, `>`).
  - [ ] Trazabilidad de motivos semánticos (`NA:Reason`) en pipelines multi-etapa complejos (Filter -> Mutate -> Impute -> Summarize).
- [ ] **Zero-Leak & Estabilidad a Largo Plazo:**
  - [ ] Prueba de resistencia de $1,000,000$ de iteraciones midiendo que el Resident Set Size (RSS) en RAM se mantenga estrictamente plano.
  - [ ] *Parquet / Arrow Round-Trip Interoperability*: Escribir y leer archivos Parquet y streams Arrow entre GHL, Python Polars y R `arrow` verificando integridad binaria bit a bit.

---

## Fase 7: Retos Computacionales Universales de Compiladores

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

---

## Fase 8: Documentación de Usuario, Guías de Migración y Cookbook

Creación del directorio `docs/guides/` con material didáctico para usuarios finales:

- [ ] **Guía de Migración para Usuarios de R (`docs/guides/ghl_for_r_users.md`):**
  - [ ] Tabla de equivalencias de funciones: `lm()` -> `ols()`, `glm()` -> `fit_logistic()`, `summary()`, `predict()`.
  - [ ] Comparativa Tidyverse vs GHL: `%>%` / `|>` con `filter()`, `mutate()`, `group_by()`.
  - [ ] Manejo de `NA`: del `is.na()` tradicional a los motivos semánticos `NA:Reason` y lógica de Kleene.
- [ ] **Guía de Migración para Usuarios de Python / Pandas (`docs/guides/ghl_for_python_users.md`):**
  - [ ] Comparativa Pandas/Polars vs DataFrames GHL.
  - [ ] Equivalencias de Álgebra Lineal: NumPy/SciPy vs GHL `dot()`, `\`, `cholesky()`, `qr()`.
  - [ ] Sintaxis de expresiones funcionales frente a métodos de objetos.
- [ ] **Tutorial Rápido "GHL en 15 Minutos" (`docs/guides/quickstart.md`):**
  - [ ] Instalación del binario y uso del REPL.
  - [ ] Hola Mundo estadístico: carga de CSV, filtrado, regresión OLS y visualización.
- [ ] **The GHL Statistical Cookbook (`docs/guides/cookbook.md`):**
  - [ ] Receta 1: Limpieza e imputación de datos faltantes con motivos semánticos (`impute`, `filter_na_reason`).
  - [ ] Receta 2: Regresión Lineal Robusta con errores estándar corregidos por heterocedasticidad (HC0 a HC3).
  - [ ] Receta 3: Clasificación Binaria con Regresión Logística (IRLS).
  - [ ] Receta 4: Agrupamiento no supervisado con Modelos de Mezclas Gaussianas (GMM / EM).
  - [ ] Receta 5: Muestreo Bayesiano MCMC y Bootstrap paralelo reproducible.

---

## Fase 9: Validación Extensiva con Diversidad Real de DataFrames

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

---

## Fase 10: Formateo Canónico (`ghl fmt`), Fuzzing del Compilador, CI/CD y Distribución

Cerrar la brecha de calidad de ingeniería y entrega continua del lenguaje:

- [ ] **Formateador Canónico de Código (`ghl fmt`):**
  - [ ] Subcomando `ghl fmt <file.gh|file.ghl>` que utiliza el pretty-printer del AST para normalizar automáticamente la indentación a 4 espacios y los saltos de línea canónicos.
  - [ ] Bandera `--check` para integración en pipelines de CI (falla si el archivo no está formateado).
  - [ ] Integración con Positron / VS Code para formateo automático al guardar (`editor.formatOnSave`).
- [ ] **Fuzz Testing del Compilador (`cargo-fuzz` / AFL):**
  - [ ] Generación de entradas de bytes pseudoaleatorias y sintaxis maliciosa contra `ghl-syntax` y `ghl-types`.
  - [ ] Invariante: Garantizar $0$ *panics*, $0$ *crashes* y $0$ desbordamientos de búfer ante cualquier código malformado, retornando siempre diagnósticos limpios.
- [ ] **CI/CD Automatizado con GitHub Actions:**
  - [ ] Matriz de compilación y tests automáticos en cada pull request para **Windows** y **Linux** (con runners automáticos de macOS en la nube para releases sin requerir hardware Mac local).
  - [ ] Alerta automatizada de regresión de rendimiento: Notificar si algún commit degrada los benchmarks empíricos más de un 5%.
- [ ] **Empaquetado y Distribución Automatizada:**
  - [ ] Script de instalación en una línea para entornos Unix/Linux: `curl -fsSL https://ghl-lang.org/install.sh | sh`.
  - [ ] Manifiesto de instalación para Windows (`winget` o script PowerShell automatizado).
  - [ ] Publicación automática de binarios precompilados y del paquete `.vsix` en los Releases de GitHub.
