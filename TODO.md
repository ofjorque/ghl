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
      el resto de verbos.
  - [x] **`na_reasons` preservado a través del join — hecho.** La API eager de
        `DataFrame::join` no expone qué fila(s) de origen generó cada fila de salida, así
        que `df_join` (`io.rs`) construye esa provenance a mano: agrega una columna
        `__ghl_left_idx__`/`__ghl_right_idx__` (vía `with_row_index`) a cada lado *antes*
        de unir — el join las lleva como cualquier otra columna de datos — y las usa
        después para reconstruir el `NaReasonTable` del resultado, columna por columna,
        leyendo la razón desde el lado (izquierdo o derecho) y la fila original correcta.
        Las columnas no-clave del lado derecho que colisionan de nombre con el izquierdo
        se renombran a mano con el mismo sufijo `_right` que usaría polars por default,
        pero decidido por este código (no adivinado post-join a partir del nombre), así el
        mapeo columna-de-salida → columna-de-origen es exacto. Una fila de `left_join` sin
        match del lado derecho no fabrica una razón (no hay fila de origen que la tuviera).
        Test nuevo: `test_join_preserves_na_reasons_on_both_sides` (cubre ambos lados,
        `inner`/`left`, y el caso de colisión de nombres). Verificado de punta a punta con
        `ghl run`.
- [x] `group_by`/`summarize`: la agrupación (`get_groups()`) viene de polars; cada
      `AggSpec` se evalúa con una pasada fusionada nativa de polars por `(kind, col)`
      distinto (ver el ítem de fusión más abajo, ahora hecho).
- [x] **Optimización de `summarize()` — hecha, y encontró un bug de correctitud real
      en el camino.** `compute_agg` (`io.rs`) ya no extrae cada grupo a `Vec<Value>`
      antes de agregar: toma el subconjunto del grupo como `Column` nativo
      (`column.take(&idx_ca)`) y llama `mean_reduce`/`sum_reduce`/`min_reduce`/
      `max_reduce`/`median_reduce`/`std_reduce`/`var_reduce`/`n_unique` de polars
      directo — sin pasar por `Value` para nada salvo el resultado final escalar.
  - **Medido:** `group_by(category) |> summarize(4 agregaciones)` sobre 1M filas /
    5 grupos: 44-50ms (~4.4-5ms proyectado a 100k filas). El Spike #1 original midió
    ~601µs para 100k filas con **una sola** agregación nativa vía la API eager de
    polars (`GroupBy::select().mean()`) — la diferencia (~8x) es real y esperada:
    esta implementación hace `take()` + reduce **por grupo por agregación** (aquí
    4 grupos × 4 specs = 20 llamadas nativas), no una sola pasada fusionada de
    polars sobre todos los grupos a la vez. Con alta cardinalidad de grupos (Suite
    02 pide 100.000) ese overhead por-llamada empieza a importar — anotado abajo
    como el siguiente paso real si se mide que hace falta, no asumido.
  - **El bug que encontró la verificación de punta a punta (no los tests unitarios):**
    las reducciones nativas de polars **saltan los NA por defecto** (estilo
    pandas/numpy) — exactamente el comportamiento "silencioso" que RFC 02 §2.4
    dice explícitamente que GHL NO hace (`mean(data)` con un NA debe dar `NA`, no
    promediar el resto). El primer intento de este cambio pasó los 32 tests
    existentes sin problema, **porque ninguno de ellos combinaba `summarize()` con
    datos NA** — recién al probarlo a mano con `NA:SensorDropout` en una columna
    agrupada apareció la regresión (`mean_x`/`max_x` calculados ignorando el NA en
    vez de dar `NA`). Arreglado con un chequeo `subset.null_count() > 0` (O(1),
    metadata de Arrow, no un escaneo) antes de reducir para las agregaciones que
    deben propagar NA (`mean`/`sum`/`std_dev`/`var`/`min`/`max`/`median`) — `count`/
    `n_distinct`/`first`/`last` no lo necesitan, igual que antes de esta optimización.
    Se agregó `test_summarize_propagates_na_kleene_style` para que esto no vuelva a
    pasar inadvertido. **Lección concreta:** el chequeo automatizado no reemplaza
    probar con datos que tengan NA de verdad cuando se toca código de agregación.
- [x] **Optimización de seguimiento — hecha, sobre `polars-lazy` (sin API deprecada).**
      `df_summarize` (`io.rs`) reemplazó el `take()`+reduce por-grupo-por-agregación de
      `compute_agg` por **una sola query de `polars-lazy`**: `frame.lazy().group_by_stable
      (keys).agg([...])`, con un `Expr` por cada `(kind, col)` distinto que aparece en los
      `specs` de `summarize()` (`col(x).mean()`, `col(x).sum()`, etc.), más un
      `col(x).null_count()` por cada columna que participa en una agregación que debe
      propagar NA (ver más abajo) y un `col(row_idx).first()` para poder recuperar la fila
      representante de cada grupo — todo evaluado en una única pasada del motor de queries
      de polars sobre TODOS los grupos a la vez.
  - **Primer intento (revertido): API eager deprecada + `#[allow(deprecated)]`.** La
    primera versión de este arreglo reusaba los métodos de reducción eager de `GroupBy`
    (`.mean()`, `.sum()`, etc.), que están deprecados en `polars-core` 0.55.2 desde 0.24.1
    ("use polars.lazy aggregations"), silenciando el warning del compilador con
    `#[allow(deprecated)]`. Se rechazó explícitamente: silenciar un warning no es resolver
    el problema, solo esconderlo. No hay alternativa eager no-deprecada que fusione una
    agregación sobre todos los grupos en una sola pasada nativa — la única salida real es
    `polars-lazy`, así que se sumó como dependencia nueva de `ghl-runtime` (antes de lo
    planeado; se iba a introducir recién en Fase 2) y se reescribió sobre eso. El spike
    `spike_polars_latency.rs` (Fase 0), que ya usaba esa misma API eager deprecada, se
    migró también por consistencia. `cargo build --workspace --examples` queda limpio, cero
    warnings de deprecación en todo el crate.
  - **La preocupación de "requiere manejar joins entre N resultados parciales" que este
    ítem tenía anotada resultó no aplicar:** las expresiones de agregación viven todas
    dentro del mismo `.agg([...])` de la misma query, así que sus resultados ya vienen
    alineados por posición en el mismo orden de grupo — concatenar columnas alcanza, no
    hace falta un join real por claves.
  - **Kleene NA sigue propagando** (la regresión que este mismo ítem había encontrado
    arriba): por cada columna fuente que participa en una agregación que debe propagar NA
    (`mean`/`sum`/`std_dev`/`var`/`min`/`max`/`median`) y que de hecho tiene algún nulo
    (`column.null_count() > 0`, chequeo O(1) que se salta la columna entera si no aplica),
    se agrega `col(x).null_count().alias(...)` a la misma query — sin helper column ni
    `group_by()` aparte, la expresión de conteo de nulos corre en la misma pasada fusionada
    que la agregación real. Si ese conteo es >0 para un grupo, esa celda del resultado se
    reemplaza por `NA` sin adivinar una razón (igual que antes).
    `test_summarize_propagates_na_kleene_style` y `test_group_by_summarize_unquoted_columns`
    siguen pasando sin cambiar sus aserciones, y se verificó de nuevo a mano con `ghl run`
    (grupo con un `NA:SensorDropout` en la columna agregada → `mean`/`max` de ese grupo dan
    `NA`, los otros grupos no se ven afectados).
  - **Medido (`spike_summarize_high_cardinality.rs`, dataset sintético en memoria de 100.000
    grupos, la cardinalidad que Suite 02 pide — no se reusa el de 5 grupos del Caso 2.1
    porque no ejercita el escenario que este ítem apunta):** 2M filas / 4 agregaciones
    (`count`, `mean`, `max`, `sum`) en **164.68ms** (~12.1M filas/s) con `polars-lazy` — más
    rápido incluso que la primera versión sobre la API eager deprecada (321.79ms, misma
    máquina/dataset), probablemente porque el optimizador de queries fusiona las 4
    expresiones (3 agregaciones + `first()` del índice de fila, sin helper column de nulos
    en este dataset porque no tiene NA) en un único recorrido físico en vez de 3-4 pasadas
    `GroupBy` separadas. No es una comparación controlada (no es lo que este ítem pedía
    medir), solo un dato adicional de que la migración no salió cara.
- [x] **CSV a escala — `read_csv()` reescrito, y luego reescrito otra vez para sacar el
      cuello de botella real.** Se sumó `polars-io` (con feature `csv`) a Fase 0.
      `read_csv_file` (`io.rs`) usa `CsvReadOptions` de polars-io para la tokenización
      multi-hilo del archivo, forzando `dtype_overwrite` a `String` en todas las columnas
      — así polars solo hace la parte cara (leer + partir en campos) — y la inferencia de
      tipos + `NA:razon` de GHL corre encima, columna por columna en paralelo vía `rayon`
      (ya en Fase 0). `parse_csv()` (texto en memoria, `parse_csv_string`) se dejó con el
      parser viejo a propósito — no es el camino de escala GB.
  - **Primer intento: mejora real pero parcial (~1.8x), cuello de botella identificado y
    dejado como subitem abierto.** `infer_and_convert_column` producía `Vec<Value>` por
    celda (un `String` propio por cada valor, boxeado en `Value::String`/`Value::F64`/etc.)
    — con `build_dataframe` reboxeando eso otra vez a la `ChunkedArray` final, una columna
    de texto llegaba a clonar cada celda **tres veces** (`StringChunked` de polars-io →
    `Vec<String>` → `Value::String` → `Vec<Option<String>>` final) antes de llegar a su
    forma definitiva.
  - **Segundo intento (esta pasada): arreglado de raíz.** `infer_and_convert_column_native`
    (`io.rs`) hace la misma inferencia de tipo (i64 > f64 > bool > string, ensanchando ante
    cualquier duda, política sin cambios) pero directo sobre el `StringChunked` que entrega
    polars-io, construyendo la `ChunkedArray`/`Column` final en un solo paso — sin pasar por
    `Vec<Value>` en absoluto. Las razones `NA:razon` se recolectan en la misma pasada a una
    lista rala `(fila, razón)`, no re-escaneando un `Vec<Value>` después como hacía
    `build_dataframe`. `parse_csv_string` (el camino que no es de escala) sigue usando la
    versión vieja basada en `Vec<Value>` sin cambios — no valía la pena tocarla.
  - **Medido con `generate_synthetic_csv` + `spike_csv_ingest_latency`, 1M filas/12
    columnas/84MB:** parser viejo 4.6s (18 MB/s) — sin cambios, no es el camino tocado —
    → primer intento 2.6s (32 MB/s, ~1.8x) → **segundo intento ~0.85-0.90s (95-99 MB/s,
    ~5.2x contra el parser viejo, ~3x contra el primer intento)**, medido 3 veces para
    confirmar que no era ruido de caché de disco (la primera corrida en frío dio 1.81s,
    las siguientes ya con el archivo en caché del SO se estabilizaron en ese rango).
    Verificado además con `test_read_csv_file_matches_parse_csv_string` (misma salida byte
    a byte que el parser viejo, incluyendo razones de NA — no solo más rápido, sigue siendo
    exactamente correcto) y de punta a punta con `ghl run` sobre `read_csv(...)`.
  - **CSV sintético reproducible:** `crates/ghl-runtime/examples/generate_synthetic_csv.rs`
    (parametrizado por filas; `cargo run --release --example generate_synthetic_csv -p
    ghl-runtime -- 25000000 target/synthetic_25m.csv` para el caso completo de Suite 02).
    El archivo generado no se versiona. Medido a 1M filas por tiempo/disco en este sandbox
    compartido — `benchmarks/methodology.md` exige hardware aislado para números
    "oficiales" de todas formas, así que la corrida completa de 25M queda para esa etapa
    (Fase 10), no para esta medición de spike.
- [x] **Parquet — hecho, resultado: el salto real que CSV no dio.** `read_parquet(path)`/
      `write_parquet(df, path)` (`io.rs::read_parquet_file`/`write_parquet_file`, feature
      `parquet` de `polars-io`). Sin el problema de `NA:razon` de CSV — Parquet ya tiene su
      propio bit de validez nativo, así que es una lectura directa y tipada, sin el híbrido
      string-luego-inferir que necesitó CSV. `write_parquet()` avisa (no falla) si el
      DataFrame tiene razones de NA registradas, porque Parquet no tiene dónde guardarlas
      — se pierden a propósito, documentado, no silenciosamente.
  - **Medido sobre el mismo archivo de 1M filas:** lectura en **61ms** (16.4M filas/s,
    206 MB/s) — **~43x más rápido que el `read_csv()` nuevo (2.6s) y ~75x más rápido que
    el parser viejo (4.6s).** En disco, 12.6 MB vs 84.2 MB del CSV (**6.7x más chico**).
    Esto sí es el salto de rendimiento que CSV no pudo dar (ver nota de arriba sobre el
    boxing a `Vec<Value>`) — Parquet no tiene que parsear texto ni inferir tipos en
    absoluto, así que ese cuello de botella directamente no existe en este camino.
  - Test de round-trip (`test_parquet_round_trip`, `lib.rs`) + verificado de punta a
    punta con `ghl run` (`read_csv → write_parquet → read_parquet`, tamaños reales).
- [x] **Filtrado vectorial (Caso 2.3) — hecho para predicado simple `col OP escalar`.**
      `filter(df, col(...) > 75.0)` (`io.rs::colref_predicate_mask` +
      `df_filter_by_col_predicate`) ya no boxea la columna a `Vec<Value>` y compara celda
      por celda: compara nativo en polars (`Column::gt`/`lt`/`equal`/etc. contra un
      `Column::Scalar` — broadcast real, sin materializar N copias del escalar) y da
      directo un `BooleanChunked`, que se reindexa/aplica con el mismo `take_rows` que
      usan `arrange`/`slice`/`sample_n`/`distinct`. `eval_predicate` (comparación escalar
      vieja) se eliminó por completo, no quedó como código muerto.
  - **Medido sobre el archivo de 1M filas:** columna `i64` (`value_a > 500000`): nuevo
    67ms vs viejo 99ms — **~1.5x, modesto**. Columna string (`category == "A"`): nuevo
    27ms (no se rehizo el camino viejo para strings, pero por lo ya medido en la ingesta
    de CSV — donde boxear un string implica clonar un `String` en heap por celda, a
    diferencia de un `i64` que es una copia trivial dentro del enum — la ganancia ahí
    debería ser bastante mayor). Ambos casos son rápidos en términos absolutos (<100ms/1M
    filas) independientemente de la mejora relativa.
  - **Predicados compuestos — ahora soportados.** El ejemplo exacto del Caso 2.3
    (`score > 75.0 && !is_na(category)`) corre tal cual. Ver el detalle completo en
    "Mismatch de sintaxis del doc" más abajo — el hueco que esa sección documentaba
    (`is_na()`/`!`/`&&`/`||` dentro de `filter()`) se arregló de raíz, no se dejó como
    limitación aceptada.
  - [ ] "Asignación sin copia (Copy-on-Write)" real de la descripción del Caso 2.3 no se
        abordó — `take_rows`/`Column::Scalar` ya evitan las copias evitables que estaban al
        alcance sin rediseñar el modelo de memoria completo. Depende del modelo ARC +
        Copy-on-Write real de Fase 5 (RFC 03 §2.1, ítem ya existente ahí) — se deja anotado
        acá también para que no se pierda de vista al leer Fase 1 en diagonal y parecer
        "hecho" del todo.
- [x] **Mismatch de sintaxis del doc — resuelto: se reescribió a la sintaxis real, no
      se agregó azúcar de method-chaining.** Reescribir el lenguaje para soportar
      `df.filter(...).group_by(...)` hubiera sido mucho trabajo por una sola sección de
      un doc, cuando la sintaxis de pipes ya está establecida y probada en ~30 verbos.
      `benchmarks/suites/02-dataframe-operations.md` ahora tiene un `query.gh` real que
      corre tal cual (verificado con `ghl run`, no solo escrito a mano), cubriendo los
      Casos 2.2/2.3/2.4 con `read_csv`/`filter`/`group_by`/`summarize`/`mutate`/
      `inner_join`/`left_join` reales.
  - **Se agregó `is_na(x)` como builtin** (antes solo existía `Value::is_na()` a nivel
    de Rust, ningún script GHL podía llamarlo) — vectorizado sobre `Vector` igual que
    los demás helpers de math/string.
  - **Se encontró un hueco real de diseño al escribir el ejemplo del Caso 2.3 tal como
    lo describe el enunciado original** (`score > 75.0 && !is_na(category)`), y **se
    arregló de raíz en vez de dejarlo documentado como limitación aceptada** — el primer
    intento de cerrar este ítem solo documentaba el hueco (`is_na(col)` envolvía el
    `ColRef` en una llamada de función *antes* de la comparación, `filter()` perdía la
    referencia a la columna, y probado a mano con `filter(is_na(category) == false)` no
    filtraba nada: 4 de 4 filas sobrevivían cuando debía filtrar 2 — un *fallo silencioso*,
    exactamente lo que RFC 00 prohíbe). Documentar eso como "hueco conocido" en vez de
    arreglarlo fue rechazado explícitamente: un `filter()` que a veces no filtra sin avisar
    no es un límite de alcance aceptable, es un defecto de diseño.
  - **El arreglo real:** un pequeño árbol de predicados diferidos en `Value`
    (`IsNaPredicate(String)`, `NotPredicate(Box<Value>)`, `AndPredicate`/`OrPredicate`,
    junto al `ColPredicate` que ya existía). `is_na(col(x))` ahora produce
    `IsNaPredicate("x")` en vez de perder la referencia a la columna; `eval.rs` construye
    `Not`/`And`/`Or` sobre estos predicados en vez de sobre `Bool` cuando cualquiera de los
    operandos es un predicado diferido (`is_predicate()`); y `io::predicate_mask` resuelve
    el árbol completo a un solo `BooleanChunked` reusando el álgebra booleana nativa de
    polars (`!`, `&`, `|` sobre `BooleanChunked` — ya soportan Kleene 3-valores para NA
    reales del lado de polars, no hubo que reimplementar lógica de 3 valores a mano).
    También se agregó al parser (`ghl-syntax`) el operador unario `!` (`Token::Bang`), que
    hasta ahora no se conectaba a ningún `ExprKind` a pesar de que el lexer y el evaluador
    ya lo esperaban — sin este cambio `!is_na(...)` ni siquiera parseaba.
  - **`filter()` ahora falla en vez de pasar de largo en silencio:** si el segundo
    argumento no es ninguno de (comparación de columna, `is_na(...)`, una combinación de
    esos con `!`/`&&`/`||`, o un `Vector[Bool]`), `filter()` devuelve un diagnóstico
    (`C0202`) en vez de devolver el DataFrame sin filtrar — cerrando la clase completa de
    bug que motivó este arreglo, no solo el caso puntual de `is_na`.
  - Tests nuevos: `test_filter_is_na_predicate_and_negation`,
    `test_filter_compound_predicate_and_or`,
    `test_filter_rejects_unrecognized_predicate_instead_of_silently_passing_through`
    (`lib.rs`). Verificado además de punta a punta con `ghl run`: `!is_na(category)`,
    `is_na(category)`, `score > 75.0 && !is_na(category)` y
    `score > 90.0 || category == "B"` sobre datos con NA simple y `NA:SensorDropout`,
    todos con el resultado esperado fila por fila.

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
