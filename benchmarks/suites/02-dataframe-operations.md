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
| **GHL** | `std::dataframe` nativo | Motor Lazy / Arrow C Data Interface nativo |

---

## 3. Ejemplo de Consulta de Prueba en GHL (`query.gh`)

```lang
use std::dataframe::{DataFrame, col, count};

// Case 2.2: High-cardinality aggregation in GHL
let summary = df
    .filter(col("status") == "active")
    .group_by("customer_id")
    .parallel()
    .agg([
        col("revenue").sum().as("total_revenue"),
        col("revenue").mean().as("avg_revenue"),
        col("latency_ms").std().as("sd_latency"),
        count().as("event_count")
    ]);
```
