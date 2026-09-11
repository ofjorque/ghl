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
  - [x] **"Asignación sin copia (Copy-on-Write)" — hecho, acotado a `Value::DataFrame`/
        `GroupedDataFrame` (no el modelo ARC+CoW completo de Fase 5).** El CoW real de
        RFC 03 §2.1 (`Vector`/`Matrix`/`HashMap`, todo el intérprete) sigue siendo trabajo
        de Fase 5 — este ítem se cerró con un alcance deliberadamente más chico, decidido
        junto al usuario, después de que la primera respuesta ("queda abierto, depende de
        Fase 5 completa") no fuera aceptable: perder de vista un ítem real detrás de una
        dependencia grande tampoco es una solución.
    - **El diseño:** `na_reasons` pasó de `NaReasonTable` (dueño) a
      `Arc<NaReasonTable>` en `value.rs`. Como todos los ~30 verbos de este módulo son
      puramente funcionales (piden prestado su `Value` de entrada y devuelven uno nuevo,
      nunca mutan un `Value` que otro binding todavía sostiene), compartir la tabla vía
      `Arc` y clonar el `Arc` (no su contenido) cuando la salida de un verbo mantiene las
      mismas razones sin cambios es siempre seguro — no hace falta un disparador de
      "clonar recién al mutar" (`Arc::make_mut`) porque nunca hay una mutación en el lugar
      que vigilar: un verbo que necesita razones distintas simplemente construye una tabla
      nueva antes de envolverla. `frame: DataFrame` no necesitó el mismo tratamiento:
      `Column`/`Series` de polars ya son `Arc` por dentro, así que `DataFrame::clone()` ya
      era barato (clona el vector de columnas, no los datos de fila).
    - `take_rows`/`build_dataframe` (los dos puntos que ya centralizaban casi toda la
      construcción de `NaReasonTable`) ahora envuelven en `Arc` internamente, así que la
      inmensa mayoría de los ~30 verbos de `io.rs` no necesitó ningún cambio — solo los que
      construyen una tabla nueva a mano por fuera de esos dos caminos (`df_join`,
      `df_select`, `df_rename`, `df_drop`, `df_mutate`, `df_fill_na`, `read_parquet_file`,
      la nueva `read_csv_file`) necesitaron envolver su resultado en `Arc::new(...)` — el
      compilador señaló cada uno de esos sitios exactos al primer intento de build, cero
      quedó sin cubrir.
    - **Verificado que la propiedad es real, no solo "sigue andando":** test nuevo
      `cloning_a_dataframe_value_shares_the_na_reasons_arc_instead_of_deep_cloning`
      (`polars_bridge.rs`) usa `Arc::ptr_eq` para confirmar que clonar un `Value::DataFrame`
      comparte el mismo puntero de `NaReasonTable`, no que produce una tabla igual por
      casualidad. Las 39 pruebas de `ghl-runtime` (incluidas las de joins/CSV de este mismo
      pase) y el test suite completo del workspace siguen pasando sin cambiar aserciones, y
      se verificó de nuevo a mano con `ghl run` sobre `select`/`rename`/`drop`/`mutate`/
      `fill_na`/joins — las razones se preservan/descartan exactamente igual que antes.
    - **Medido (`spike_dataframe_clone_latency.rs`, comparando clonar la tabla completa
      -- el costo real que se pagaba antes en cada lookup de variable, cada binding, cada
      paso por valor de un `Value::DataFrame` -- contra clonar el `Arc`):** con 200.000
      celdas `NA:razón` registradas (peor caso, todas las celdas), 31.05ms → ~16ns. Con
      10.000: 453.66µs → ~16ns. Con 1.000: 42.63µs → ~16ns. Con 100: 8.63µs → ~46ns. El
      camino viejo es O(N) en el número de razones registradas; el nuevo es O(1) sin
      importar cuántas haya — el "speedup" crece con N a propósito, no es un número mágico
      fijo, y en la práctica (pocas decenas o cientos de razones típicas, no 200k) la
      diferencia absoluta ya es de microsegundos a nanosegundos por cada clonado de
      `Value`, que ocurre con mucha frecuencia en un intérprete tree-walking.
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

## Fase 2 — Motor "lazy" (consultas diferidas con `polars-lazy`) — Hecho
Depende del backend columnar de la Fase 1; sin él no hay nada que optimizar de forma diferida.

- [x] **Diseñar un `LazyFrame`/plan de consulta que difiera la ejecución de una
      cadena `select |> filter |> group_by |> summarize` hasta un `.collect()`.**
      Implementado con `Value::LazyFrame { plan: LazyPlan, na_reasons: Arc<NaReasonTable> }`
      y `Value::GroupedLazyFrame { plan: LazyPlan, na_reasons: Arc<NaReasonTable>, keys: Vec<String> }`.
      `LazyPlan` es un newtype wrapper transparente sobre `polars_lazy::frame::LazyFrame`
      con `Deref`/`DerefMut` y formato legible.
- [x] **Optimizador: predicate pushdown, projection pushdown y plan inspection con `explain()`.**
      Al deferir a `polars-lazy` y compilar con `polars-stream`, el optimizador de Polars
      combina proyecciones (`select`), filtra filas antes de cargarlas (`filter`), y empuja
      las operaciones hacia los scans de disco. `explain(lf)` devuelve el plan de ejecución
      lógico optimizado como String para auditoría.
- [x] **Decisión y retrocompatibilidad: convive como opt-in (`lazy(df) |> ... |> collect()`).**
      Se adoptó el modelo opt-in (igual que Polars y dplyr/dbplyr): el código existente corre
      en modo eager con cero cambios ni penalizaciones de rendimiento, mientras que
      `df |> lazy()` habilita el pipeline diferido. Se añadieron además `scan_csv(path)` y
      `scan_parquet(path)` para escaneos lazy directamente desde almacenamiento en disco.
      Los verbos diferidos soportados incluyen `filter`, `select`, `mutate`, `arrange`,
      `head`, `tail`, `group_by`, `summarize`, `inner_join`, `left_join`, y `ungroup`.
      Test suite dedicado en `crates/ghl-runtime/tests/test_lazy_engine.rs` (8 tests de punta
      a punta) y 177 tests pasando en todo el workspace.

## Fase 3 — Álgebra lineal de alto rendimiento (`benchmarks/suites/01`)
Independiente de las Fases 1-2, puede avanzar en paralelo una vez resuelta la Fase 0.

> **Hallazgo al arrancar (2026-09-08):** el alcance real de esta fase era más grande de lo
> que las 5 líneas de abajo sugerían. `MatrixOps::mul` (multiplicación real de matrices)
> **existía pero no lo llamaba nada** — ni el operador `*` ni ningún builtin — así que el
> Caso 1.2 no estaba implementado en absoluto, no solo lento. Tampoco existían
> `transpose`/`det`/decomposiciones/`dot` para vectores/`map` fusionado/`random_uniform` —
> Fase 3 era más "construir de cero" que "reemplazar lo que hay". Y el doc de referencia
> (`benchmarks/suites/01`) usa sintaxis de method-chaining (`a.dot(&b)`, `x.map(xi => ...)`)
> que el parser no soporta (no hay `.method()` en la gramática) — el mismo "mismatch de
> sintaxis" que Fase 1 ya encontró para DataFrames. Se decidió con el usuario partir la fase
> en dos tracks: **Matrices primero** (acotado, se cierra en esta pasada) y **Vector
> después** (más grande de lo que parecía — ver la nota debajo del track de Vector).

### Track 1: Matrices — hecho

- [x] **Reemplazar `matrix.rs` por `faer`: LU (`solve`), QR, Cholesky, SVD, autovalores —
      hecho.** `MatrixOps::solve` (usado por el operador `A \ b` y por el ajuste OLS de
      `neko.rs`) pasó de eliminación gaussiana a mano a `faer`'s `partial_piv_lu()` +
      `.solve()`, con la misma detección de matriz singular (S0101) leída del pivote mínimo
      de la diagonal de `U` después de factorizar, en vez de durante la eliminación —
      mismo comportamiento observable, backend real. `MatrixOps::elementwise` (usado por
      `.+`/`.-`/`.*`/`./`) se dejó como estaba a propósito: ya es un loop plano
      auto-vectorizable por LLVM, `faer` no le aporta nada (no hay descomposición ni kernel
      de bloques involucrado).
  - **Nuevo: `qr(m)`/`qr_q`/`qr_r`, `cholesky(m)`, `svd(m)`/`svd_u`/`svd_s`/`svd_v`,
    `eigen(m)`/`eigen_values`/`eigen_vectors`.** GHL no tiene sintaxis de acceso a campos
    (`resultado.q`) ni tuplas, así que cada descomposición devuelve un `Value` dedicado
    (`QrDecomp`/`SvdDecomp`/`EigenDecomp`, mismo patrón que `ModelFit`) con funciones libres
    de acceso — consistente con `na_reason()`/`na_reasons()` de Fase 0/1, no method-chaining.
    `cholesky()` devuelve directo un `Matrix` (solo hay `L`, no hace falta envoltorio).
  - **`eigen()` está acotado a matrices simétricas a propósito, no es una limitación
    oculta:** `faer`'s `self_adjoint_eigen()`/`llt()` (Cholesky) **solo leen un triángulo y
    asumen que el otro lo espeja, sin validarlo** — pasarles una matriz asimétrica
    silenciosamente factoriza/diagonaliza una matriz *distinta* (la simetrizada), no la que
    el usuario pidió. Se agregó un chequeo de simetría explícito (`check_symmetric`) antes
    de llamar a `faer` en ambos casos, con error S0412 claro — exactamente el tipo de "no
    silent state" que el resto del proyecto ya exige en todos lados. Autovalores/vectores
    complejos (matrices no-simétricas en general) quedan fuera: `Value` no tiene tipo
    complejo todavía.
  - **`A * B` (Caso 1.2) — conectado por primera vez.** El operador `*` entre dos
    `Value::Matrix` caía antes al error genérico "cannot apply Mul" (`MatrixOps::mul`
    nunca se llamaba desde ningún lado). Ahora despacha a `MatrixOps::mul`, que usa el
    operador `*` de `faer::Mat` — su propio kernel GEMM multinúcleo por bloques (los
    crates `gemm`/`gemm-f64` que ya estaban en el árbol de dependencias transitivo) — así
    que la "multiplicación matricial densa multinúcleo por bloques" que pedía el ítem
    original sale del mismo cambio, no hizo falta implementarla a mano.
  - Tests nuevos (`lib.rs`): `test_matrix_multiplication_real_product`,
    `test_matrix_multiplication_rejects_non_conformable_dimensions`,
    `test_qr_decomposition_reconstructs_original_matrix`,
    `test_cholesky_reconstructs_symmetric_positive_definite_matrix`,
    `test_cholesky_rejects_asymmetric_matrix`,
    `test_cholesky_rejects_non_positive_definite_matrix`,
    `test_eigen_symmetric_matrix`, `test_eigen_rejects_asymmetric_matrix`,
    `test_svd_reconstructs_matrix` — cada uno verifica la reconstrucción numérica real
    (`Q*R ≈ A`, `L*Lᵀ ≈ A`, `U·diag(S)·Vᵀ ≈ A`), no solo que no crashee. Los tests
    preexistentes de `\` (`test_eval_matrix_solve_gaussian`,
    `test_eval_singular_matrix_emits_s0101`) y de OLS/vcov en `neko.rs`
    (`test_neko_ols_fit_and_projections`, `test_neko_vcov_hc3`,
    `test_neko_singular_matrix_emits_s0101`) siguen pasando sin cambiar sus aserciones —
    el swap de backend es transparente. Verificado además de punta a punta con `ghl run`.

### Track 2: Vector (SIMD real, fusión, sintaxis)
Reordenado (2026-09-08) para que lo estructural vaya primero — Punto 1 es el prerequisito
real de los Puntos 2 y 3, no un ítem independiente más.

- [x] **Punto 1 — Migrar `Value::Vector` a una representación plana y tipada — hecho.**
      `Value::Vector(Vec<Value>)` boxeaba cada elemento individualmente — el mismo
      problema que tenía `Value::DataFrame` antes de Fase 0/1. Ahora es
      `Value::Vector(VectorData)` (`vector_data.rs`, nuevo), donde `VectorData` envuelve
      un `polars_core::Column` (mismo backend que `DataFrame`, reusando toda la
      inferencia de tipos y el side-channel de `NA:razon` ya construidos) más un
      `Arc<NaReasonTable>` propio.
  - **Estrategia de shim para no reescribir las ~35 funciones existentes de una:**
    `VectorData` implementa `Deref<Target = Vec<Value>>`, materializando perezosamente
    (una sola vez, cacheado en un `Arc<OnceLock<Vec<Value>>>` — clonar `VectorData` es
    siempre barato, se haya materializado o no) el `Vec<Value>` boxeado que el código
    viejo espera. Los ~35 sitios que ya hacían `Value::Vector(items) => ... items.iter()
    ...`/`for item in items` compilaron sin tocar su lógica interna (solo `for item in
    items` directo — sin `.iter()` — necesitó el insert mecánico de `.iter()`, unas 15
    veces, señalado exacto por el compilador). Los ~25 sitios de *construcción*
    (`Value::Vector(vec_de_values)`) sí necesitaron un cambio de una línea a
    `Value::Vector(VectorData::from_values(vec_de_values))` — también señalados exactos
    por el compilador, cero quedaron sin cubrir. El código numérico nuevo (`MatrixOps::
    solve`, `rank()`, `residuals()`/`coef()`, `svd_s()`/`eigen_values()`) usa en cambio
    `VectorData::from_f64(Vec<f64>)`, que no boxea nada — el camino que de verdad importa
    para Punto 2/3.
  - **Bug real encontrado (no solo "quedó compilando"):** un `Vector` de un solo elemento
    string (ej. `["id"]`, el patrón más común para pasar nombres de columna a
    `select`/`drop`/`rename`) se corrompía a `"\"id\""` (comillas literales embebidas) —
    `df |> drop(["id"])` no rompía, pero tampoco dropeaba nada, porque buscaba una columna
    llamada `"id"` (con comillas) que no existía. Causa: polars representa un `Column` de
    longitud 1 como un `ScalarColumn` internamente, cuyo `.get()` devuelve
    `AnyValue::StringOwned` en vez del `AnyValue::String(&str)` prestado que devuelve una
    columna normal de varias filas — variante que `any_value_to_plain_value`
    (`polars_bridge.rs`) no manejaba, cayendo a un fallback genérico que usa `Display`
    (que cita los strings) para tipos "sin contraparte en GHL". Arreglado agregando el
    arm que faltaba. Esto no es un bug nuevo de esta migración — ya existía para
    cualquier `Column`/`Series` de una sola fila con datos de texto — pero recién se
    manifestó al ejercitar vectores de un solo elemento intensivamente. Tests nuevos:
    `single_element_string_vector_round_trips_without_quote_corruption`,
    `from_f64_round_trips_without_boxing_na` (`vector_data.rs`).
  - Las 50 pruebas de `ghl-runtime` (incluida `test_dataframe_tidyverse_pipeline`, que
    ejercitaba `drop(["id"])` y fue la que hizo saltar el bug de arriba) y el test suite
    completo del workspace pasan sin cambiar aserciones. Verificado de nuevo a mano con
    `ghl run` sobre `select`/`rename`/`drop`/`mutate`/`fill_na` — comportamiento idéntico
    al de antes de la migración.
- [x] **Punto 2 — SIMD real para `dot()` y reducciones sobre `Vector` (Caso 1.1) — hecho.**
      `dot(a, b)` (nuevo, no existía ni rápido ni lento) usa `MatrixOps::dot` — `RowRef *
      ColRef` de `faer` (`row/rowref.rs:63`/`col/colref.rs:67` para los constructores
      `from_slice` sin copia, `linalg/mat_ops.rs:1005` para el `Mul` que internamente llama
      a `crate::linalg::matmul::matmul` — el mismo kernel GEMM bloqueado/multinúcleo que ya
      usa `MatrixOps::mul` de Track 1), no un loop a mano. `VectorData` (`vector_data.rs`)
      ganó `as_f64_view()` (`&[f64]` sin copia vía `ChunkedArray::cont_slice()` cuando el
      `Column` ya es `Float64`/un solo chunk/sin nulos; cast+rechunk como fallback si no —
      sigue evitando el boxing celda por celda), `null_count()` (O(1)), `first_na()`
      (detecta el primer nulo y su razón sin materializar `Vec<Value>`, solo cuando
      `null_count() > 0`), y `value_at(i)` (lee una sola celda sin materializar todo).
  - **Migradas al camino nativo sobre `Column`** (mismo patrón que `compute_agg` de
    Fase 1, ya probado): `mean`, `sum`, `var` (con lo que `std_dev` sale gratis, sigue
    delegando en `var`), `min`, `max`, `median`, `first`, `last`.
  - **Decisión de diseño explícita, no un detalle menor:** `mean`/`sum`/`var`/`min`/`max`/
    `median` siguen propagando la *razón específica* del primer NA encontrado (ej.
    `NA:SensorDropout`), no un `NA(None)` genérico — a diferencia de `summarize()`'s
    agregación por grupo (que sí descarta la razón a propósito, ahí "qué razón gana entre
    filas de un grupo colapsado" no está definido). Acá no hay esa ambigüedad: un solo
    `Vector`, un solo NA que encontrar. Este comportamiento ya existía antes de este punto
    pero **no tenía ningún test que lo fijara** — se agregó
    `test_mean_sum_preserve_na_reason_over_vector` para que no se pierda en silencio en
    una futura migración.
  - **Efecto secundario encontrado y aceptado, no ignorado:** `min()`/`max()` antes
    forzaban `F64` sin importar el tipo de entrada (`min([1,2,3])` daba `1.0`, no `1`);
    ahora preservan el dtype nativo del `Column` igual que `sum()` ya hacía y que
    `summarize()` ya hace — `min([1,2,3])` da `1` (I64). No había ningún test que fijara
    el comportamiento viejo, y el nuevo es más consistente con el resto del lenguaje, así
    que se dejó así a propósito en vez de forzar F64 artificialmente para "no cambiar
    nada".
  - **`n_distinct()` — en su momento explícitamente NO migrado, más tarde sí (ver bloque
    "Migrar el resto de funciones..." más abajo).** El `format!("{:?}", it)` que usaba
    para deduplicar resultó ser, al revisarlo de nuevo, un bug de correctness (no una
    distinción intencional que preservar): contaba cada *razón* de NA distinta como un
    valor distinto, algo que ningún otro lugar del lenguaje hace. Se migró a
    `Column::n_unique()` nativo de polars, que colapsa cualquier NA a lo sumo a un valor
    faltante — el fix correcto, no una pérdida de una distinción válida.
  - Tests nuevos: `test_dot_product_real_computation`,
    `test_dot_product_rejects_mismatched_lengths`,
    `test_dot_product_propagates_na_with_reason`,
    `test_mean_sum_preserve_na_reason_over_vector` (`lib.rs`). Las 54 pruebas de
    `ghl-runtime` y el test suite completo del workspace pasan sin cambiar aserciones
    preexistentes. Verificado de punta a punta con `ghl run`.
  - **Medido (`spike_vector_dot_latency.rs`, 10⁷ elementos — la escala que pide Caso
    1.1):** `dot()` a mano sobre `Vec<Value>` boxeado, 138-144ms → `dot()` real (`faer`,
    `VectorData::as_f64_view`), 9.7-9.9ms — **~14-15x**, estable en 3 corridas.
  - **Lista actualizada de qué sigue en el camino lento (boxeado)** — actualizada de
    nuevo tras el bloque "Migrar el resto de funciones de `Vector`..." (ver más abajo,
    después de Fase 4): a esta altura ya quedan migrados `map_numeric_fn`/`map_string_fn`,
    `cumulative`, `sort_vector`, `rank`, `between`, `UnaryNeg` y `n_distinct` — de la lista
    original solo siguen boxeados **`lag`/`lead`** e **`if_else`**, documentado
    explícitamente por qué en ese mismo bloque (no en silencio).
- [x] **Punto 3 — Fusión de `map` sin buffers intermedios en heap (Caso 1.4) — hecho.**
      `map(x, f)` no existía en absoluto (ni rápido ni lento) y **no se podía implementar
      como una función nativa común**: todas las ~100 funciones nativas de GHL son
      `fn(Vec<Value>) -> Result<Value, Diagnostic>` (`value.rs`) — un puntero a función
      plano, sin acceso al intérprete — y `map()` necesita *invocar* `f` (una `Closure` o
      un `NativeFn`) una vez por elemento, algo que solo sabe hacer
      `Interpreter::call_value` (`eval.rs`, método privado).
  - **Nuevo tipo de función nativa:** `Value::NativeFnCtx(fn(&mut Interpreter, Vec<Value>)
    -> Result<Value, Diagnostic>)`, junto a `NativeFn`. `call_value` gana una rama
    (`Value::NativeFnCtx(func) => func(self, args)`). `value.rs` pasa a referenciar
    `crate::eval::Interpreter` (dos módulos hermanos refiriéndose mutuamente a los tipos
    del otro — válido en Rust, no es un ciclo de crates) — primer uso de este mecanismo,
    reusable para una futura segunda función de orden superior si aparece.
  - **Hallazgo real antes de que esto fuera rápido de verdad:** llamar a `call_value` una
    vez por elemento (`call_value(f.clone(), ...)` en loop) habría clonado el
    `RuntimeEnv` completo capturado por la clausura **una vez por elemento** — la rama
    `Closure` de `call_value` consume el env capturado en cada llamada
    (`push_scope`/correr el body/descartarlo). Para un vector grande eso es un costo real,
    no solo overhead menor — exactamente el tipo de trampa que esta sesión viene cazando
    en cada punto. Arreglado clonando el env de la clausura **una sola vez** antes del
    loop, un solo `push_scope()`, y adentro del loop solo reasignar el parámetro
    (`RuntimeEnv::set`, un `HashMap::insert`) + dos `mem::replace` (baratos, no clonan
    nada) por elemento.
  - El resultado se junta en un `Vec<Value>` (vía `VectorData::from_values`), no un
    `Vec<f64>` directo: `f` es código arbitrario y puede dar `NA` para algunos elementos
    (ej. `log(-1.0)` ya lo hace hoy vía `map_numeric_fn`), así que la salida no está
    garantizada 100% `f64` de antemano. La fusión real que pedía el Caso 1.4 es evitar
    los **buffers intermedios por sub-operación** (`abs`, `exp`, suma, `log`, `sin`, suma
    final — seis `Vector` completos si se encadenan los helpers existentes), no eliminar
    el boxeo de la salida final, que es inherente a que `f` devuelve un `Value`
    arbitrario. Un `Vec<f64>` optimista con fallback a boxeado queda anotado como posible
    refinamiento futuro, no implementado ahora (sin un caso medido que lo justifique).
  - **`sin`/`cos` no existían y se agregaron** — el enunciado mismo del Caso 1.4
    (`... + sin(xi)`) los necesita para correr de verdad, no una versión simplificada.
  - Tests nuevos: `test_map_applies_closure_over_vector` (el ejemplo exacto del Caso 1.4),
    `test_map_with_native_fn` (`map(v, sqrt)`, pasando un builtin existente en vez de una
    clausura), `test_map_preserves_per_element_na`, `test_map_rejects_non_vector_first_argument`,
    `test_map_rejects_non_callable_second_argument`. Las 59 pruebas de `ghl-runtime` y el
    test suite completo del workspace pasan sin cambiar aserciones preexistentes.
    Verificado de punta a punta con `ghl run`.
  - **Dos huecos reales de la aritmética de `Vector` encontrados al escribir el spike de
    comparación, decididos y arreglados en un pase aparte (mismo día) — hecho.**
    - [x] `escalar + Vector` no estaba soportado — el broadcasting de escalar (`eval.rs`)
          solo manejaba el orden `Vector op escalar`, no el inverso (`1.0 + v` fallaba con
          "Cannot apply Add to f64 and Vector"; `v + 1.0` sí funcionaba). Se agregó la rama
          simétrica para `+`/`-`/`*`/`/`, con la dirección correcta para las no
          conmutativas: `5.0 - v` da `[5.0-v[0], ...]`, no `[v[0]-5.0, ...]`.
    - [x] `Vector + Vector` con `+` liso no estaba soportado (solo `.+`/`.-`/`.*`/`./`).
          **Decisión tomada:** `+`/`-`/`*`/`/` entre dos `Vector`s del mismo largo ahora
          se comportan como sus versiones con punto (elemento a elemento) — incluido `*`,
          sin excepción. Esto sigue a propósito la convención de R/NumPy/Julia (`*` entre
          vectores es elemento a elemento en los tres; el producto punto se pide con una
          función aparte) en vez de replicar la distinción que sí tiene sentido para
          `Matrix` (`*` = producto matricial real, `.* ` = Hadamard — esa decisión de
          Track 1 queda intacta, no se tocó). Refactor: la lógica de `.+`/`.-`/`.*`/`./`
          para Vector-Vector se extrajo a `vector_elementwise_op` (`eval.rs`), reusada
          ahora por ambos caminos (con punto y sin punto) en vez de duplicarse.
    - Tests nuevos: `test_scalar_vector_arithmetic_is_symmetric`,
      `test_vector_vector_plain_operators_are_elementwise`,
      `test_vector_vector_plain_operators_reject_mismatched_lengths`. Las 62 pruebas de
      `ghl-runtime` y el test suite completo del workspace pasan sin cambiar aserciones
      preexistentes. Verificado de punta a punta con `ghl run`, incluyendo que
      `Matrix * Matrix` sigue dando el producto real (no se vio afectada por este cambio,
      que es exclusivo de `Vector`).
  - **Medido (`spike_map_fusion_latency.rs`, N=2×10⁶ — no los 5×10⁷ que pide el
    enunciado del Caso 1.4 al pie de la letra: el camino "encadenado" aloca **seis**
    `Vec<Value>` completos a la vez a propósito, que es justo lo que se está midiendo, y
    con 5×10⁷ un primer intento terminó en un **OOM-kill real** confirmado por `dmesg`
    en este sandbox compartido de 15GB — 2×10⁶ ya deja ver el efecto con margen de
    sobra):** encadenado 2.50-2.52s → `map()` fusionado 1.90-1.99s, **~1.26-1.33x**,
    estable en 3 corridas. **Nota honesta:** la ganancia acá es mucho más modesta que el
    ~14x de `dot()` (Punto 2) a propósito — ahí se reemplazaba un loop boxeado por SIMD
    real; acá el costo dominante en ambos caminos sigue siendo interpretar el árbol de
    expresión elemento por elemento (`eval_expr` recursivo), y lo único que cambia es
    evitar cinco pasadas extra de asignación de `Vector` completo. Reportado tal cual
    salió, no lo que se esperaba de antemano.
- [x] **Punto 4 — Superficie de sintaxis:** `random_uniform(n)` implementado como función
      libre (`env.rs::native_random_uniform`, registrada en el prelude y en el checker de
      `ghl-types`), vía `rand::rng().random::<f64>()` (uniforme en `[0, 1)`) — versión
      mínima no reproducible entre corridas, a propósito: roza con Fase 5 ("PRNG
      reproducible bit-a-bit... con `PRNG::seed(seed)`"), que es donde le toca la
      reproducibilidad real; esto solo desbloquea los benchmarks de Suite 01.
      `map(x, f)` y `dot(a, b)` ya habían salido de los Puntos 2/3 como funciones libres,
      como corresponde.
    - Reescrito `benchmarks/suites/01-vector-and-matrix-math.md` (el nombre real del
      archivo — el texto anterior de este punto lo llamaba, por error,
      `01-vector-and-matrix-algebra.md`): el bloque de referencia GHL usaba
      `Vector::random_uniform(n)` (estilo método estático), `a.dot(&b)` y `x.map(xi => ...)`
      (method-chaining con flecha de lambda `=>`) — nada de eso existe en GHL, que no tiene
      sintaxis de método (`.foo()`) en absoluto, solo pipes y funciones libres, y cuya
      lambda real es `\param -> expr` (confirmado en su momento vía
      `ghl-syntax::parser::test_parse_lambda`). Mismo tipo de "mismatch de sintaxis del
      doc" que ya le corrigió Fase 1 al de DataFrames. Reescrito a
      `random_uniform(n)` / `dot(a, b)` / `x |> map(\xi -> log(1.0 + exp(-abs(xi))) + sin(xi))`.
    - Verificado de punta a punta con el CLI de release (`ghl run`) sobre un script que
      ejercita los 4 casos de Suite 01 con la sintaxis final del doc: dot product,
      multiplicación de matrices (`*` real vía faer), Cholesky, y el `map` fusionado tanto
      en su forma de función libre (`map(x, f)`) como en pipe (`x |> map(f)`) — las dos
      formas producen el mismo resultado, confirmando que el pipe simplemente inserta `x`
      como primer argumento.
      Tests nuevos en `lib.rs`:
      `test_random_uniform_produces_vector_of_requested_length_in_unit_interval`,
      `test_random_uniform_rejects_negative_length`.

## Fase 4 — Paralelismo transversal (RFC 05)
Se apoya en `rayon` (Fase 0); habilita el resto de casos de Suite 03.

- [x] **Iteradores work-stealing (`.par_iter()` o equivalente) sobre `Vector`:**
      `vector_elementwise_op` (`eval.rs`, compartido por `.+`/`.-`/`.*`/`./` y, desde el fix
      de Fase 3, los operadores planos `+`/`-`/`*`/`/` Vector-Vector) tenía dos problemas
      antes de este punto, no solo la falta de paralelismo: pagaba boxing completo a
      `Vec<Value>` incluso en el caso 100% numérico sin NA, vía el `Deref` de `VectorData`.
      Ahora tiene tres caminos:
      - Numérico sin NA (`null_count() == 0` en ambos lados) y `len < 50_000`: secuencial
        sobre `&[f64]` vía `as_f64_view()` (mismo mecanismo zero-copy que `dot()` usa desde
        el Punto 2) — cero boxing, un solo hilo.
      - Numérico sin NA y `len >= 50_000`: igual que arriba pero repartido con
        `rayon::prelude::*`'s `par_iter().zip()`.
      - Cualquier tamaño con al menos un NA de cualquiera de los dos lados: el loop
        original sobre `Vec<Value>`, sin tocar — sigue siendo el único lugar con la lógica
        Kleene por-elemento (si `a` es NA, resultado es `a`; si `b` es NA, resultado es
        `b`). No se paralelizó a propósito: no es el caso que benchmarquea Suite 01 y
        tocar esa rama sin necesidad medida era riesgo sin beneficio.
      - **`PARALLEL_THRESHOLD = 50_000` no es adivinado** — medido con
        `examples/spike_vector_elementwise_parallel_latency.rs`: a N=50.000 el camino
        paralelo todavía va ~0.91x del secuencial (empate técnico), a N=75.000 ya es
        ~1.23x más rápido, y a N=5.000.000 llega a ~2.9x. Por debajo de ~30.000 el overhead
        de despacho de rayon lo hace directamente peor que el secuencial (a N=1.000, el
        secuencial es ~66x más rápido que el paralelo) — de ahí que el threshold exista en
        primer lugar, no sea "paralelizar siempre".
      - **Nota honesta del primer intento del spike:** un primer intento midiendo hasta
        N=20.000.000 terminó en un **OOM-kill real** (confirmado por `journalctl -k`:
        `anon-rss:11563088kB`) en este sandbox de 15GB. Causa, medida y no adivinada:
        `size_of::<Value>() == 160 bytes` (mucho más de lo estimado a ojo en sesiones
        previas), y a diferencia de `dot()` (input+input+un `f64` escalar), una op
        elementwise produce un vector de salida `O(n)` completo — el camino "boxed" del
        spike sostenía hasta 3-4 buffers de tamaño N simultáneos (dos inputs + resultado
        de la repetición anterior + resultado nuevo en construcción). Bajado a N=5.000.000
        como techo del spike y liberando el resultado de cada repetición antes de construir
        el siguiente (antes se solapaban un instante).
      - `native_map()` (Punto 3) **no se tocó**: su loop invoca una clausura que captura
        `RuntimeEnv` mutable e intercambia el env del intérprete por llamada
        (`mem::replace`) — no es un loop `Send`/`Sync`-seguro para repartir en rayon sin
        rediseñar ese mecanismo primero (un `RuntimeEnv` por hilo, no uno compartido). Queda
        fuera de alcance de este punto, anotado explícitamente, no omitido en silencio.
      - Tests nuevos en `lib.rs`:
        `test_vector_elementwise_parallel_path_matches_sequential_reference` (N=60.000,
        ejercita la rama rayon), `test_vector_elementwise_below_threshold_still_correct`
        (N=100, rama secuencial sin boxing), `test_vector_elementwise_na_fallback_unaffected`
        (NA mezclado, confirma que sigue cayendo al loop original y preserva Kleene
        por-elemento).
      - Verificado de punta a punta con el CLI de release: suma Vector-Vector grande
        (1.000.000 elementos, camino paralelo), suma/`.* ` chicos (camino secuencial sin
        boxing), y suma con un NA mezclado (camino de respaldo, preserva `NA:NaN` en la
        posición correcta) — los tres dan el resultado correcto.
- [x] **Paralelismo automático/opt-in en verbos de DataFrame** (`.parallel()` antes de
      `.agg()`): investigado, no implementado, porque **ya no hacía falta implementar
      nada** — `summarize()`/`group_by()` (`io.rs::df_summarize`) ya arman un único query de
      `polars-lazy` (`LazyFrame::group_by_stable().agg([...])`), que **ya paraleliza
      internamente** la agregación entre grupos vía el pool propio de `polars-core`/
      `polars-lazy` (el mismo pool basado en rayon que usan las reducciones de `Vector` —
      `mean_reduce`/`sum_reduce`/etc., ver Punto 2 de Fase 3). No hay una superficie de
      sintaxis `.parallel()` explícita porque no hay nada que activar: es el comportamiento
      por defecto del motor de queries, no un modo opt-in. Dejado como verificado y
      documentado, no como trabajo pendiente.
- [x] **Bootstrap paralelo sin duplicar la muestra base (memoria compartida, Caso 3.3):**
      `bootstrap_mean(v, n_replicas)` — nueva función libre en `env.rs`, devuelve un
      `Vector[f64]` de `n_replicas` medias bootstrap, cada una calculada remuestreando `v`
      con reemplazo. `benchmarks/suites/03-statistical-modeling.md` no traía ejemplo de
      código para este caso (solo para el Gibbs sampler del Caso 3.1, con sintaxis
      aspiracional de todos modos), así que se diseñó la superficie desde cero.
      - **A propósito, solo la media** — no un estimador arbitrario vía clausura (como
        `map()`): una clausura captura `RuntimeEnv` mutable y lo intercambia por llamada,
        no es `Send`/`Sync`-segura para repartir en rayon sin rediseñar ese mecanismo
        (mismo motivo por el que `map()` no se paralelizó en el punto (a) de esta fase). La
        media, en cambio, no necesita más que un acumulador `f64` por réplica, así que
        paraleliza limpio tal cual.
      - **"Memoria compartida sin duplicar la muestra base" sale gratis de algo que ya
        existía**, no hubo que construir nada nuevo para eso: `base: &[f64]` se obtiene
        una sola vez vía `VectorData::as_f64_view()` (Punto 2, Fase 3) y se comparte por
        referencia entre todas las tareas de rayon (`&[f64]` es `Sync`) — cero copias del
        array de la muestra sin importar cuántas réplicas corran. Cada réplica tampoco
        materializa su propio subconjunto remuestreado (sería O(n) por réplica) — es un
        `f64` suelto acumulando `n` sorteos con `rng.random_range(0..n)`
        (`rand::RngExt::random_range`, ya importado desde `random_uniform`). Memoria total:
        O(n + n_replicas), no O(n × n_replicas).
      - **Hallazgo al investigar (nada reusable directo):** `sample_n`/`sample_frac`
        (`io.rs::df_sample_n`) samplean filas de un `DataFrame` **sin reemplazo**, con su
        propio xorshift sembrado por reloj del sistema — un RNG completamente distinto al
        `rand` crate que ya usa `random_uniform` (dos generadores conviviendo en el mismo
        runtime, dato curioso más que un problema en sí). Y `take_rows()` (usado por
        `sample_n`) hace un *gather* real que copia los valores seleccionados — exactamente
        lo que este punto pide evitar, así que no servía como base para bootstrap.
      - **Medido (`examples/spike_bootstrap_mean_latency.rs`, N=100.000, 2.000 réplicas —
        no las 20.000 réplicas × N=100.000 completas del enunciado, que son 2×10⁹ sorteos
        en total y tardan minutos; 2.000 ya deja ver el efecto con margen de sobra):
        secuencial 1.435s → paralelo 331ms, **~4.34x** con 8 hilos disponibles. Sin riesgo
        de OOM como los dos spikes anteriores de esta fase — por diseño, no por suerte: acá
        no hay ningún buffer O(n) por réplica que retener.
      - Tests nuevos en `lib.rs`:
        `test_bootstrap_mean_produces_requested_number_of_replicas` (cada réplica cae en
        el rango de la muestra base, la gran media de 500 réplicas se acerca a la media
        real), `test_bootstrap_mean_propagates_na`, `test_bootstrap_mean_rejects_non_vector_first_argument`,
        `test_bootstrap_mean_rejects_empty_vector`.
      - Verificado de punta a punta con el CLI de release: `bootstrap_mean` sobre una
        muestra real (2.000 réplicas centradas correctamente en ~0.5 para
        `random_uniform`) y sobre una muestra con NA (colapsa a `NA:NaN`, no a un Vector).
      - **Qué queda afuera, a propósito:** otros estimadores (mediana, percentiles, uno
        arbitrario vía clausura); `quantile()`/`percentile()` para calcular el intervalo de
        confianza real que pide el enunciado completo del caso (esta pasada entrega la
        distribución de réplicas, no el intervalo en sí — no existe `quantile()` en el
        runtime todavía); bootstrap sobre vectores con NA (colapsa a `NA`, no excluye las
        posiciones NA del pool de remuestreo); reproducibilidad bit-a-bit (mismo criterio
        que `random_uniform`, es trabajo de Fase 5).

Con esto, Fase 4 queda cerrada por completo (los tres puntos: iteradores work-stealing
sobre `Vector`, paralelismo de DataFrame verificado, y bootstrap paralelo). **"Cerrada"
acá es dentro del alcance que definió cada plan de punto — no significa que no haya
quedado nada boxeado/secuencial en el lenguaje.** Al preguntarse eso directamente, la
respuesta fue no: quedó una lista concreta de funciones de `Vector` (documentada desde el
Punto 2 de Fase 3, commit `36ab513`) que nunca se migraron ni paralelizaron. El bloque de
abajo cierra esa lista.

### Migrar el resto de funciones de `Vector` al camino rápido/paralelo

Cierra (con matices, ver abajo) la lista pendiente desde Fase 3 Punto 2: `map_numeric_fn`/
`map_string_fn`, `cumulative`, `sort_vector`, `lag`/`lead`/`rank`/`if_else`/`between`,
`UnaryNeg` sobre `Vector`, `n_distinct`.

- [x] **Grupo A — camino rápido + paralelo (`VectorData::as_f64_view()`, reusa
      `PARALLEL_THRESHOLD = 50.000` de Fase 4 punto (a); una función más cara por elemento
      que una suma solo adelanta el cruce real, así que 50.000 sigue siendo conservador
      acá, no incorrecto):**
    - `map_numeric_fn` — cubre de una sola migración `log`/`log2`/`log10`/`exp`/`sqrt`/
      `abs`/`floor`/`ceil`/`round`/`sin`/`cos`/`pow`/`clamp` (los últimos tres ya
      currying-eaban su argumento extra hacia `map_numeric_fn`). Nuevo constructor
      **`VectorData::from_f64_opt(Vec<Option<f64>>)`** en la salida (no `from_f64`): un
      resultado puede dar `NaN` (`pow(-8.0, 0.5)`) aunque ningún elemento de entrada fuera
      NA, así que la salida necesita mezclar `Some`/`None` sin pasar por `Vec<Value>`.
    - `UnaryNeg` sobre `Vector` (`eval.rs`) — camino rápido **solo si el `Column` ya es
      `Float64`**. Encontrado *antes* de escribir código: a diferencia de
      `map_numeric_fn`/`cumulative` (que ya daban `F64` siempre, sin importar el tipo de
      entrada), `UnaryNeg` **preserva el tipo original** (`-[1,2,3]` da `I64`, no `F64`).
      Reconstruir desde `as_f64_view()` sin este chequeo habría coercionado todo vector
      `I64` a `F64` en silencio. Vectores `Int64` (o con NA) siguen por el loop boxeado
      existente, sin cambios — verificado con `test_unary_neg_preserves_i64_dtype`.
    - `between()` — nuevo constructor **`VectorData::from_bool(Vec<bool>)`** para la
      salida (siempre `Bool`, sin problema de tipo).
    - `cumulative` (`cumsum`/`cumprod`/`cummax`/`cummin`) — sin NA en toda la entrada:
      scan secuencial sobre `&[f64]`, salida vía `from_f64` (ya daba siempre `F64`, sin
      cambio de comportamiento). **No se paralelizó**: un cumsum tiene dependencia
      secuencial estricta entre elementos — paralelizarlo de verdad pide un algoritmo de
      *parallel scan* dedicado, no `rayon::par_iter()`. Con NA presente: exactamente el
      mismo loop boxeado de siempre (el comportamiento "un NA envenena todo lo que sigue"
      no se tocó).
- [x] **Grupo B — camino rápido vía permutación + gather nativo (preserva dtype sin
      reconstruir desde floats):**
    - `sort_vector` (`sort_asc`/`sort_desc`) — sin NA y dtype numérico: `as_f64_view()`
      **solo para las claves de orden**, ordena un array de índices, y reordena la
      `Column` **original** vía `.take(&idx_ca)` (mismo mecanismo que
      `io.rs::take_rows` usa para `sample_n`) — preserva el dtype (`sort_asc([3,1,2])`
      sigue dando `I64`, no `F64`; verificado con `test_sort_asc_preserves_i64_dtype`).
      Nuevo constructor **`VectorData::from_column_no_na(Column)`**, wrapper directo sin
      pasar por `Vec<f64>`/`Vec<Value>`. **Umbral paralelo medido aparte**
      (`examples/spike_sort_threshold_latency.rs`): un sort es O(n log n) con más trabajo
      por elemento que una suma, y el cruce real resultó mucho más bajo que el de
      Fase 4(a) — `par_sort_unstable_by` todavía pierde a N=4.000 (~0.76x) pero ya gana a
      N=5.000 (~1.06x), subiendo a ~1.46x (10K), ~2.48x (50K), ~3.29x (1M). Nueva
      constante `PARALLEL_THRESHOLD_SORT = 5_000` (`eval.rs`), separada de
      `PARALLEL_THRESHOLD` a propósito, no reusada a ciegas.
    - `rank` — la salida ya era siempre `F64` (sin problema de tipo); solo se aceleró la
      *lectura* (`as_f64_view()` + `total_cmp` en vez de `compare_values` genérico sobre
      `Vec<Value>` materializado).
- [x] **Grupo C — mejora parcial (lectura rápida, escritura sigue boxeada, a
      propósito):** `map_string_fn` (`str_upper`/`str_lower`/`str_trim`/`str_len`/
      `str_contains`/`str_starts`/`str_ends`/`str_replace`/`str_pad`) — lee directo desde
      `column.str()`'s `ChunkedArray` (`Option<&str>` por celda) en vez de recursar sobre
      el `Vec<Value>` materializado, pero la salida sigue por `Vec<Value>`/`from_values`
      sin cambio: `f: impl Fn(&str) -> Value` puede devolver `String`, `Bool`
      (`str_contains`) o `I64` (`str_len`) — no hay un `NumericView` equivalente para
      strings, y construir esa infraestructura completa no está justificado por lo que
      este proyecto mide (benchmarks numéricos, no de texto). Ganancia real pero menor
      que el Grupo A.
- [x] **Grupo D — delegación directa a polars (fix de correctness + performance a la
      vez):** `n_distinct()` → `Column::n_unique()` en vez de `format!("{:?}", it)` +
      `HashSet<String>`. La distinción de razones de NA que motivó no migrarlo en su
      momento (Fase 3, Punto 2) resultó ser, al revisarla de nuevo, un bug real y no una
      semántica a preservar — corregido, verificado con
      `test_n_distinct_counts_different_na_reasons_as_one_missing_value`.
**Puntos completados de cierre de Fase 4 (zero-boxing y paralelismo):**
- [x] **`lag`/`lead` — zero-boxing nativo con `Column::shift`:** implementado
      directamente sobre el backend Arrow de `polars_core` (`Column::shift(periods)`).
      Preserva cualquier dtype nativo sin materializar `Vec<Value>` ni clonar elementos.
      El side-channel de razones de NA se preserva mediante `NaReasonTable::shift(periods, len)`,
      desplazando las razones a sus nuevas posiciones y descartando las desbordadas.
- [x] **`if_else` — vectorizado con camino rápido numérico y fallback polimórfico:**
      Condición `Vector[Bool]` sin NAs y ramas numéricas (vectores o escalares) despachan a
      un loop directo sin boxing; para `len >= 50_000` (`PARALLEL_THRESHOLD`) se evalúa en
      paralelo con `rayon`. El camino polimórfico general (cadenas, Kleene NA, objetos) lee
      vía `VectorData::value_at(i)` y `broadcast_get` sin materializar todo el vector vía `Deref`.
- Tests nuevos en `lib.rs` (11): `test_map_numeric_fn_fast_path_matches_boxed_reference`,
  `test_pow_fast_path_produces_na_on_nan_without_input_na`,
  `test_unary_neg_preserves_i64_dtype`, `test_between_fast_path`,
  `test_cumsum_fast_path_matches_boxed_reference`,
  `test_cumsum_na_poisons_rest_unaffected`, `test_sort_asc_preserves_i64_dtype`,
  `test_sort_asc_fast_path_matches_boxed_reference`,
  `test_rank_fast_path_matches_boxed_reference`,
  `test_str_upper_fast_read_matches_reference`,
  `test_n_distinct_counts_different_na_reasons_as_one_missing_value`. Las 82 pruebas de
  `ghl-runtime` y el workspace completo pasan sin regresiones. Verificado de punta a
  punta con `ghl run` (incluyendo confirmar visualmente `Vector[i64]` preservado en
  `-iv` y `sort_asc`/`sort_desc` sobre un vector entero).

## Fase 5 — RNG + distribuciones + arenas (`benchmarks/suites/03`, RFC 03 §2.2)
Requisito para todo el modelado estadístico de la Suite 03.

**Orden revisado y confirmado explícitamente** (misma discusión que Fase 3 Track 2):
"más estructural primero" acá tenía dos lecturas posibles — dependencia interna de la
fase (PRNG antes que Distribuciones, que necesitan un generador para samplear) o
profundidad arquitectónica (CoW/ARC afecta todo el intérprete, no solo lo estadístico).
Se eligió la primera: PRNG → Distribuciones → Arenas → CoW, dejando el cambio más grande
e invasivo (CoW) al final, informado por dónde el profiling real de Fase 6 (corriendo
Gibbs/bootstrap/GLM/EM de verdad) muestre que duele — no apostado a ciegas antes de tener
evidencia medida.

**Investigación previa (antes de tocar código):** `rand_xoshiro`, `rand_distr`, `statrs`
y `bumpalo` ya estaban declarados en `ghl-runtime/Cargo.toml` desde Fase 0 — ninguno se
usa todavía en el código (`grep` no encontró referencias). Y la sintaxis que el propio
doc de Suite 03 usa para esto (`PRNG::seed(seed)`, `Normal::new(...).sample(&mut rng)`)
**no existe en GHL**: el parser no tiene soporte de sintaxis `Tipo::método(...)` (path)
en absoluto — mismo tipo de mismatch que ya se resolvió para `Vector::random_uniform`/
`a.dot(&b)` en Fase 3, Punto 4. La resolución consistente es funciones libres, no
inventar sintaxis de path nueva; se decide en el plan de cada punto.

- [x] **PRNG reproducible bit-a-bit — hecho, con una decisión de diseño real.** El
      enunciado aspiracional (`PRNG::seed(seed)`, un objeto que se muta en cada sorteo)
      no es expresable en GHL hoy: no hay reasignación de variables (`is_mut` existe en
      `StmtKind::Let` pero ningún `ExprKind::Assign` lo usa — campo vestigial) ni
      tuplas/destructuring para un estilo `let (val, rng2) = draw(rng)`. En vez de
      inventar cualquiera de las dos cosas solo para esto, **la semilla es un argumento
      explícito de cada función nativa que genera aleatoriedad** — sin objeto persistente,
      sin mutación, sin alias.
    - `random_uniform(n, seed)` — segundo argumento opcional (mismo patrón de aridad
      opcional que `round(v, digits)`). Mismo `seed` y `n` → mismo `Vector` byte a byte,
      en cualquier corrida. Sin `seed`: comportamiento default sin cambios.
    - `bootstrap_mean(v, n_replicas, seed)` — tercer argumento opcional. Como las
      réplicas corren en paralelo vía rayon, un simple swap a un generador sembrado
      compartido **no alcanza** para reproducibilidad real: el resultado podría depender
      de qué réplica cae en qué hilo. Arreglado asignando a cada réplica su propio
      generador **antes** de lanzar el loop paralelo, vía
      `Xoshiro256PlusPlus::jump()` (avanza el estado el equivalente a 2^128 sorteos,
      produce streams estadísticamente independientes sin overlap — el mecanismo
      estándar para paralelizar esta familia de generadores; combinar `seed + índice` a
      mano habría arriesgado correlacionar streams vecinos). Verificado con
      `test_bootstrap_mean_seeded_independent_of_thread_count`: forzar el pool de rayon
      a 1 hilo (`ThreadPoolBuilder::num_threads(1)`) da el mismo resultado que el pool
      default — la prueba real de que no depende del scheduling.
    - `rand_xoshiro`/`rand_distr`/`statrs`/`bumpalo` ya estaban en
      `ghl-runtime/Cargo.toml` desde Fase 0, sin usar — este punto usa `rand_xoshiro`
      por primera vez.
    - **Queda afuera, a propósito:** un objeto `PRNG` persistente con estado (se retoma
      si el Punto 2 de verdad necesita componer sorteos heterogéneos desde una sola
      semilla en un mismo script); verificación de reproducibilidad **entre
      plataformas** (Linux/macOS/Windows, x86/ARM) — no se puede probar en este sandbox
      (solo Linux x86_64), documentado como esperado por diseño (Xoshiro256++ es
      aritmética entera pura) pero no verificado cruzado.
    - Tests nuevos en `lib.rs`: `test_random_uniform_seeded_is_reproducible`,
      `test_random_uniform_without_seed_still_unseeded`,
      `test_bootstrap_mean_seeded_is_reproducible`,
      `test_bootstrap_mean_seeded_independent_of_thread_count`. Las 86 pruebas de
      `ghl-runtime` y el workspace completo pasan sin regresiones. Verificado de punta a
      punta con `ghl run`.
- [x] **Biblioteca de distribuciones (`Normal`, `Gamma`) — hecho, con una trampa de
      parametrización real evitada.** `random_normal(n, mean, sd)`/`random_gamma(n,
      shape, rate)`, mismo patrón que `random_uniform`: último argumento opcional es la
      semilla (`random_normal(n, mean, sd, seed)`), salida vía `VectorData::from_f64`.
    - **`statrs` elegido sobre `rand_distr`** (ambos estaban en `Cargo.toml` desde
      Fase 0, sin usar) — no por descartar el segundo, sino porque con el mismo esfuerzo
      de implementación `statrs` deja pdf/cdf usables gratis (ver abajo) y, más
      importante: **`statrs::distribution::Gamma::new` toma `(shape, rate)`, mientras
      `rand_distr::Gamma::new` toma `(shape, scale)`** (`scale = 1/rate`) — la misma
      familia de distribución, dos convenciones distintas para el segundo parámetro. El
      Gibbs sampler aspiracional del propio doc de Suite 03 actualiza una precisión con
      `Gamma::new(1.0 + n/2.0, 1.0 + ssq/2.0)` — la fórmula de actualización conjugada
      Normal-Gamma bayesiana estándar, que es shape-**rate**. `rand_distr` habría dado
      resultados estadísticamente distintos con el mismo número pasado como segundo
      argumento. El parámetro GHL se llama `rate` explícitamente (no `scale`) para que
      esto no quede ambiguo — blindado con `test_random_gamma_matches_expected_mean`
      (la media de `Gamma(shape, rate)` es `shape/rate`; con shape-scale hubiera dado
      sistemáticamente otro valor).
    - **`normal_pdf(x, mean, sd)`/`normal_cdf(x, mean, sd)`/`gamma_pdf(x, shape,
      rate)`/`gamma_cdf(x, shape, rate)`** — agregados tras discusión con el usuario:
      no eran trabajo nuevo real, `statrs::distribution::Normal`/`Gamma` ya
      implementan `Continuous`/`ContinuousCDF` además de `Distribution<f64>`, así que
      exponerlos es reusar el mismo constructor con otra llamada.
    - **Paralelismo agregado** tras la misma discusión: la razón original para
      omitirlo ("`random_uniform` tampoco lo tiene") no era una decisión de diseño,
      era que `random_uniform` se construyó en Fase 3 antes de que
      `PARALLEL_THRESHOLD` (Fase 4) existiera — mantenerlo así habría sido
      inconsistente, no prudente. Con NA... no aplica acá (no hay NA de entrada, solo
      parámetros escalares), así que el único eje es tamaño: por debajo de
      `PARALLEL_THRESHOLD` (50.000), secuencial; por encima, paralelo vía
      `rayon::par_chunks_mut`. **Diseño distinto al de `bootstrap_mean`, a propósito**:
      ahí un `.jump()` por réplica era barato porque cada réplica ya hacía O(largo de
      la base) de trabajo interno; acá cada elemento es un solo sorteo escalar, así que
      un `.jump()` por elemento habría hecho el *setup* secuencial O(n) — con el mismo
      orden que el propio trabajo paralelo, capando la ganancia real a medida que n
      crece (ley de Amdahl, no hipotético). Se resolvió con **chunks de 1024
      elementos**: un `.jump()` por chunk (no por elemento), cada chunk llena su propio
      rango con un solo generador — mismo mecanismo de reproducibilidad
      (`test_random_normal_seeded_independent_of_thread_count`, igual criterio que
      Punto 1), pero con setup secuencial O(n/1024) en vez de O(n).
    - Tests nuevos en `lib.rs` (10):
      `test_random_normal_seeded_is_reproducible`,
      `test_random_gamma_seeded_is_reproducible`,
      `test_random_normal_matches_expected_mean_and_sd`,
      `test_random_gamma_matches_expected_mean`,
      `test_random_normal_rejects_non_positive_sd`,
      `test_random_gamma_rejects_non_positive_shape`,
      `test_normal_pdf_cdf_match_known_values`,
      `test_gamma_pdf_cdf_match_known_values`,
      `test_random_normal_parallel_path_matches_sequential_reference`,
      `test_random_normal_seeded_independent_of_thread_count`. Las 96 pruebas de
      `ghl-runtime` y el workspace completo pasan sin regresiones. Verificado de punta
      a punta con `ghl run` (incluyendo el camino paralelo a N=100.000 y el error claro
      de `sd <= 0`).
    - **Queda afuera, a propósito:** otras distribuciones más allá de `Normal`/`Gamma`
      (sin consumidor concreto todavía — se agregan si Fase 6 las pide de verdad); un
      objeto `PRNG`/distribución persistente con estado — **no es una decisión de
      alcance, es la misma pared técnica que ya bloquea Arenas** (Punto 3 de esta
      fase): GHL no tiene reasignación de variables ni tuplas/destructuring hoy,
      confirmado leyendo el parser (ver el ítem de Fase 7 sobre iteración, con el que
      comparte la misma causa raíz).
- [x] **Arenas regionales (`bumpalo`) para bucles iterativos (MCMC/bootstrap) con
      cero asignaciones de heap por iteración — sintaxis `arena::scope(\a -> { ... })` — hecho.**
      - Módulo `std::arena` registrado en `ghl-types` y `ghl-runtime` (`use std::arena::{scope, alloc_vector, alloc_matrix, reset, allocated_bytes}`).
      - `Value::Arena(Arc<Mutex<ArenaState>>)` respaldado por `bumpalo::Bump` 3.20.3.
      - Primitivas:
        - `scope(\a -> ...)`: ejecuta el closure inyectando el handle de arena y garantiza reseteo/liberación al terminar.
        - `alloc_vector(a, len, [default])`: bump allocation contigua para vectores de punto flotante.
        - `alloc_matrix(a, rows, cols, [default])`: bump allocation contigua para matrices.
        - `reset(a)`: reclamación de memoria instantánea $O(1)$ en fronteras de iteración (`bump.reset()`).
        - `allocated_bytes(a)`: diagnóstico del consumo activo en bytes dentro del ciclo.
      - **Scope Pool (Zero-allocation frames) en `RuntimeEnv`:** Se introdujo `scope_pool: Vec<HashMap<String, Value>>`
        para reciclar las tablas de símbolos de scopes en bucles `while`, `Block` y llamadas de cola TCO,
        eliminando las asignaciones continuas en el heap. Además, la herencia de variables globales en el
        trampolín de TCO se extrajo fuera del bucle de iteración.
- [x] **Modelo de memoria ARC + Copy-on-Write real (RFC 03 §2.1) — hecho.**
      - `Value::Matrix` migrado a `Arc<Vec<f64>>`: clonar matrices al pasarlas como argumento o retornarlas
        de funciones es una operación atómica $O(1)$ sin copia del buffer.
      - Mutación in-place con `Arc::make_mut`: `native_set` y `native_set_row` detectan cuando la referencia
        es única (`strong_count == 1`, típico en bucles iterativos MCMC/Gibbs) y modifican el búfer
        directamente en el sitio sin realizar ninguna clonación ni asignación de heap. Si la matriz está
        compartida (`strong_count > 1`), se clona transparentemente solo la matriz afectada preservando el original.
      - Verificado con test unitario de punteros `test_matrix_cow_inplace_when_unique_and_clones_when_shared` (`Arc::as_ptr`).
      - Rendimiento Gibbs medido (`spike_gibbs_mcmc_latency`): latencia reducida a **46.47 µs/iteración** (**21.519 iter/s**).

## Fase 6 — Modelado estadístico avanzado (`benchmarks/suites/03`)
Depende de Fase 5 (RNG/distribuciones/arenas) y de Fase 3 (solver matricial). **El
bloqueo de iteración de Fase 7 que preocupaba acá ya se resolvió** (TCO para recursión
en posición de cola) — con una condición de estilo explícita, no automática: los tres
casos de abajo necesitan escribirse **sin `return`**, apoyándose en la expresión final
implícita de `if`/`Block` (`return expr;` no gana TCO, ver el ítem de Fase 7). Escritos
así, la profundidad de iteración deja de ser un bloqueo — el costo real pasa a ser de
tiempo de ejecución, no de crash: ~56µs por iteración medido a profundidad extrema
(2.000.000), lo que da estimados perfectamente razonables a las escalas que estos casos
piden (ver detalle en Fase 7).

- [x] **Gibbs sampler jerárquico (Caso 3.1, Suite 03) — hecho, corriendo de punta a punta en GHL.**
      El algoritmo se ejecuta como código GHL puro directamente sobre el intérprete de
      `ghl-runtime`, sin depender de librerías en C++ ni requerir extensiones DSL ad-hoc.
      - **Primitivas generales de colecciones y matrices agregadas:**
        - `zeros(n)` (vector de ceros) y `zeros(rows, cols)` (matriz de ceros).
        - `len(x)` polimórfico (`Vector`, `String`, `DataFrame`, `Matrix`).
        - `get(collection, index)` (indexación por entero, gather vectorial `get(mu, groups)`,
          acceso por índice en strings, y `get(matrix, r, c)`).
        - `set(collection, index, val)` (actualización in-place/funcional de vector y matriz `set(m, r, c, val)`).
        - `get_row(matrix, r)`, `set_row(matrix, r, vector)` y `get_col(matrix, c)`.
        - `filter(vector, mask_booleana)` soportado nativamente en `native_filter`, usando
          filtrado Arrow/Polars zero-copy `Column::filter`, preservando compatibilidad
          completa con `filter(dataframe, ...)`.
      - **Convergencia estadística verificada:** probada en `test_gibbs_sampler_runs_end_to_end_and_recovers_means`
        (N=600, 3 grupos) y en el spike de benchmark (N=3.000, 3 grupos con medias reales
        `[3.0, 8.0, -4.0]`, $\sigma=0.8$). Las medias posteriores convergen con error < 0.02.
      - **Rendimiento medido (`spike_gibbs_mcmc_latency.rs`, N=3.000, 500 iteraciones):**
        - Tiempo total: **24.22 ms**.
        - Latencia por iteración: **~48.4 µs**.
        - Rendimiento: **~20.640 iteraciones/segundo**.
      - Documentación actualizada en `benchmarks/suites/03-statistical-modeling.md` con el
        código ejecutable real de `gibbs.gh` y las métricas obtenidas.
- [x] **IRLS para GLM / regresión logística sobre N=1M, P=40 (Caso 3.2) — hecho.** No
      estuvo bloqueado por recursión (pocas iteraciones hasta converger), ni lo estuvo
      nunca.
      - **Hallazgo grande al investigar antes de diseñar (pedido explícito): ya existía
        un motor de regresión completo, NEKO** (`crates/ghl-runtime/src/neko.rs`, RFC en
        `docs/design/11-neko-statistical-modeling-framework.md`), con soporte de
        fórmulas `y ~ x1 + ...` de punta a punta. Reusado tal cual, sin cambios:
        `Blueprint::bake()` (construcción de matriz de diseño + trazabilidad de NA por
        fila), `MatrixOps::solve` (Fase 3, LU vía faer) para el sistema ponderado de cada
        iteración de IRLS, y `normal_cdf`/`erf` (subidos a `pub(crate)` para reusarse
        desde el nuevo `glm.rs`) para los z-estadísticos.
      - **Bug real encontrado de paso en NEKO, no relacionado con IRLS — arreglado en la
        misma pasada, no dejado anotado para después.** El RFC especifica la fórmula
        sandwich real para HC0-HC3 (`V_HC = (X^TX)^{-1}(Σw_i·X_i^TX_i)(X^TX)^{-1}`,
        ponderada por residuo/leverage), pero `FittedModel::compute_vcov` no la
        implementaba así: HC0/HC2/HC3 daban exactamente lo mismo que Classical (solo
        HC1 aplicaba una corrección escalar de grados de libertad). Causa raíz:
        `FittedModel` nunca guardaba la matriz de diseño `X` cruda después de ajustar —
        sin `X` fila por fila no hay forma de calcular leverage ni el "meat" ponderado
        por observación. Fix: se agregó `x_data: Vec<f64>` a `FittedModel`, y se
        factorizó un helper compartido `sandwich_vcov(...)` que implementa la fórmula
        real (con leverage `h_ii` para HC2/HC3) — **reusado tal cual por
        `FittedGlm::compute_vcov`** (con `(X^TWX)^{-1}` como "bread" y `(y-μ)²` como
        peso, en vez del "bread"/peso de OLS) para que la vcov robusta de GLM entrara en
        la misma pasada en vez de quedar afuera arbitrariamente. Tests:
        `test_vcov_hc0_differs_from_classical_under_heteroskedasticity`,
        `test_vcov_hc2_hc3_differ_via_leverage`, `test_glm_vcov_hc0_differs_from_classical`.
      - **`FittedGlm` (nuevo `glm.rs`) deliberadamente NO es una reutilización de
        `FittedModel`:** los diagnósticos de OLS (`r_squared`, `f_stat`, t-stats con
        corrección de grados de libertad) no tienen un análogo correcto en regresión
        logística — forzarlos habría sido exactamente el "estado silencioso" que este
        proyecto evita en cada punto. `FittedGlm` reporta sus propios diagnósticos
        (deviance, pseudo-R² de McFadden, AIC/BIC, z-estadísticos de Wald,
        `iterations`), reusando de NEKO solo lo que es máquina compartida real.
      - IRLS: `β` inicial en 0, `MAX_ITER=25`/`TOL=1e-8` en `max|Δβ|`, sigmoid numéricamente
        estable (evita overflow de `exp()` para `η` muy negativo), `W=max(μ(1-μ), 1e-10)`
        (evita `X^TWX` singular cuando `μ` satura). Si no converge en 25 iteraciones →
        error explícito `S0205`, no un ajuste parcial devuelto en silencio — esto
        efectivamente disparó durante las pruebas sobre un dataset de juguete
        (cuasi-)separable, confirmando que el comportamiento es el diseñado, no un bug.
        Respuesta no-binaria → `S0204`. `n<=p` → reusa `S0201` de OLS.
      - **Ensamblado de `X^T W X`/`X^T W z` paralelizado con rayon (`fold`+`reduce` por
        encima de `PARALLEL_THRESHOLD=50.000`, umbral ya medido en Fase 4), factorizado
        como helper compartido `assemble_weighted_normal_equations` — y aplicado también
        a `fit_ols`**, que hasta ahora ensamblaba su normal-equations con un triple loop
        secuencial sin paralelizar (`neko.rs`, nunca hizo falta a las escalas usadas
        hasta ahora). Pedido explícito del usuario al revisar el plan: no dejar esto
        "para después" si la pieza ya se estaba construyendo al lado.
        Test de regresión: `test_fit_ols_parallel_assembly_matches_sequential_reference`.
      - **`Value::GlmFit(Box<FittedGlm>)`** nuevo, y las 8 nativas de NEKO
        (`summary`/`tidy`/`glance`/`augment`/`predict`/`residuals`/`coef`/`vcov`) ganan
        una rama para despachar sobre él — mismo verbo de GHL funciona sobre un ajuste
        OLS o logístico sin que el script sepa cuál es cuál (confirmado con
        `test_glm_generic_verbs_dispatch_on_either_model_type` y de punta a punta con el
        CLI de release).
      - **Medido (`examples/spike_irls_latency.rs`, N=1.000.000, P=40 predictores
        continuos, datos generados con el `random_normal` sembrado real de GHL vía el
        intérprete — no un CSV intermedio):**
        - Ajuste completo: **8.7s**, converge en **5 iteraciones**, coeficientes
          recuperados a `max|β_est-β_true| = 0.0044` del valor verdadero (estabilidad
          numérica confirmada a escala completa, no solo en el toy dataset de los tests).
        - Memoria pico real medida con `/usr/bin/time -v`: **~6.9GB** a esta escala — casi
          toda de `HashMap<String, Vec<Value>>` que `Blueprint::bake` consume
          (`size_of::<Value>() == 160` bytes/celda × 41 columnas × 1M filas ≈ 6.5GB). La
          primera corrida a N=1M sin las optimizaciones de abajo terminó en un **OOM-kill
          real** (`exit 137`) en este sandbox de 15GB (con ~9GB disponibles al momento de
          medir) — se arregló liberando `interp`/`predictors`/`data` tan pronto como cada
          uno deja de hacer falta, no reduciendo N. Esto también documenta un límite real
          de escalabilidad de la representación de datos de NEKO (no introducido por este
          trabajo, preexistente para `fit_ols` también) — no se rediseña acá, es un punto
          aparte de mayor alcance.
        - Ensamblado paralelo vs. secuencial (pesos reales del ajuste convergido, best-of-5
          alternando orden): **paralelo salió ~0.72x — más lento, no más rápido** — en esta
          corrida de punta a punta, reproducible entre corridas. Un microbenchmark aislado
          del mismo algoritmo exacto (mismo N/p, datos sintéticos, proceso limpio sin la
          huella de memoria del resto del pipeline) sí midió una mejora real (~1.2x a
          p=41, con retornos decrecientes al subir p: 1.47x/p=10, 1.07x/p=100, 1.01x/p=300
          — consistente con estar acotado por ancho de banda de memoria, no por cómputo).
          La brecha entre ambas mediciones no se explica por NUMA (un solo nodo en este
          sandbox) ni por retener `data` vivo (se probó soltarlo antes de medir, sin
          cambio) — lectura más plausible en su momento: la churn de asignación del
          pipeline completo a N=1M dejaba el heap en un estado que penaliza más al camino
          paralelo. **Actualización, tras el fix de representación de datos de NEKO más
          abajo:** esa lectura quedó descartada por el dato, no solo sin confirmar — con
          el pico de memoria de este mismo benchmark bajado de ~6.9GB a ~736MB (ver más
          abajo), el mismo ~0.6-0.65x persiste igual de estable. La presión de memoria no
          era la causa. **Sigue sin investigarse más a fondo** (fuera del alcance de
          ambos casos) — documentado tal cual salió, no promediado ni descartado, para que
          quien ajuste `PARALLEL_THRESHOLD` más adelante parta de un número real y no de
          un supuesto (ni del supuesto original, ni de esta corrección).
      - Tests nuevos en `lib.rs`: `test_fit_logistic_recovers_known_coefficients` (N=8.000,
        β verdadero conocido, tolerancia estadística), `test_fit_logistic_rejects_non_binary_response`,
        `test_fit_logistic_rejects_insufficient_df`, `test_fit_logistic_na_disposition_matches_ols_pattern`,
        `test_glm_generic_verbs_dispatch_on_either_model_type`, `test_glm_vcov_hc0_differs_from_classical`,
        más los tres tests del fix de NEKO listados arriba.
      - Verificado de punta a punta con el CLI de release: `fit_logistic(y ~ x1 + x2, df)`
        sobre un dataset chico real, con `summary()`/`tidy()`/`glance()`/`augment()`/
        `predict()` sobre el resultado, y el caso de respuesta no-binaria (`S0204` limpio,
        no un crash).
      - [x] **Comparación elementwise sobre `Vector` — implementada y probada.**
        Los operadores binarios de comparación (`==`, `!=`, `<`, `<=`, `>`, `>=`) en
        `eval.rs` y `checker.rs` ahora soportan `Vector` contra `Vector` (longitudes
        coincidentes) y `Vector` contra `escalar` (broadcasting bidireccional), produciendo
        un `Vector` de booleanos tipado (`VectorData` con `BooleanChunked` de Polars/Arrow).
        Las comparaciones estrictamente escalares no sufrieron ninguna regresión
        (`test_scalar_comparisons_parity_unchanged`). Verificado con
        `test_vector_comparisons_elementwise`.
      - **Qué queda afuera, a propósito (decisiones de diseño/alcance, no piezas a medio
        construir):** otras familias GLM (Poisson, binomial de conteos) — cada una tiene
        su propia función de varianza/link/deviance, es una feature nueva que nadie pidió
        todavía, se agrega si Fase 6 la necesita de verdad; y expresar el loop de IRLS en
        GHL puro con `while` — a diferencia del Gibbs sampler del Caso 3.1 (que pide
        explícitamente "capacidad de escribir el algoritmo directamente"), el enunciado
        de este caso solo evalúa "eficiencia del solver + estabilidad + sintaxis de
        fórmulas", y el patrón ya establecido para esto en todo el proyecto (`fit()`/OLS)
        es una función nativa de una sola llamada, no un script GHL.
- [x] **Optimizar la representación de datos de NEKO (`Blueprint::bake`) — hecho.** El
      pico de memoria de ~6.9GB medido arriba para el Caso 3.2 no era del algoritmo de
      IRLS: `bake()` recibía los datos como `HashMap<String, Vec<Value>>`, un `Value`
      boxeado (`size_of::<Value>() == 160` bytes) por celda — a 41 columnas×1M filas,
      ~6.5GB solo por esta representación, cuando el dato real (`f64`) pesa ~328MB.
      Resuelto ahora en vez de esperar a que otro caso de Fase 6 (Gibbs, EM) se topara
      con el mismo techo — decidido explícitamente con el usuario como infraestructura
      que toca a todo lo que sigue, no solo a IRLS.
      - **El material rápido ya existía en el proyecto**, nadie había conectado NEKO a
        él: `Value::DataFrame` ya está respaldado por `polars_core::DataFrame` (columnas
        `Float64Chunked` contiguas, tipadas) desde su propia migración; el único camino
        real a `bake()` (`native_fit_ols`/`native_fit_logistic`, y `predict()`/
        `augment()` de ambos modelos — 6 call sites en total, confirmado por grep) pasaba
        por `polars_bridge::dataframe_to_columns_and_data`, un shim de compatibilidad
        **documentado como tal en su propio comentario** ("predates the polars
        migration") que boxeaba cada celda. Este proyecto ya había resuelto el mismo
        problema exacto para `Value::Vector` en Fase 3/4 (`VectorData::as_f64_view()`) —
        el fix acá es aplicar ese mismo patrón a columnas de `DataFrame`, no inventar uno
        nuevo.
      - `VectorData::as_f64_view`'s lógica se extrajo a una función libre reusable,
        `vector_data::column_as_f64_view(&Column)`, para no depender de envolver cada
        columna en un `VectorData` (que además ata el `NaReasonTable` a un nombre de
        columna fijo, incompatible con columnas reales con nombre propio).
      - `Blueprint::bake` pasa a leer `&DataFrame`/`&NaReasonTable` directo: **camino
        rápido** (caso común, ninguna columna involucrada tiene nulos —
        `column.null_count() == 0`, O(1)) usa `column_as_f64_view` sin boxear nada, cero
        branching de disposición por fila; **camino de reserva** (alguna columna sí tiene
        nulos) recorre fila por fila leyendo `AnyValue` (vive en el stack) en vez de un
        `Value` boxeado, con `na_reasons.get(col, fila)` directo para la razón — misma
        semántica exacta de antes, sin el intermedio boxeado. El parámetro `_columns`
        de la firma vieja ya estaba muerto (nunca usado en el cuerpo) — desapareció sin
        reemplazo, no solo sin uso.
      - `augment()` (`FittedModel`/`FittedGlm`) tenía el mismo problema por una razón
        distinta: boxeaba el DataFrame **completo** (las P columnas originales) solo para
        copiarlas sin cambios a un DataFrame nuevo, cuando el único trabajo real es
        agregar 4 columnas derivadas de tamaño N. Arreglado con el mismo patrón que ya
        usa `mutate()` (`io.rs`): `frame.clone()` (barato, buffers compartidos por Arc) +
        `DataFrame::with_column()` por columna nueva — el costo pasa de O(n·p) a O(n),
        independiente de P. `dataframe_to_columns_and_data`, sin llamadores tras esto, se
        eliminó junto con el comentario de módulo desactualizado que la rodeaba.
      - **Medido de nuevo, mismo `spike_irls_latency.rs`, misma escala (N=1.000.000,
        P=40), antes/después:**
        - Memoria pico: **~6.9GB → ~736MB** (`/usr/bin/time -v`), ~9.4x menos — coincide
          con lo esperado (~328MB del DataFrame + ~328MB de `x_data` + buffers chicos).
        - Ajuste completo: **8.7s → 4.6-4.8s** (~1.9x más rápido) — ya no hay que
          des-boxear 41M celdas antes de ajustar.
        - **Hallazgo al re-medir, corrige una lectura anterior:** el benchmark en sí
          también boxeaba sin necesidad — su propio helper `vector_f64` (extraía
          `x1..x40` del intérprete tras generarlos con `random_normal`) leía vía el
          `Deref` de `VectorData` a `Vec<Value>`, materializando 160 bytes/celda para
          las 40 columnas *antes* de que el fix de `bake()` siquiera entrara en juego —
          arreglado para usar `as_f64_view()` también. Y con la memoria ya baja, el
          hallazgo del Caso 3.2 de que el ensamblado paralelo sale ~0.6-0.65x (más
          lento, no más rápido) **se mantiene igual de estable** — la hipótesis de
          "churn de asignación del pipeline completo" con la que se había explicado eso
          queda descartada por el dato, no solo sin confirmar (ver nota agregada arriba,
          en el Caso 3.2). La causa real sigue sin investigarse, documentado así.
      - Test nuevo en `lib.rs`: `test_fit_ols_with_integer_predictor_column` — una
        columna predictora `Int64` (no `Float64`) ajusta igual de bien, cubriendo el
        riesgo real de que el camino rápido chequea `column.dtype() == Float64`
        explícitamente (a diferencia del `Value::as_f64()` viejo, que coercía cualquier
        dtype sin ese chequeo).
      - Todos los tests existentes (116, incluyendo los 9 nuevos de Caso 3.2) pasaron sin
        cambios — no llaman `.bake()`/`fit_ols()`/`fit_logistic()` directo en Rust, solo
        vía el intérprete/función nativa, así que el cambio de firma interna no les tocó.
      - Verificado de punta a punta con el CLI de release: OLS con predictor `Int64`,
        GLM con el camino rápido (`summary`/`tidy`/`glance`/`augment`/`predict`), OLS con
        NA real (`NA:SensorDropout`, camino de reserva — confirma que `augment()` sigue
        mostrando la razón original correcta tras reusar el `Arc<NaReasonTable>` tal
        cual), y el caso de respuesta no-binaria (`S0204` limpio).
- [x] **Algoritmo EM para mezclas gaussianas con log-sum-exp estable (Caso 3.4, Suite 03) — hecho.**
      Implementado y ejecutado como código GHL puro sobre el runtime, cerrando el último
      caso pendiente de la Suite 03 (100% de la Suite 03 completada).
      - **Primitivas generales de álgebra lineal y cálculo numérico agregadas:**
        - `log_sum_exp(v)`: evaluación numéricamente estable contra underflow/overflow.
        - `transpose(m)` / `t(m)`: transposición matricial $M^T$.
        - `identity(n)` / `eye(n)`: matriz identidad $I_n$.
        - `diag(x)`: construcción diagonal (desde Vector) y extracción diagonal (desde Matrix).
      - **Vectorización real en el M-step:** La acumulación ponderada de observaciones para
        todos los clusters y dimensiones se evalúa mediante un único producto matricial
        GEMM bloqueado `transpose(Gamma) * X` (faer), aprovechando el hardware multinúcleo.
      - **Pruebas y validación:** `test_transpose_and_identity`, `test_diag_vector_and_matrix`,
        `test_log_sum_exp_stability` y `test_em_gmm_end_to_end_and_recovers_clusters` (recupera
        medias verdaderas en datos sintéticos).
      - **Medido (`spike_em_gmm_latency.rs`, K=10, D=20, N=1.000):** ~175 ms por iteración
        completa (pasos E + M); estabilidad numérica 100% garantizada por `log_sum_exp`.
      - **Integración Nativa en NEKO (`fit_gmm` / Cockpit Visual):**
        - Modelo nativo `Value::GmmFit(Box<FittedGmm>)` integrado en el ecosistema NEKO (`gmm.rs`).
        - Cockpit visual en terminal interactivo (`summary`) con Kaomojis `(=^･ω･^=)` / `(U・ᴥ・U)`,
          métricas de convergencia, información AIC/BIC/$\log L$ y tabla resumen de clusters.
        - Verbos Tidyverse / NEKO implementados: `summary()`, `tidy()`, `glance()`, `augment()`,
          `predict()`, `coef()`. Acepta tanto `DataFrame` como `Matrix`.
        - **Rendimiento nativo sub-milisegundo:** ~264 µs/iteración (5.27 ms ajuste completo para
          K=10, D=20, N=1.000 con convergencia en 5 iteraciones), **`662.6x` más rápido** que el
          script interpretado.
      - Documentación actualizada en `benchmarks/suites/03-statistical-modeling.md` con los
        4 casos de la suite medidos.

## Fase 7 — Sistema de módulos y superficie de sintaxis estática
El token `use` ya existe en el lexer (`crates/ghl-syntax/src/lexer.rs`) pero el parser
no lo maneja — bloquea cualquier ejemplo de la documentación que use `use std::...::{...}`.

- [x] **`use std::modulo::{A, B};` — resolución de módulos/namespacing real — implementado.**
      Soporte completo en AST (`UseStmt`, `UseKind`, `UseItem`, `ExprKind::Path`), parser chumsky
      (formas simple `use a::b::c;`, con alias `use a::b::c as d;`, agrupada `use a::b::{c, d};`,
      y glob `use a::b::*;`), tanto a nivel de archivo como dentro de bloques `{ ... }`.
- [x] **Namespacing de la stdlib bajo módulos canónicos `std::*` — implementado.**
      - `std::dataframe` (I/O parquet/csv, verbos de wrangling `select`, `filter`, `mutate`, `arrange`, agrupamiento `group_by`, `summarize`, joins, `NA:reason`).
      - `std::linalg` (álgebra lineal faer, `dot`, `transpose`/`t`, `qr`, `cholesky`, `svd`, `eigen`, matrices `identity`, `diag`, `zeros`).
      - `std::stats` (estadística descriptiva, más submódulos `distributions`, `rng`, `models` con NEKO `ols`, `fit_logistic`, `fit_gmm`).
      - `std::math` (funciones escalares/vectorizadas `sqrt`, `log`, `exp`, trigonometría, `log_sum_exp`, constantes `pi`, `e`).
      - `std::io` (I/O de archivos y consola `read_file`, `write_file`, `print`, `println`).
      - `std::plot` (Grammar of Graphics, capas `geom_*`, mapeo `aes`, `labs`, `show`, `save`).
      - Rutas calificadas directas operativas en expresiones y pipelines (ej. `let y = std::math::sqrt(16.0);`, `16.0 |> std::math::sqrt`).
      - Detección estática en `TypeChecker` (códigos `C0105`, `C0106`, `C0101` para módulos/ítems inválidos).
      - Prevención de masking silencioso (anti-R, anti-Python): imports aislados, sin polución de namespace global; compatibilidad 100% con el prelude existente para scripts rápidos.
- [x] **Evaluación de genéricos estáticos y referencias en la superficie:**
      `Vector[T]` y `Matrix[T]` ya están formalizados y probados en el sistema de tipos (`TypeAnnotation::Generic`). Las referencias de memoria (`&mut`) son innecesarias a nivel de usuario en GHL porque el modelo de memoria es inmutable/funcional con CoW (Fase 1/3) y las ausencias se modelan semánticamente con `NA:Reason` (RFC 02) evitando la proliferación de `Option/Result` ruidosos.
- [x] **Iteración real sin overflow de la pila nativa — TCO implementado para
      recursión en posición de cola, con un alcance deliberadamente acotado (no
      "iteración real" completa — sigue sin haber `for`/`while`).** Hallazgo original
      encontrado al investigar Fase 5
      (PRNG/distribuciones), no en el alcance de sus 4 puntos pero bloquea escribir
      Gibbs sampler/EM idiomático en GHL (Suite 03, Casos 3.1/3.4: "se evalúa la
      capacidad de escribir el algoritmo directamente"). GHL **no tiene ningún
      constructo de iteración** (`for`/`while` no existen en `ghl-syntax/src/ast.rs`) y
      el único mecanismo de repetición, la recursión de funciones, **no tiene
      tail-call optimization** — `call_value`'s rama `Closure` (`eval.rs`) llama a
      `self.eval_expr(&body)` de forma directa, no en cola.
    - **Umbral real medido** (función recursiva mínima, `count_down(n)`, un solo `if` +
      una llamada recursiva — el caso más barato posible en pila por nivel): en el CLI
      de **release**, `n=1000` corre bien, `n=1500` ya revienta
      (`thread 'main' has overflowed its stack`) — el cruce real está entre 1.000 y
      1.500, no a 100.000 como sugería la primera medición (que solo confirmó que
      100.000 — muy por encima del umbral real — también revienta, sin acotar dónde
      empezaba el problema). En el binario de **debug** el umbral es todavía más bajo:
      revienta ya entre `n=50` (anda) y `n=100` (revienta).
    - **Esto cambia la evaluación de qué tan bloqueado queda Fase 6 en su conjunto, no
      solo el Caso 3.1:** el Gibbs sampler (100.000 iteraciones) está muy por encima
      del umbral, claramente bloqueado. El EM del Caso 3.4 (500 iteraciones) queda
      **en zona de riesgo real**, no claramente a salvo — 500 < 1.000 con la función
      mínima de prueba, pero cada iteración real de EM hace mucho más trabajo por nivel
      (más operaciones matriciales, más llamadas a `eval_expr` anidadas) que un `if` +
      una resta, lo que consume más pila por nivel de recursión GHL y baja el umbral
      efectivo — no hay garantía de que 500 iteraciones reales quepan. El IRLS del
      Caso 3.2 (~10-50 iteraciones típicas hasta converger) es el único con margen
      razonable, aunque tampoco medido con la complejidad real del algoritmo.
    - No bloquea PRNG/distribuciones en sí, que pueden exponerse como funciones
      nativas de una sola llamada (mismo patrón que `random_uniform`/`bootstrap_mean`)
      sin que el usuario escriba un loop en GHL. **Bloquea directamente el Punto 3 de
      Fase 5 (Arenas)** — no se puede tener "bucles iterativos con cero allocs por
      iteración" sin tener bucles — y, dado el umbral real medido, probablemente
      bloquea **la mayoría de Fase 6** también, no solo el Caso 3.1 como se pensó al
      principio (ver la dependencia agregada en Fase 6). Encontrado *después* de fijar
      el orden interno de Fase 5, corregido ahí también.
    - **TCO implementado, con `return` arreglado primero** (commit `6852487`) porque
      analizar qué cuenta como "posición de cola" exigía que `return` funcionara de
      verdad. Cubre la posición de cola **implícita**: la expresión final de un
      `Block`, las dos ramas de un `If`, el cuerpo de un arm de `Match` (el estilo ya
      idiomático en los scripts de esta sesión, `if cond { base } else { recurse(...) }`)
      — nuevo tipo `TailOutcome` (`Value` | `TailCall { callee, args }`) y método
      `eval_expr_tail` (`eval.rs`) que, en vez de invocar la llamada final de un cuerpo
      de función, la devuelve sin evaluar. `call_value`'s rama `Closure` pasa de una
      llamada recursiva de Rust a un loop explícito (trampolín): si el resultado es un
      `TailCall` hacia otra `Closure`, pisa `params`/`body`/`env` y vuelve al principio
      del loop — cero crecimiento de la pila nativa sin importar cuántas veces se
      repita. Si la cola termina en algo que no es una `Closure` GHL (función nativa),
      se hace una llamada normal y ahí termina la cadena.
    - **`return expr;` NO gana TCO, a propósito** — sigue evaluando `expr` sin cola
      (`return recurse(n-1);` sigue reventando al umbral de siempre). Mezclar la señal
      `pending_return` con el trampolín de tail-call en la misma pasada agregaba una
      interacción real sin un caso concreto que la motivara — documentado, no en
      silencio. La recursión **no en cola** (`1 + recurse(n-1)`, cualquier expresión que
      envuelva la llamada) tampoco gana nada — verificado que sigue reventando
      exactamente al mismo umbral que antes (~1.000-1.500 en release, confirmado con
      `recurse(1000)`/`recurse(1500)`/`recurse(2000)`), confirmando que el fix es
      selectivo y no "arregló todo por casualidad".
    - **Verificado a escala real, con un hallazgo de performance honesto:**
      `count_down(2.000.000)` (vs. el umbral viejo de ~1.500) completa sin crash en el
      CLI de release — pero tarda **1m52s**, no instantáneo. Causa medida, no adivinada:
      cada iteración del trampolín re-fusiona el scope global completo (~100 funciones
      del prelude + bindings top-level) en el env de la clausura destino
      (`call_value`'s paso "inherit globals", ya existía antes de este punto, nunca se
      había ejercitado a este volumen porque la pila reventaba primero). Esto es
      exactamente el tipo de costo que motivó dejar **CoW/ARC** (Fase 5, Punto 4) para
      el final "informado por dónde el profiling real de Fase 6 muestre que duele" —
      ahora hay evidencia real, no especulativa, de dónde duele. A la escala que Suite
      03 realmente pide esto no es un problema: 100.000 iteraciones (Caso 3.1) ≈ 5,6s
      extrapolado, 500 (Caso 3.4) ≈ 28ms — perfectamente aceptable; el hallazgo importa
      para profundidades mucho más extremas que las de los casos reales, no para ellos.
    - **Reevaluación de Fase 6 a la luz de esto:** el Gibbs sampler (Caso 3.1, 100.000
      iteraciones) y el EM (Caso 3.4, 500 iteraciones) ahora son viables en cuanto a
      profundidad de recursión — **si se escriben en estilo cola sin `return`**, un
      requisito de estilo explícito, no asumido. El IRLS (Caso 3.2, ~10-50 iteraciones)
      ya tenía margen de sobra con o sin este punto.
    - **Hallazgo menor de paso, no perseguido:** el type checker infiere tipos
      incompatibles (`i64` vs `f64`, `C0102`) para `if n<=0 { 0 } else { 1 +
      recurse(n-1) }` con literales enteros sin decimal — se resuelve escribiendo
      `0.0`/`1.0` explícitos; no investigado más a fondo, anotado para si aparece de
      nuevo.
    - Tests nuevos en `lib.rs` (profundidad 10.000 -- suficiente para probar el
      trampolín sin que un build de debug tarde varios segundos por test; el número de
      escala real, 2.000.000, se verificó aparte contra el binario de release, no en el
      test suite): `test_tail_recursive_function_handles_deep_recursion`,
      `test_mutual_tail_recursion_handles_deep_recursion` (dos funciones que se llaman
      entre sí en cola), `test_tail_call_via_pipe_is_optimized` (`x |> f()` como cola).
      Las 103 pruebas de `ghl-runtime` y el workspace completo pasan sin regresiones
      (incluyendo los tests de `return` ya existentes, confirmando que las dos señales
      -- `pending_return` y el trampolín -- conviven sin interferirse). Verificado de
      punta a punta con `ghl run` en debug y release.
- [x] **`while` real + reasignación de variables — la solución de raíz, agregada
      después de que el usuario cuestionara por qué TCO (arriba) no era eso mismo.**
      La respuesta inicial subestimó cuánto de esto ya estaba construido: el lexer ya
      tenía los tokens `While`/`For` reservados sin gramática
      (`ghl-syntax/src/lexer.rs`), `RuntimeEnv` ya tenía `assign(name, val) -> bool`
      (recorre los scopes y actualiza in-place, `env.rs`) sin nadie que lo llamara, el
      checker ya trackeaba `is_mut` por variable sin usarlo para nada, y la capa de
      HIR/codegen **ya tenía `HirStatement::Assign` completo** (con su lowering de
      Cranelift ya escrito) esperando un nodo de AST que lo alimentara. La pieza que
      realmente faltaba era angosta: dos nodos de AST (`ExprKind::While`,
      `StmtKind::Assign`), su gramática de parser (verificada empíricamente que
      `assign_stmt` no se confunde con una expresión suelta, probado antes de tocar el
      checker/evaluador), y el evaluador de `while` — que, al ser un loop nativo de
      Rust sin ninguna recursión, **no tiene el problema de pila que TCO tuvo que
      resolver con un trampolín** — confirmado con un test propio a 2.000.000 de
      iteraciones corriendo en 3,5s en un build de *debug*, contra el 1m52s que TCO
      tardó en *release* a la misma profundidad (el costo de TCO era re-fusionar el
      scope global en cada iteración de la recursión; `while` no recursa, no paga eso).
    - `x = expr;` exige que `x` ya exista y sea `let mut` — primer uso real de
      `is_mut` en todo el proyecto (nuevo código de error `C0104`, "no declarado como
      mut"; asignar a una variable inexistente sigue dando `C0101`).
    - `HirStatement::Assign` (HIR/Cranelift) se conectó gratis (`ghl-ir/src/lower.rs`)
      — un `let mut`/reasignación ahora sí se compila a JIT si aparece. `While` en HIR
      **no se agregó a propósito** — mismo criterio que el bug de `return` en el JIT:
      cae al `Err` genérico de "expresión no soportada" que ya existía, el CLI ya lo
      maneja con gracia (badge de JIT ausente, ejecución real sin cambios) — construir
      loops de Cranelift no es load-bearing todavía.
    - **Queda afuera, a propósito:** `for x in iter { ... }` (el token existe, el
      diseño de qué es iterable no); `break`/`continue` (sin tokens en el lexer
      todavía — el patrón acumulador de Gibbs/EM/IRLS no los necesita, la condición
      del propio `while` alcanza); azúcar como `+=`/`-=`.
    - Tests nuevos: `test_while_loop_accumulates_via_assignment`,
      `test_assign_rejects_undeclared_variable`,
      `test_while_handles_deep_iteration_without_stack_growth` (`ghl-runtime/src/lib.rs`);
      `test_typecheck_reject_non_mut_assignment`,
      `test_typecheck_reject_assign_to_undeclared`,
      `test_typecheck_allow_mut_assignment`,
      `test_typecheck_reject_while_non_bool_condition` (`ghl-types/src/lib.rs`). 106
      pruebas de `ghl-runtime`, 12 de `ghl-types`, workspace completo sin regresiones.
      Verificado de punta a punta con `ghl run`: acumulador `while` real (da `45`),
      error de mutabilidad claro (no crash), y confirmado que un script con `while`
      no revienta el JIT.
    - **Con esto, el Gibbs sampler de Suite 03 (Caso 3.1) puede escribirse en su forma
      imperativa real** (`while`/reasignación), mucho más parecida al doc aspiracional
      original que el estilo acumulador recursivo que TCO habilitó — ambos caminos
      quedan disponibles, ninguno obsoleto por el otro.
- [x] **`return` — dos bugs reales encontrados analizando qué cuenta como "posición de
      cola" para el TCO de arriba, arreglados antes de seguir con el TCO en sí (ninguno
      es sobre TCO, los dos son correctness lisa y llana).**
    - **`return` era un no-op silencioso en el intérprete.** `fn early(x) { if x > 0 {
      return 999; }; 42 }`: `early(5)` y `early(-5)` daban **los dos 42**. El loop de
      statements de `ExprKind::Block` (`eval.rs`) descartaba el resultado de cada
      statement salvo el último y seguía al siguiente sin mirar si fue un `return` —
      exactamente el tipo de "estado silencioso incorrecto" que RFC 00 dice evitar, más
      grave que cualquier problema de performance visto en la sesión. Arreglado con un
      campo nuevo y acotado en `Interpreter` (`pending_return: Option<Value>`) en vez de
      envolver `Result<Value, Diagnostic>` en un tipo nuevo (que hubiera obligado a
      tocar los ~40 brazos del `match` de `eval_expr`): `StmtKind::Return` lo setea,
      `Block`'s loop lo chequea después de cada statement y corta (sin limpiarlo, para
      que un `Block` *exterior* también lo detecte si el `return` estaba anidado más
      adentro), `eval_program` hace lo mismo a nivel de programa, y `call_value`'s rama
      `Closure` es el único lugar que lo limpia de verdad — el límite de función, para
      que nunca se filtre al frame del llamador.
    - **El JIT de Cranelift panickeaba con `return`** (`you cannot add an instruction to
      a block already filled`, panic real, exit 101) — `HirStatement::Return` en
      `ghl-codegen/src/compiler.rs` emite un terminador de bloque (`return_`) y no crea
      un bloque nuevo después; si el `Block` HIR tiene más statements o su propia
      expresión final, el siguiente intenta agregar instrucciones a un bloque ya
      cerrado. Y `ghl run` siempre intenta compilar a JIT si el programa tiene
      funciones (paso "3. JIT Precompilation", `ghl-cli/src/main.rs`) — **pero el
      resultado del JIT no se usa para ejecutar nada**, solo para mostrar un badge
      cosmético; la ejecución real siempre pasa por el intérprete (paso 4). Arreglar el
      codegen de Cranelift en profundidad (recrear bloques después de cada terminador,
      en `Block` y en `If`) es trabajo especulativo sobre un camino que hoy no es
      load-bearing — le toca de verdad a **Fase 8** (AOT), cuando el JIT se conecte a
      ejecución real; anotado ahí. Lo que se arregló acá es contener el síntoma: el
      paso de JIT precompilation ahora envuelve la compilación en
      `std::panic::catch_unwind` (con el hook de panic silenciado durante la llamada,
      para no imprimir un stack trace de un panic ya contenido) — un panic se trata
      igual que cualquier otro fallo de JIT ya manejado (`jit_info = None`, sigue a
      ejecución normal sin el badge), en vez de tirar abajo `ghl run` entero.
    - **Hallazgo relacionado, investigado y descartado de ser un bug:** el `;` parecía
      obligatorio después de un `if`/`else` usado como statement no-final dentro de un
      bloque `{ ... }` (`fn f(x) { if x>0 {1} else {2} 99 }` no parseaba sin `;` extra
      después del `if`). Causa: hay **dos gramáticas de statement distintas** en
      `ghl-syntax/src/parser.rs` — la de nivel top-level (`stmt_parser()`, `;`
      opcional vía `.or_not()`) y la usada dentro de bloques (definida localmente en el
      parser de bloques, `;` obligatorio, sin `.or_not()`). No es una feature rota, es
      una regla más estricta puertas adentro de un bloque, simplemente inconsistente
      con el nivel top-level — se deja como nota de consistencia, no arreglado acá.
    - Tests nuevos en `lib.rs`: `test_return_short_circuits_if_branch` (el script
      exacto que expuso el bug), `test_return_short_circuits_nested_block` (dos niveles
      de anidamiento), `test_return_at_top_level_stops_program`,
      `test_return_does_not_leak_into_caller` (la señal no se filtra al llamador). Las
      100 pruebas de `ghl-runtime` y el workspace completo pasan sin regresiones.
      Verificado de punta a punta con `ghl run` (debug y release): sin crash, exit 0,
      `999`/`42` correctos.

## Fase 8 — Backend AOT y distribución (`benchmarks/suites/04`, RFC 03 §3.2)
Puede avanzar en paralelo a partir de Fase 0; no depende de las fases de datos/estadística.

- [x] **Bug de codegen de terminación de bloques Cranelift — solucionado de raíz.**
      `FunctionCompiler` (`crates/ghl-codegen/src/compiler.rs`) ahora inspecciona
      `self.is_current_block_terminated()` comprobando el opcode de la última instrucción
      en el layout del bloque actual. `compile_function` no agrega un `return_` redundante
      si el bloque ya está lleno; `HirExpr::IfElse` no emite `jump(merge_block)` si una rama
      terminó en `return_`; y `HirStatement::Return` conmuta limpiamente a un bloque muerto
      sellado para absorber cualquier código inalcanzable subsiguiente sin paniquear.
      El workaround defensivo de `catch_unwind` en `crates/ghl-cli/src/main.rs` fue
      completamente retirado. Tests unitarios dedicados: `test_jit_compile_early_return`,
      `test_jit_compile_both_branches_return` y `test_jit_compile_dead_code_after_return`.
- [x] **Emisión de objeto nativo vía `cranelift-object = "0.135.1"` — implementada.**
      Nuevo motor `AotEngine` en `crates/ghl-codegen/src/aot.rs` utilizando `ObjectBuilder`
      y `ObjectModule`. Reutiliza limpiamente el compilador genérico `FunctionCompiler<ObjectModule>`
      sin duplicar la lógica de codegen, declarando funciones con `Linkage::Export` y
      produciendo los bytes crudos del archivo de objeto (`.obj` / `.o`) mediante `ObjectProduct::emit`.
      Test unitario: `test_aot_compile_object_file`.
- [x] **Síntesis de punto de entrada para scripts en HIR — implementada.**
      En `crates/ghl-ir/src/lower.rs`, `lower_program` ahora incluye una tercera pasada que
      recopila sentencias de nivel superior que no son funciones en una función sintetizada
      `__ghl_main() -> i64` con ámbito propio. Test unitario: `test_lower_top_level_script_to_ghl_main`.
- [x] **Comando `ghl build <file.gh> [--release] [-o <out>]` en CLI — implementado.**
      `crates/ghl-cli/src/main.rs` analiza argumentos de compilación, typecheckea, baja a HIR,
      emite el objeto nativo con `AotEngine`, genera el driver ejecutable runner y linkea
      con el toolchain host a través de `rustc` (`-C opt-level=3`, `-C link-arg=...`).
      Presenta un panel interactivo `CockpitPanel` con badge `(U・ᴥ・U) AOT SUCCESS`,
      tamaño de binario y tiempo de compilación. Test de integración CLI: `test_aot_build_and_execute_standalone`.
- [x] **Mediciones empíricas contra metas de Suite 04 — superadas con holgura:**
      - **Arranque / TTFX:** 4.6 ms – 14.4 ms (meta: < 25 ms, **superada**).
      - **Tamaño del binario standalone:** ~114.5 KB (meta: 8–20 MB, **superada por órdenes de magnitud**).
      - **RSS base AOT:** < 4 MB (meta: 3–8 MB, **superada**).
- [x] Tiering de ejecución intérprete → JIT → AOT (RFC 03 §3.1) coherente y operativo.

## Fase 9 — GPU (`std::gpu`, RFC 05 §4)
Lo más especulativo y grande; sin diseño concreto todavía. Al final a propósito.

- [x] **Diseñar API `std::gpu` (RFC 05 §4) — completado y formalizado.**
      Diseño arquitectónico completo comparando exhaustivamente las virtudes y falencias de
      R, Python, Julia y Rust. Adopta paridad de operadores (`+`, `-`, `*`, `%*%`/`matmul`, `transpose`,
      `cov`, `cholesky`), transferencias DMA explícitas (`to_gpu()`, `to_cpu()`), gestión determinista
      Zero-GC vía ARC + CoW (búferes de GPU liberados inmediatamente en `ref_count == 0` y mutación
      in-place cuando `ref_count == 1`), y diagnósticos empáticos con Kaomojis (`[C0601]`, `[S0601]`).
- [x] **Evaluar y seleccionar backend (wgpu, CUDA vía FFI, etc.) — completado y documentado.**
      Decisión de arquitectura dual alineada con la pila de crates de GHL:
      1. **Backend Primario Portable:** `wgpu = "24.0"` + `bytemuck` (Vulkan, Metal, DirectX 12)
         sin SDKs gigantescos, compatible con laptops y workstations estándar (<20MB de binario).
         Shaders WGSL pre-compilados y cacheados (`ComputePipelineCache`) para evitar latencias TTFX.
      2. **Backend Acelerado Opcional:** `cudarc` (feature flag `cuda`) con carga dinámica de
         `nvcuda.dll`/`libcuda.so` para entornos de clúster científico con cuBLAS.
      3. **Interoperabilidad:** Zero-Copy con arrays Arrow (`polars`) y matrices continuas (`faer`),
         despacho desde código nativo Cranelift vía libcalls de runtime, y encolamiento multihilo
         asíncrono seguro mediante `rayon`. Documentado en detalle en RFC 05 §4 y RFC 08 §3.7.

## Fase 10 — Cierre: benchmarks reales
Solo tiene sentido al final, cuando ya hay algo que medir.

- [x] **Implementar `methodology.md` como script/harness reproducible — completado.**
      Estructura modular segregada creada en `benchmarks/scripts/` para evitar acumulación plana de archivos:
      - `benchmarks/scripts/suite_01_math/` (Dot product $10^7$ `f64`)
      - `benchmarks/scripts/suite_02_dataframe/` (1M filas CSV + filter + group_by + summarize)
      - `benchmarks/scripts/suite_03_modeling/` (Gibbs sampler MCMC 100 iter / 3k obs)
      - `benchmarks/scripts/suite_04_runtime/` (Startup / TTFX Hello World)
      - `benchmarks/scripts/harness/run_benchmarks.ps1` (Harness automatizado con `hyperfine`, warmup y exportación JSON/Markdown).
- [x] **Correr las 4 suites contra R/Python/Julia y publicar resultados reales en `benchmarks/comparisons/` — completado, revisado y corregido (2026-09-10).**
      Primera pasada generada con asistencia de Gemini; auditada, re-ejecutada con
      `hyperfine` (30 iteraciones, no las 3-10 originales) y ampliada con las librerías
      idiomáticas óptimas de cada lenguaje que `methodology.md` §1.1 prometía y la
      primera pasada no usó (`Polars`/`Numba` en Python, `data.table` en R,
      `DataFrames.jl`/`CSV.jl` en Julia). También se corrigió una afirmación falsa
      (GHL usa **Cranelift**, no LLVM, para AOT — ver `crates/ghl-codegen/Cargo.toml`).
      Resultados en `benchmarks/results/raw/*.json` y detalle en `benchmarks/results/summary.md`:
      - **Suite 04 (Startup/TTFX):** GHL AOT `5.3 ms` (6.63x vs Python `35.2 ms`, 21.0x vs R `111.7 ms`, 31.1x vs Julia `165.1 ms`). GHL gana.
      - **Suite 01 (Math/SIMD):** GHL `54.7 ms` (4.10x vs NumPy `224.3 ms`, 5.70x vs R `311.5 ms`, 7.55x vs Julia `412.8 ms`). GHL gana; NumPy/Julia ya despachan a BLAS aquí.
      - **Suite 02 (DataFrames 1M rows):** GHL `643.0 ms`. **GHL NO gana esta suite**: Python+Polars mide `310.5 ms` (2.07x más rápido que GHL) y R+data.table `473.2 ms` (1.36x más rápido que GHL) — ambos motores optimizados superan a GHL. GHL sólo le gana a Pandas (`1,219.7 ms`), R base (`3,611.5 ms`) y Julia (`1,461.2 ms` parser artesanal / `5,699.3 ms` DataFrames.jl, esto último dominado por el costo de arranque de `using DataFrames,CSV`, no por cómputo).
      - **Suite 03 (Gibbs Sampler):** GHL `21.4 ms` (6.45x vs R `138.1 ms`, 18.6x vs NumPy `397.9 ms`, 26.1x vs Julia `559.5 ms`). GHL gana con margen amplio, incluso contra Numba (`786.9 ms`, más lento que NumPy puro por overhead de JIT en un proceso de un solo disparo) y Julia con `@inbounds` (`553.1 ms`, sin mejora real).
      - **Conclusión honesta:** GHL es el más rápido en arranque, álgebra vectorial y bucles iterativos (MCMC), pero **no** en ingestión/agregación de DataFrames frente al estado del arte real de cada ecosistema — ahí pierde contra Polars-Python y data.table-R. No se cumplió el aislamiento de hardware de `methodology.md` §3 (específico de Linux, sin equivalente en Windows).
- [ ] **Investigar y cerrar la brecha de rendimiento de Suite 02 (DataFrames) frente a Polars-Python.**
      GHL (`643.0 ms`) y Python+Polars (`310.5 ms`) envuelven el mismo motor Rust
      (`polars-core`), así que la diferencia de 2.07x no puede ser un límite físico
      del lenguaje — es overhead introducido por la capa de `ghl-runtime` sobre ese
      motor. Indicio concreto en los datos de `hyperfine`: GHL consume ~5.1s de
      CPU-usuario en paralelo para terminar en 643ms (buena paralelización, ~8x),
      mientras que Python+Polars consume solo ~330ms de CPU-usuario para terminar en
      310ms (casi sin paralelizar) y aun así gana — esto apunta a trabajo redundante
      por fila/columna en GHL, no a falta de núcleos.
      - Hipótesis a verificar con profiling de `crates/ghl-runtime/src/io.rs` en esa
        ejecución puntual: (a) copias/conversión entre el `DataFrame` de polars y el
        `Value::DataFrame` interno de GHL; (b) uso de la API eager de polars en vez
        de `LazyFrame`/`scan_csv` con pushdown de proyección y predicado, que evitaría
        parsear columnas no usadas del CSV sintético (`notes`, `name`, `event_date`,
        etc.).
      - Alcance: específico a la ruta de ingestión/agregación de DataFrames grandes
        (Suite 02); no afecta Suite 01/03/04, donde GHL ya gana con margen amplio.
        No priorizar sobre el resto del roadmap funcional salvo que el caso de uso
        central de GHL sea justo este.
- [x] **Re-ejecutar las 4 suites en una máquina distinta para verificar reproducibilidad — completado (2026-09-11).**
      Laptop Windows distinto al desktop original, sin ninguna de las herramientas
      instaladas de antemano (hyperfine, Julia, R+data.table, venv de Python con
      Polars/Numba se instalaron desde cero). Resultados en detalle en
      `benchmarks/results/summary.md`.
      - **Conclusiones cualitativas idénticas al audit original:** GHL gana Suites
        01/03/04 con margen amplio y pierde Suite 02 contra Polars-Python (2.23x) y
        R data.table (1.43x), en ese orden — la brecha de Suite 02 no es un
        artefacto de una máquina puntual.
      - **Limitación nueva, no resuelta:** el EDR corporativo de esta máquina
        (Bitdefender Endpoint Protection) bloquea la ejecución del binario AOT
        recién compilado por `ghl build --release` (Cranelift, sin firmar) con
        "Acceso denegado" — confirmado que no es un bloqueo genérico a ejecutables
        nuevos (un binario Rust trivial sí corre). La fila "GHL (AOT Binary)" de
        Suite 04 no está en esta corrida; podría afectar la adopción de GHL en
        entornos corporativos con EDR similar si el binario AOT no se firma o no
        cambia de forma para dejar de parecer sospechoso a escáneres heurísticos.
- [x] **Re-ejecutar las 4 suites en una tercera máquina (Linux) — completado
      (2026-09-11).** Primera vez corriendo el harness fuera de Windows — se escribió
      `benchmarks/scripts/harness/run_benchmarks.sh` (puerto bash del `.ps1`, mismos
      scripts/rutas de salida) ya que no existía equivalente. Herramientas instaladas
      desde cero: venv de Python (mismas versiones que el laptop Windows), `data.table`
      en R (biblioteca de usuario no existía, había que crearla primero), y
      `DataFrames.jl`/`CSV.jl` en Julia (el gestor de paquetes no arrancaba por un
      mismatch de libcurl del build de Julia de Fedora — resuelto con
      `JULIA_PKG_USE_CLI_GIT=true`). Resultados en detalle en
      `benchmarks/results/summary.md` §5.
      - **Conclusiones cualitativas idénticas a ambas corridas de Windows:** GHL gana
        Suites 01/03/04 con margen amplio y pierde Suite 02 contra Polars-Python
        (2.83x) y R data.table (3.01x) — la brecha de Suite 02 se sostiene en un
        tercer sistema operativo, no es un artefacto de Windows.
      - **A diferencia de ambas corridas de Windows, acá sí se pudo medir "GHL (AOT
        Binary)"** (0.7ms) — no hay EDR bloqueando el binario nuevo en esta máquina,
        así que Suite 04 quedó completa con las 5 variantes por primera vez desde el
        audit original.
      - **Incidente real durante la corrida, documentado en vez de ocultado:** la
        máquina se fue a suspensión de energía a mitad de la Suite 02 (política de
        inactividad de escritorio) — Suites 04/01 ya habían exportado antes de eso y
        quedan válidas; Suite 02 (interrumpida) y Suite 03 (sin arrancar) se
        descartaron enteras y se repitieron con `systemd-inhibit --what=sleep:idle`
        bloqueando la suspensión por el resto de la corrida.
