# Suite 02: Operaciones sobre DataFrames en GHL

- **Área:** Manipulación de Datos en Columnas / Ingestión / Agregación
- **Objetivo:** Evaluar la eficiencia del motor columnar, gestión de memoria caché, algoritmos de hashing para agrupaciones y aprovechamiento multinúcleo.

---

## 1. Casos de Prueba Especificados

### Caso 2.1: Ingestión y Parseo de CSV Masivo
- **Conjunto de Datos:** Archivo CSV sintético de 5 GB (~25 millones de filas, 12 columnas mixtas: enteros, flotantes, cadenas de texto y fechas).
- **Aspecto Evaluado:** Rendimiento del parser multi-hilo, detección automática de tipos e indexación en memoria sin copias redundantes.

### Caso 2.2: Agrupación de Alta Cardinalidad (Group-By Aggregation)
- **Operación:** Agrupar 25 millones de filas por una clave categórica de alta cardinalidad (100.000 grupos únicos) y calcular la suma, media y varianza de 3 columnas numéricas.
- **Aspecto Evaluado:** Eficiencia de la tabla hash columnar, distribución paralela entre hilos de CPU y gestión de contención de memoria.

### Caso 2.3: Filtrado y Mutación de Columnas con Datos Faltantes
- **Operación:** Filtrar filas con `col("score") > 75.0 && !is_na(col("category"))` y añadir una columna normalizada logarítmica.
- **Aspecto Evaluado:** Filtrado vectorial mediante bitmasks de validez Arrow y asignación sin copia (Copy-on-Write).

### Caso 2.4: Cruce de Tablas (Inner Join & Left Join)
- **Operación:** Cruce por clave primaria/foránea entre una tabla grande ($10^7$ filas) y una tabla de dimensiones ($10^5$ filas).
- **Aspecto Evaluado:** Rendimiento de *Hash Join* paralelo y particionamiento eficiente.

---

## 2. Entornos y Librerías Evaluadas

| Lenguaje | Implementación Estándar | Implementación de Alto Rendimiento |
| :--- | :--- | :--- |
| **R** | `data.frame` + `dplyr` | `data.table` / `collapse` |
| **Python** | `pandas` (NumPy backend) | `polars` (Rust backend) / `duckdb` |
| **Julia** | `DataFrames.jl` | `DataFrames.jl` multi-threaded |
| **GHL** | `polars-core` embebido vía `ghl-runtime` (TODO.md, Fase 0/1) | Motor Lazy (TODO.md, Fase 2 — todavía no implementado) |

---

## 3. Ejemplo de Consulta de Prueba en GHL (`query.gh`)

> **Nota (2026-09-07):** esta sección usaba antes una sintaxis de method-chaining
> (`df.filter(...).group_by(...).parallel().agg([...])`) que nunca coincidió con la
> sintaxis real del lenguaje — GHL usa pipes con funciones libres (`df |> filter(...)`).
> El ejemplo de abajo es real: corre tal cual contra el intérprete (`ghl run`), no es
> aspiracional. Ver `TODO.md`, Fase 1, para el detalle de qué verbos hay detrás.

```lang
let df = read_csv("events.csv"); // o read_parquet(...) — ver Caso 2.1

// Caso 2.2: agrupación de alta cardinalidad. Los nombres de columna van sin comillas
// dentro de filter/group_by/summarize (col_ctx, ver docs/design/01-syntax-and-grammar.md);
// `col("status")` con comillas también funciona, es la forma explícita equivalente.
let summary = df
    |> filter(status == "active")
    |> group_by(customer_id)
    |> summarize(
        total_revenue = sum(revenue),
        avg_revenue   = mean(revenue),
        sd_latency    = std_dev(latency_ms),
        event_count   = count()
    );

// Caso 2.3: filtrado vectorial + columna derivada. `filter()` hoy solo entiende un
// único predicado `col OP escalar` (vectorizado nativamente contra polars, ver TODO.md
// Fase 1) — el enunciado original de este caso ("score > 75.0 && !is_na(category)") no
// es expresable en una sola llamada todavía: no hay predicados compuestos (`&&`/`||`
// combinando dos condiciones de columna) ni una forma vectorizada de "excluir NA" que
// se pueda pasar como predicado de `filter()` (`is_na(col)` da un `Vector[Bool]` que
// sirve para inspección, pero envolver un `ColRef` en una llamada antes de comparar le
// hace perder a `filter()` la referencia a la columna). Con lo que existe hoy, la parte
// de score se filtra vectorizado y las filas con NA se dejan así, documentado como
// limitación real en vez de simulado con código que en verdad no filtra nada:
let filtered = df |> filter(score > 75.0);
let log_scores = log(pull(filtered, "score"));
let clean = filtered |> mutate("log_score", log_scores);

// Caso 2.4: cruce de tablas — hash join real vía polars-ops (TODO.md, Fase 1).
let joined = orders |> inner_join(customers, customer_id);
let joined_left = orders |> left_join(customers, customer_id);
```
