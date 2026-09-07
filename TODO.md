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
> Fase 0 decide si se adoptan esas librerías o se sigue a mano — esa decisión
> determina el diseño de todo lo demás, por eso va primero.

---

## Fase 0 — Decisión de arquitectura y adopción de dependencias
Bloquea todo lo demás: el diseño de `Value::DataFrame` / `Value::Matrix` depende de esto.

- [ ] Decidir: adoptar `polars-core`/`arrow2` + `faer` + `rayon` + `statrs`/`rand_distr` +
      `rand_xoshiro` + `bumpalo` (RFC 08) vs. seguir con implementación artesanal.
- [ ] Si se adopta: spike de integración mínima (una función de cada librería
      corriendo dentro de `ghl-runtime`) antes de comprometerse.
- [ ] Definir cómo conviven los `Value` dinámicos del intérprete con tipos
      estáticos/tipados de esas librerías (capa de conversión en los bordes).

## Fase 1 — Motor columnar + Joins (`benchmarks/suites/02`)
Es lo que el usuario pidió primero y lo que más impacto tiene sobre el resto de la Fase de datos.

- [ ] Reemplazar `Value::DataFrame { columns: Vec<String>, data: HashMap<String, Vec<Value>> }`
      por un backend columnar tipado (Arrow-compatible si se adoptó Fase 0),
      con bitmasks de validez en vez de `Value::NA` por celda.
- [ ] `inner_join(left, right, on)` / `left_join(left, right, on)` — hash join.
- [ ] Paralelizar `group_by`/`summarize` sobre múltiples hilos (tabla hash concurrente
      o particionado + merge).
- [ ] Parser CSV multi-hilo para archivos de varios GB (el actual es un parser
      simple in-memory de una sola pasada, ver `crates/ghl-runtime/src/io.rs`).
- [ ] Ingestión Parquet (mencionada en `benchmarks/README.md`, sin suite detallada).
- [ ] Generar el CSV sintético de 5GB / 25M filas / 12 columnas mixtas (Caso 2.1)
      + script reproducible para generarlo (no versionar el archivo en sí).
- [ ] Filtrado vectorial con bitmask de validez + asignación copy-on-write (Caso 2.3),
      depende del nuevo backend columnar.
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
      `faer` (o el motor elegido en Fase 0): LU, QR, Cholesky, SVD, autovalores.
- [ ] Multiplicación matricial densa multinúcleo por bloques (Caso 1.2).
- [ ] SIMD real (AVX2/AVX-512/NEON) para dot product y reducciones sobre `Vector` (Caso 1.1).
- [ ] Fusión de operaciones elemento-a-elemento sin buffers intermedios en heap —
      soporte real para `x.map(xi => log(1.0 + exp(-abs(xi))) + sin(xi))` (Caso 1.4).
- [ ] Métodos/sintaxis: `Vector::random_uniform(n)`, `.dot(&b)`, `.map(...)`.

## Fase 4 — Paralelismo transversal (RFC 05)
Se apoya en lo que exista de Fase 0 (`rayon` si se adoptó); habilita el resto de casos de Suite 03.

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
