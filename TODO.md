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
  - **`n_distinct()` — explícitamente NO migrado en este punto.** El actual usa
    `format!("{:?}", it)` sobre los items boxeados como clave de dedup, lo que
    **distingue razones de NA distintas** como valores distintos (`NA:A` ≠ `NA:B`). El
    `.n_unique()` nativo de polars no sabe nada del side-channel de razones — migrarlo
    ingenuamente perdería esa distinción en silencio. Queda en el camino boxeado
    (correcto, no rápido) hasta que se decida si esa distinción vale la pena preservar
    acá también.
  - Tests nuevos: `test_dot_product_real_computation`,
    `test_dot_product_rejects_mismatched_lengths`,
    `test_dot_product_propagates_na_with_reason`,
    `test_mean_sum_preserve_na_reason_over_vector` (`lib.rs`). Las 54 pruebas de
    `ghl-runtime` y el test suite completo del workspace pasan sin cambiar aserciones
    preexistentes. Verificado de punta a punta con `ghl run`.
  - **Medido (`spike_vector_dot_latency.rs`, 10⁷ elementos — la escala que pide Caso
    1.1):** `dot()` a mano sobre `Vec<Value>` boxeado, 138-144ms → `dot()` real (`faer`,
    `VectorData::as_f64_view`), 9.7-9.9ms — **~14-15x**, estable en 3 corridas.
  - **Lista actualizada de qué sigue en el camino lento (boxeado)** — lo que quedó
    explícitamente afuera de este punto (ver arriba para el detalle de por qué cada uno):
    - `n_distinct()` (distinción de razones de NA, ver arriba).
    - Los 4 dispatchers compartidos — territorio del **Punto 3**: `map_numeric_fn`/
      `map_string_fn` (`log`/`sqrt`/`exp`/etc., `str_upper`/etc.), `cumulative`
      (`cumsum`/`cumprod`/`cummax`/`cummin`), `sort_vector` (`sort_asc`/`sort_desc`).
    - `lag`, `lead`, `rank`, `if_else`, `between`.
    - Aritmética/elementwise en `eval.rs`: broadcasting de escalar contra `Vector`, los
      operadores `.+`/`.-`/`.*`/`./` Vector-Vector, y `UnaryNeg` sobre un `Vector`.
    - Ninguna de estas quedó peor que antes — dan el resultado correcto, simplemente no
      están en el camino rápido todavía.
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
- [ ] Bootstrap paralelo sin duplicar la muestra base (memoria compartida, Caso 3.3). Aún
      no existe ningún código de bootstrap/resample en el runtime (confirmado por
      búsqueda exhaustiva). La base ya ayuda: `Column`/`Series` de polars es `Arc`-interno
      (clonar es O(1), no duplica el buffer), así que la "memoria compartida sin duplicar
      la muestra base" que pide el caso viene casi gratis de la representación actual —
      falta el propio algoritmo de remuestreo + su paralelización con rayon, y una decisión
      sobre RNG-por-réplica (roza con Fase 5: `rand::rng()` es seguro entre hilos de rayon
      pero no reproducible entre corridas ni entre distinta cantidad de hilos).

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
