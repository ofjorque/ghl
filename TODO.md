# TODO — Roadmap hacia paridad con `benchmarks/`

Este documento junta lo que falta implementar para que GHL pueda correr realmente
las suites descritas en `benchmarks/suites/` y cumplir lo especificado en
`docs/design/*.md`. Está ordenado de forma **estratégica**: cada fase habilita
o simplifica las siguientes, no por prioridad de "lo más importante primero".

> Contexto clave (no repetir el error): `docs/design/08-compiler-implementation-stack.md`
> especifica explícitamente qué crates debe usar `ghl-runtime` — `polars`/`arrow2`
> (motor columnar + joins + lazy), `faer` (álgebra lineal), `rayon` (paralelismo),
> `statrs` + `rand_distr` (distribuciones), `rand_xoshiro` (PRNG reproducible),
> `bumpalo` (arenas). **Ninguna está en `Cargo.toml` hoy.** El runtime actual es
> un intérprete tree-walking artesanal sobre `Value` / `Vec<Value>` / `HashMap`.
> Fase 0 ya resolvió esto — se adoptan directamente (ver decisión abajo) — y esa
> decisión determina el diseño de todo lo demás, por eso va primero.

---

## Fase 0 — Arquitectura: decisión tomada
Bloquea todo lo demás: el diseño de `Value::DataFrame` / `Value::Matrix` depende de esto.

> **Decisión (2026-09-07) — Opción A:** `Value::DataFrame` envuelve un `polars::DataFrame`
> real desde el día uno; los verbos (`select`, `filter`, `group_by`, `summarize`, `arrange`,
> joins, ...) llaman directo a la API/expresiones de polars. Se descartó un híbrido
> (DataFrame chico en `Value` + `LazyFrame` grande en polars aparte): mantener dos
> representaciones duplica cada verbo, contradice cómo se usan pandas/polars/dplyr en la
> práctica (misma representación para explorar 10 filas y procesar millones), y el propio
> RFC 08 ya especifica adoptar polars directamente, no un híbrido.
>
> Por qué no es tanto trabajo como parece: el diseño de `col_ctx`/`ColRef`/`AggSpec`/
> named-args (`summarize(n = count(), mean_x = mean(x))`) ya commiteado es una capa de
> *sintaxis y orden de evaluación*, no depende de que el DataFrame sea un
> `HashMap<String, Vec<Value>>`. Lo que cambia es la *ejecución* dentro de
> `crates/ghl-runtime/src/io.rs` (traducir la lista de `AggSpec` a
> `.group_by(keys).agg([...])` de polars y envolver el resultado en `Value::DataFrame`),
> no el parser, el AST, ni `col_ctx` en `eval.rs`/`checker.rs`.

- [x] Agregar a `Cargo.toml` de `ghl-runtime`: `polars-core` 0.55.2, `faer` 0.24.4,
      `rayon` 1.12.0, `statrs` 0.19.1, `rand_distr` 0.6.0, `rand` 0.10.2,
      `rand_xoshiro` 0.8.1, `bumpalo` 3.20.3. Todas resuelven sin conflicto de versión
      real: `rand`/`rand_xoshiro`/`rand_distr`/`statrs` coinciden en la línea
      `rand_core 0.10.x` (verificado en `Cargo.lock`), así que un mismo
      `Xoshiro256PlusPlus` sirve tanto para `rand_distr` como para `statrs`.
- [x] **Spike #1 — resultado: GO.** `crates/ghl-runtime/examples/spike_polars_latency.rs`
      (`cargo run --release --example spike_polars_latency -p ghl-runtime`). Construir +
      filtrar + `group_by().mean()` sobre un `polars::DataFrame`:
      10 filas ≈ 40µs · 1.000 filas ≈ 71µs · **100.000 filas ≈ 1.64ms** (construcción
      896µs + filter 144µs + group_by/mean 601µs). Muy por debajo del presupuesto de
      <25ms de punta a punta de Suite 04 (Prueba C) — **no hace falta un camino
      separado para DataFrames chicos, Opción A queda confirmada a todas las escalas.**
- [x] **Spike #2 — resultado: GO.** `crates/ghl-runtime/tests/spike_na_reason.rs`
      (`cargo test -p ghl-runtime --test spike_na_reason`, 3/3 passing). Confirma que el
      side-channel de razones de NA (ver diseño abajo) reindexa correctamente tras un
      `filter`: la razón de una fila que sobrevive se reasigna a su nueva posición: la
      razón de una fila descartada desaparece sin filtrarse a la fila equivocada; un
      filtro no-op deja la tabla de razones intacta.
- [x] **Capa de conversión en los bordes — implementada y probada** (aún no enchufada a
      `Value::DataFrame`, ver nota):
  - `crates/ghl-runtime/src/na_reasons.rs` — `NaReasonTable` de producción (`set`/`get`/
    `reindex`/`rename_column`/`retain_columns`), generaliza el mecanismo validado en el
    Spike #2 para que también lo usen `rename()`/`select()`/`drop()`, no solo `filter()`.
  - `crates/ghl-runtime/src/polars_bridge.rs` — `build_dataframe(cols)` (construcción:
    `Vec<(String, Vec<Value>)>` → `polars::DataFrame` + `NaReasonTable`, ensanchando cada
    columna al tipo más permisivo presente: `String` > `f64` > `i64` > `bool`) y
    `pull_column_as_values(frame, na_reasons, col)` (extracción: columna de polars →
    `Vec<Value>`, reconstruyendo `NA:razon` donde exista). 7 tests cubriendo round-trip
    por tipo, ensanche `i64`+`f64` → `f64`, columnas enteramente NA, y columna inexistente.
  - **Actualización:** ya enchufada — ver Fase 1 abajo, `Value::DataFrame` es
    `{ frame: polars::DataFrame, na_reasons: NaReasonTable }` desde el commit `d711827`.
  - **Matrix:** sigue pendiente pero casi gratis — `Value::Matrix { rows, cols, data: Vec<f64> }`
    ya es compatible con `faer::Mat<f64>` (buffer contiguo), la conversión es un wrap directo.
- [x] **Divergencia con el RFC — resuelta (2026-09-07).** La cita original ("RFC 03/09")
      estaba mal — lo que especifica el diseño de motivos de NA es **RFC 02 §2.5**, y sí
      divergía: el RFC pedía un diccionario columnar `u8` por índice nulo; se implementó
      un `HashMap<(col, row), String>` de strings planos. Decisión: mantener el `HashMap`
      (RFC 02 §2.5 actualizado para documentarlo) — el objetivo real de los motivos es
      que paquetes externos de análisis de datos perdidos los puedan consumir, y para eso
      importa que sea liviano/simple/exponible, no la codificación más compacta posible.
      Se cerró el hueco real que esto exponía: **nada del lenguaje podía leer `na_reasons`
      todavía** — se agregaron `na_reason(x)` (motivo de un valor) y `na_reasons(df, col)`
      (`Vector` de motivos alineado a la columna, usable con cualquier verbo existente:
      `filter`, `count`, `group_by`, sin API de consulta nueva).

## Fase 1 — Motor columnar + Joins (`benchmarks/suites/02`)
Es lo que el usuario pidió primero y lo que más impacto tiene sobre el resto de la Fase de datos.

- [x] Reemplazar `Value::DataFrame { columns: Vec<String>, data: HashMap<String, Vec<Value>> }`
      por `Value::DataFrame { frame: polars_core::frame::DataFrame, na_reasons: NaReasonTable }`.
      Los ~30 `df_*` de `io.rs` fueron reescritos contra la API de polars (`select`,
      `head`/`tail` vía `take`, `sort`/`arrange` con comparador propio + `take`, `rename`,
      `drop_many`, `unique`-equivalente a mano para `distinct` con reindexado de razones,
      `group_by().get_groups()` recomputado en `summarize` ya que `GroupBy<'a>` no puede
      vivir dentro de un `Value` propio). La capa de sintaxis (`col_ctx`, `AggSpec`,
      named-args) no cambió — se confirmó lo que decía la Fase 0. `neko.rs`/`env.rs`
      (fit/tidy/glance/augment/predict/plot) migrados vía un shim de compatibilidad
      (`polars_bridge::dataframe_to_columns_and_data`) sin tocar sus internos numéricos.
      Los 21 tests de DataFrame preexistentes siguen pasando sin cambiar sus aserciones.
- [x] `inner_join(left, right, on1, on2, ...)` / `left_join(...)` — usan
      `DataFrame::join` de `polars-ops` (`polars-ops` se sumó a Fase 0 solo para esto,
      `polars-core` no trae joins). Columnas `on` aceptan bare/`ColRef`/string igual que
      el resto de verbos. Razón de NA se descarta en el resultado del join a propósito:
      un join puede duplicar o descartar filas de origen, así que el reindexado posicional
      simple de `filter`/`arrange`/`slice` no aplica, y la API eager seleccionada no
      expone qué fila(s) de origen generó cada fila de salida.
- [x] `group_by`/`summarize`: la agrupación (`get_groups()`) viene de polars; la
      evaluación de cada `AggSpec` sigue reutilizando `native_mean`/`native_sum`/etc.
      sobre el subconjunto extraído por grupo — **no** es todavía el camino 100%
      zero-copy vía agregaciones nativas de polars por serie. Ver ítem nuevo abajo.
- [ ] Optimización pendiente (no bloqueante): `summarize()` extrae cada grupo a
      `Vec<Value>` antes de agregar (ver `compute_agg` en `io.rs`) en vez de usar
      agregaciones nativas de polars por `Series` sin boxear — el Spike #1 ya midió
      600µs para 100k filas con el camino ingenuo de polars, así que esto no es
      urgente, pero es la primera optimización real cuando se mida contra Suite 02.
- [ ] CSV/Parquet a escala GB: usar los lectores de `polars`/`arrow2`
      (`read_csv`/`read_parquet`, ya multi-hilo) en vez de extender el parser propio de
      `crates/ghl-runtime/src/io.rs` — ese parser sigue siendo el usado hoy (vía
      `polars_bridge::build_dataframe` al final), queda solo para el camino chico.
- [ ] Generar el CSV sintético de 5GB / 25M filas / 12 columnas mixtas (Caso 2.1)
      + script reproducible para generarlo (no versionar el archivo en sí).
- [ ] Filtrado vectorial con bitmask de validez + asignación copy-on-write (Caso 2.3) —
      el backend columnar ya existe; falta medir/optimizar el camino de filtrado a escala.
- [ ] Actualizar `docs/design` / `benchmarks/suites/02-dataframe-operations.md`:
      su `query.gh` de ejemplo usa method-chaining (`df.filter(...).group_by(...).parallel().agg([...])`)
      que no coincide con la sintaxis real de pipes (`df |> filter(...) |> group_by(...)`).
      Decidir: reescribir el doc a la sintaxis real, o agregar azúcar de method-chaining al lenguaje.

## Fase 2 — Motor "lazy" (consultas diferidas)
Depende del backend columnar de la Fase 1; sin él no hay nada que optimizar de forma diferida.

- [ ] Diseñar un `LazyFrame`/plan de consulta que difiera la ejecución de una
      cadena `select |> filter |> group_by |> summarize` hasta un `.collect()`.
- [ ] Optimizador básico: predicate/projection pushdown (filtrar/seleccionar
      columnas antes de materializar).
- [ ] Decidir si esto reemplaza el modo eager actual o convive como opt-in
      (`lazy(df) |> ... |> collect()`).

## Fase 3 — Álgebra lineal de alto rendimiento (`benchmarks/suites/01`)
Independiente de las Fases 1-2, puede avanzar en paralelo una vez resuelta la Fase 0.

- [ ] Reemplazar `crates/ghl-runtime/src/matrix.rs` (implementación a mano) por
      `faer` (Fase 0): LU, QR, Cholesky, SVD, autovalores.
- [ ] Multiplicación matricial densa multinúcleo por bloques (Caso 1.2).
- [ ] SIMD real (AVX2/AVX-512/NEON) para dot product y reducciones sobre `Vector` (Caso 1.1).
- [ ] Fusión de operaciones elemento-a-elemento sin buffers intermedios en heap —
      soporte real para `x.map(xi => log(1.0 + exp(-abs(xi))) + sin(xi))` (Caso 1.4).
- [ ] Métodos/sintaxis: `Vector::random_uniform(n)`, `.dot(&b)`, `.map(...)`.

## Fase 4 — Paralelismo transversal (RFC 05)
Se apoya en `rayon` (Fase 0); habilita el resto de casos de Suite 03.

- [ ] Iteradores work-stealing (`.par_iter()` o equivalente) sobre `Vector`.
- [ ] Paralelismo automático/opt-in en verbos de DataFrame (`.parallel()` antes de `.agg()`).
- [ ] Bootstrap paralelo sin duplicar la muestra base (memoria compartida, Caso 3.3).

## Fase 5 — RNG + distribuciones + arenas (`benchmarks/suites/03`, RFC 03 §2.2)
Requisito para todo el modelado estadístico de la Suite 03.

- [ ] PRNG reproducible bit-a-bit entre plataformas (`rand_xoshiro` o equivalente),
      con `PRNG::seed(seed)`.
- [ ] Biblioteca de distribuciones (`Normal`, `Gamma`, ...) con `.sample(&mut rng)`
      (`statrs`/`rand_distr` o implementación propia).
- [ ] Arenas regionales (`bumpalo`) para bucles iterativos (MCMC/bootstrap) con
      cero asignaciones de heap por iteración — sintaxis `arena::scope(|a| { ... })`.
- [ ] Modelo de memoria ARC + Copy-on-Write real (RFC 03 §2.1): hoy el intérprete
      clona `Vec`/`HashMap` en casi cada verbo, es lo opuesto a CoW.

## Fase 6 — Modelado estadístico avanzado (`benchmarks/suites/03`)
Depende de Fase 5 (RNG/distribuciones/arenas) y de Fase 3 (solver matricial).

- [ ] Gibbs sampler jerárquico de ejemplo (Caso 3.1) corriendo de punta a punta.
- [ ] IRLS para GLM / regresión logística sobre N=1M, P=40 (Caso 3.2).
- [ ] Algoritmo EM para mezclas gaussianas con log-sum-exp estable (Caso 3.4).

## Fase 7 — Sistema de módulos y superficie de sintaxis estática
El token `use` ya existe en el lexer (`crates/ghl-syntax/src/lexer.rs`) pero el parser
no lo maneja — bloquea cualquier ejemplo de la documentación que use `use std::...::{...}`.

- [ ] `use std::modulo::{A, B};` — resolución de módulos/namespacing real.
- [ ] Namespacing de la stdlib bajo `std::dataframe`, `std::stats::distributions`,
      `std::stats::rng`, etc. (hoy todo son funciones globales sin namespace).
- [ ] Evaluar si hacen falta genéricos estáticos (`Vector<f64>`, `Matrix<f64>`),
      referencias (`&`, `&mut`) y `Option`/`Result` con `.unwrap()` — aparecen en
      los ejemplos de los RFCs pero son un cambio grande al sistema de tipos;
      decidir alcance real antes de implementar.

## Fase 8 — Backend AOT y distribución (`benchmarks/suites/04`, RFC 03 §3.2)
Puede avanzar en paralelo a partir de Fase 0; no depende de las fases de datos/estadística.

- [ ] `ghl build --release` real: hoy solo aparece en el texto de ayuda del CLI
      (`crates/ghl-cli/src/main.rs`), no hay código detrás. `ghl-codegen` solo
      tiene JIT vía Cranelift (`cranelift-jit`), falta emisión de objeto nativo
      (`cranelift-object` o similar) + linkeo a binario standalone.
- [ ] Medir y optimizar hacia las metas de Suite 04: arranque <25ms, binario
      8-20MB, RSS base 3-8MB (AOT) / ~25MB (REPL).
- [ ] Tiering de ejecución intérprete → JIT → AOT (RFC 03 §3.1) coherente con
      lo anterior.

## Fase 9 — GPU (`std::gpu`, RFC 05 §4)
Lo más especulativo y grande; sin diseño concreto todavía. Al final a propósito.

- [ ] Diseñar API `std::gpu` (RFC 05 §4) — hoy es solo una sección de la RFC,
      sin prototipo ni crate elegido.
- [ ] Evaluar backend (wgpu, CUDA vía FFI, etc.) — decisión abierta.

## Fase 10 — Cierre: benchmarks reales
Solo tiene sentido al final, cuando ya hay algo que medir.

- [ ] Implementar `methodology.md` (aislamiento de CPU, `hyperfine`, etc.) como
      script/harness reproducible.
- [ ] Correr las 4 suites contra R/Python/Julia y publicar resultados reales en
      `benchmarks/comparisons/`.
