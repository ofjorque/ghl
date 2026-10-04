### /ᐠ˵- ⩊ -˵マ ✧ Implementación y Cierre de Issue #10: Paquete `ghl_db`

Se ha implementado, probado y consolidado exitosamente el paquete [`ghl_db`](packages/ghl_db) junto con el motor analítico nativo en Rust para consultas SQL zero-copy y conectividad relacional embebida (DuckDB & SQLite).

Con esto, **se completa y cierra al 100% el Milestone `v0.3.0 - Nuevos Paquetes de Utilidad y Datos`**.

---

#### 1. Capacidades y Arquitectura de Motor Dual
- **Motor Analítico OLAP en Memoria (`query_sql` / `sql` / `sql_df`):**
  - Transpilación y ejecución ANSI SQL de alto rendimiento basada en `polars-sql`.
  - Registro de DataFrames nativos de GHL como tablas virtuales **sin copias ni serialización (Zero-Copy)**.
  - Soporte completo para proyecciones, filtros (`WHERE`), agregaciones vectorizadas (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`), cláusulas `HAVING`, `GROUP BY`, `ORDER BY` y `JOIN` entre múltiples DataFrames.
- **Motor Relacional Embebido OLTP (`rusqlite` con `bundled`):**
  - Conexión ligera sin dependencias externas a bases de datos SQLite en disco o en memoria (`:memory:`).
  - Ciclo de vida completo: `db_connect()`, `db_execute()`, `db_query()`, `db_register()`, `db_tables()`, `db_disconnect()`.
  - Soporte para DDL (`CREATE TABLE`), transacciones ACID e inserción masiva por lotes en bloque transaccional.
- **Persistencia de DataFrames a SQLite (`db_register`):**
  - Mapeo automático de esquemas de tipos de GHL a SQLite (`INTEGER`, `REAL`, `TEXT`).
  - Inserción masiva de DataFrames de GHL en tablas relacionales para ingesta y persistencia.
- **Preservación Científica de Datos Faltantes (NA Provenance):**
  - Los valores `NULL` provenientes de SQLite o DuckDB se mapean automáticamente a valores `NA` tipados de GHL con metadatos de procedencia `"db.null"` en `NaReasonTable`.

---

#### 2. Suite de Pruebas y Verificación
- **Pruebas Unitarias del Motor en Rust (`crates/ghl-runtime/src/native_db.rs`):**
  - `test_sqlite_in_memory_crud`: Verificación de creación de tabla, inserción con `NULL`, consulta con preservación de NA reason `"db.null"`, listado de tablas y desconexión.
  - `test_sqlite_register_dataframe`: Ingesta y consulta de DataFrames a tablas SQLite en memoria.
  - `test_polars_sql_zero_copy_query`: Agregación analítica sobre tablas en memoria con `GROUP BY` y ordenamiento.
  - Resultado: **3/3 pruebas superadas al 100%**.
- **Pruebas End-to-End en GHL (`packages/ghl_db/tests/db_test.gh`):**
  - `test_sqlite_in_memory_pipeline`: Validación de ciclo de vida completo de SQLite en memoria y mapeo de `NULL` a `is_na()`.
  - `test_zero_copy_analytical_sql`: Consulta analítica sobre DataFrame con `WHERE`, `COUNT(*)`, `SUM()`, `AVG()` y `ORDER BY`.
  - `test_multi_table_sql_join`: Operación `JOIN` relacional entre `customers` y `orders` con agregaciones agrupadas.
  - `test_dataframe_registration_to_sqlite`: Registro directo de DataFrame a tabla física SQLite y consulta filtrada.
  - Resultado: **4/4 suites superadas al 100%**.

---

#### 3. Entregables
- **Paquete GHL:** [`packages/ghl_db`](packages/ghl_db) (`ghl.toml`, `src/lib.gh`, `src/connection.gh`, `src/sql.gh`, `src/cockpit.gh`, `tests/db_test.gh`).
- **Motor Nativo en Runtime:** [`crates/ghl-runtime/src/native_db.rs`](crates/ghl-runtime/src/native_db.rs) y registro de builtins en `env.rs` y `ghl-types`.
- **Guía Técnica Exhaustiva:** [`packages/ghl_db/GUIDE.md`](packages/ghl_db/GUIDE.md) con arquitectura, recetas prácticas, manejo de NA y referencia de API.
- **Panel Cockpit Deck:** Visualizador terminal interactivo `db_cockpit(conn)` con estética Gojo/Haru (`/ᐠ˵- ⩊ -˵マ ✧ CONNECTED`).
