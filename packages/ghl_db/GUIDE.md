# Guía Técnica de `ghl_db`: Conector Analítico SQL para DuckDB y SQLite

El paquete [`ghl_db`](file:///o:/Documentos/Rust%20Project/ghl/packages/ghl_db) proporciona una infraestructura unificada de acceso a bases de datos relacionales y análisis SQL analítico para GHL. Su diseño de motor dual integra:

1. **Motor Analítico OLAP en Memoria (Zero-Copy):** transpilador y evaluador SQL de alto rendimiento (`polars-sql` / estilo DuckDB) que registra DataFrames nativos de GHL como tablas virtuales sin serialización ni copias redundantes.
2. **Motor Relacional Embebido OLTP (SQLite3):** conector nativo embebido (`rusqlite`) para almacenamiento local, creación de esquemas relacionales, transacciones ACID y persistencia en disco o memoria.
3. **Preservación Estricta de Razones de NA (NA Provenance):** mapeo automático de valores `NULL` de bases de datos a valores `NA` tipados con razón `"db.null"`, asegurando trazabilidad científica.

---

## 1. Arquitectura y Comparativa de Motores

```
┌─────────────────────────────────────────────────────────────┐
│                       GHL User Code                         │
│       query_sql()        db_query()        db_execute()     │
└──────────────┬───────────────────┬──────────────────────────┘
               │                   │
               ▼                   ▼
┌───────────────────────────┐ ┌───────────────────────────────┐
│     Motor OLAP (DuckDB)   │ │      Motor OLTP (SQLite)      │
│  - Consultas analíticas   │ │  - Persistencia ACID          │
│  - Zero-Copy sobre DF     │ │  - Ingesta de datos           │
│  - Agregaciones complejas │ │  - Esquemas relacionales DDL  │
│  - JOINs vectorizados     │ │  - Archivos .db locales       │
└──────────────┬────────────┘ └──────────────┬────────────────┘
               │                             │
               └──────────────┬──────────────┘
                              ▼
           ┌─────────────────────────────────────┐
           │        GHL DataFrame + NA Table     │
           │  (Mapeo de NULL -> NA "db.null")    │
           └─────────────────────────────────────┘
```

| Característica | Motor OLAP (DuckDB / Polars SQL) | Motor OLTP (SQLite3) |
| :--- | :--- | :--- |
| **Enfoque Principal** | Consultas analíticas sobre DataFrames | Persistencia relacional y transacciones |
| **Copia de Memoria** | **Zero-Copy** (apunta directo a columnas) | Ingesta tabular estructurada |
| **Soporte de Archivos** | Parquet, Arrow, CSV, DataFrames en RAM | Archivos `.db`, `.sqlite`, `:memory:` |
| **Operaciones DDL/DML** | Vistas virtuales y lectura optimizada | `CREATE TABLE`, `INSERT`, `UPDATE`, `DELETE` |
| **Preservación NA** | Conserva metadatos de NA | Mapea `NULL` $\to$ `NA(db.null)` |

---

## 2. API Rápida y Ergonomía

### 2.1 Consultas Analíticas Zero-Copy (`query_sql`)
Permite consultar uno o varios DataFrames existentes usando sintaxis estándar ANSI SQL:

```ghl
use ghl_db

let df_ventas = dataframe {
    region: ["Norte", "Sur", "Norte", "Este", "Sur", "Norte"],
    monto: [120.5, 85.0, 210.0, 95.0, 115.0, 150.0],
    unidades: [10, 5, 15, 8, 10, 12]
};

// Registro de múltiples tablas mediante un Record
let df_res = query_sql("
    SELECT 
        region, 
        COUNT(*) AS total_transacciones, 
        SUM(monto) AS ingreso_total, 
        AVG(monto) AS ticket_promedio 
    FROM ventas 
    WHERE monto > 90.0 
    GROUP BY region 
    ORDER BY ingreso_total DESC
", { ventas: df_ventas });

println(df_res);
```

### 2.2 Relaciones y JOINs entre Múltiples Tablas
```ghl
let df_clientes = dataframe {
    id: [1, 2, 3],
    nombre: ["Alice", "Bob", "Charlie"]
};

let df_pedidos = dataframe {
    pedido_id: [101, 102, 103, 104],
    cliente_id: [1, 2, 1, 3],
    total: [250.0, 120.0, 310.0, 80.0]
};

let df_reporte = query_sql("
    SELECT 
        c.nombre, 
        COUNT(p.pedido_id) AS total_pedidos, 
        SUM(p.total) AS gasto_acumulado 
    FROM clientes c 
    JOIN pedidos p ON c.id = p.cliente_id 
    GROUP BY c.nombre 
    ORDER BY gasto_acumulado DESC
", { clientes: df_clientes, pedidos: df_pedidos });
```

---

## 3. Conexión Relacional y Persistencia con SQLite

### 3.1 Ciclo de Vida: Conexión, Creación e Inserción
```ghl
use ghl_db

// 1. Conexión en memoria o a archivo persistente en disco
let conn = db_connect("sqlite", "datos_investigacion.db");

// 2. Ejecución de DDL
db_execute(conn, "
    CREATE TABLE IF NOT EXISTS pacientes (
        id INTEGER PRIMARY KEY,
        edad INTEGER,
        colesterol REAL,
        fuma INTEGER
    )
");

// 3. Inserción de registros (soportando NULL)
db_execute(conn, "
    INSERT INTO pacientes (edad, colesterol, fuma) 
    VALUES (45, 210.5, 0), (52, NULL, 1), (38, 185.0, 0)
");

// 4. Consulta y retorno directo como GHL DataFrame
let df_pacientes = db_query(conn, "SELECT * FROM pacientes WHERE edad >= 40");

// 5. Cierre seguro de descriptores
db_disconnect(conn);
```

### 3.2 Persistencia Directa de DataFrames a SQLite (`db_register`)
GHL puede persistir cualquier DataFrame generado durante un análisis en una tabla SQLite relacional con tipos nativos automáticos e inserción transaccional por lotes:

```ghl
let conn = db_connect("sqlite", ":memory:");

let df_sensores = dataframe {
    sensor_id: [101, 102, 103],
    temperatura: [21.5, 24.8, 19.2],
    activo: [true, true, false]
};

// Registra e inserta en la base de datos
db_register(conn, "sensores_iot", df_sensores);

// Listar tablas existentes
let tablas = db_tables(conn);
println("Tablas en DB: ", tablas); // ["sensores_iot"]

// Consultar directamente
let df_activos = db_query(conn, "SELECT * FROM sensores_iot WHERE activo = 1");
db_disconnect(conn);
```

---

## 4. Preservación Científica de Valores Faltantes (`NA`)

Cuando una columna en SQLite o DuckDB contiene un valor `NULL`, GHL no lo descarta ni lo rellena con ceros arbitrarios. Lo convierte en un valor `NA` preservando su procedencia en la tabla `NaReasonTable`:

```ghl
let conn = db_connect("sqlite", ":memory:");
db_execute(conn, "CREATE TABLE encuesta (id INT, respuesta TEXT)");
db_execute(conn, "INSERT INTO encuesta VALUES (1, 'Sí'), (2, NULL)");

let df = db_query(conn, "SELECT * FROM encuesta");
let resp_col = pull(df, "respuesta");

println(is_na(resp_col[1])); // true
// El motor registra internamente na_reason = "db.null"
db_disconnect(conn);
```

---

## 5. Panel Cockpit Deck

Para inspección rápida e interactiva del estado de la base de datos y esquemas:

```ghl
use ghl_db

let conn = db_connect("sqlite", ":memory:");
db_execute(conn, "CREATE TABLE experimentos (id INT, r2 REAL)");
db_cockpit(conn);
db_disconnect(conn);
```

Salida en terminal:
```text
╭─ ghl_db Engine ─────────────────────────────────── /ᐠ˵- ⩊ -˵マ ✧ CONNECTED ─╮
│ Driver:     sqlite
│ URI:        :memory:
│ Handle ID:  1
├──────────────────────────────────────────────────────────────────────────────┤
│ Tables in Database:                                                          │
│   [1] experimentos                                                           │
╰──────────────────────────────────────────────────────────────────────────────╯
```

---

## 6. Referencia de Funciones

| Función | Parámetros | Retorno | Descripción |
| :--- | :--- | :--- | :--- |
| `db_connect(driver, uri)` | `String, String` | `Record` | Abre una conexión (`"sqlite"`, `"duckdb"`, `":memory:"`). |
| `db_disconnect(conn)` | `Record` | `bool` | Cierra la conexión liberando recursos. |
| `db_execute(conn, sql)` | `Record, String` | `i64` | Ejecuta sentencias DDL/DML (`CREATE`, `INSERT`, etc.). |
| `db_query(conn, sql)` | `Record, String` | `DataFrame` | Ejecuta una consulta SQL y retorna un DataFrame con NA provenance. |
| `db_register(conn, name, df)` | `Record, String, DataFrame` | `bool` | Registra/persiste un DataFrame como tabla en la base de datos. |
| `db_tables(conn)` | `Record` | `Vector` | Lista los nombres de las tablas de la base de datos. |
| `query_sql(sql, tables)` | `String, Record` | `DataFrame` | Ejecuta SQL analítico zero-copy sobre DataFrames en memoria. |
| `db_cockpit(conn)` | `Record` | `()` | Imprime el panel Cockpit Deck con el estado de la conexión. |
