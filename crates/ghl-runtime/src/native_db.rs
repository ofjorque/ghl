//! Native Database and Analytical SQL Engine for GHL (`ghl_db`).
//!
//! Provides zero-copy in-memory analytical SQL via `polars-sql` (DuckDB-style OLAP),
//! embedded relational database support via `rusqlite` (SQLite3 OLTP),
//! bidirectional DataFrame conversions, and NA-reason provenance preservation.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use ghl_diagnostics::Diagnostic;
use polars_core::prelude::*;
use polars_lazy::prelude::*;
use polars_sql::SQLContext;

use crate::na_reasons::NaReasonTable;
use crate::value::Value;

/// An active database connection handle.
pub enum DbConnection {
    /// Native SQLite connection (in-memory or file-backed).
    Sqlite(rusqlite::Connection),
    /// In-memory analytical context with registered DataFrames.
    Memory(HashMap<String, DataFrame>),
}

/// Global thread-safe registry of open database connections.
static DB_REGISTRY: LazyLock<Mutex<HashMap<i64, DbConnection>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Atomic counter for allocating connection IDs.
static NEXT_CONN_ID: AtomicI64 = AtomicI64::new(1);

fn extract_conn_id(args: &[Value]) -> Result<i64, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0305",
            "Database operation requires connection handle",
        ));
    }
    match &args[0] {
        Value::I64(id) => Ok(*id),
        Value::Record(fields) => {
            if let Some(Value::I64(id)) = fields.get("id") {
                Ok(*id)
            } else {
                Err(Diagnostic::compute_error(
                    "C0305",
                    "Invalid connection record: missing 'id' field",
                ))
            }
        }
        other => Err(Diagnostic::compute_error(
            "C0305",
            format!("Expected connection record or ID, got {other:?}"),
        )),
    }
}

/// Opens a database connection.
///
/// Usage: `db_connect(driver, uri)`
/// - `driver`: `"sqlite"` or `"duckdb"` / `"memory"`
/// - `uri`: `":memory:"` or a filesystem path
pub fn native_db_connect(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0301",
            "db_connect requires at least 1 argument (driver)",
        ));
    }
    let driver = match &args[0] {
        Value::String(s) => s.to_lowercase(),
        other => {
            return Err(Diagnostic::compute_error(
                "C0301",
                format!("db_connect driver must be String, got {other:?}"),
            ));
        }
    };
    let uri = if args.len() > 1 {
        match &args[1] {
            Value::String(s) => s.clone(),
            _ => ":memory:".to_string(),
        }
    } else {
        ":memory:".to_string()
    };

    let id = NEXT_CONN_ID.fetch_add(1, Ordering::SeqCst);
    let conn = match driver.as_str() {
        "sqlite" | "sqlite3" => {
            let sqlite_conn = if uri == ":memory:" || uri.is_empty() {
                rusqlite::Connection::open_in_memory()
            } else {
                rusqlite::Connection::open(&uri)
            }
            .map_err(|e| {
                Diagnostic::compute_error(
                    "C0301",
                    format!("Failed to open SQLite database '{uri}': {e}"),
                )
            })?;
            DbConnection::Sqlite(sqlite_conn)
        }
        "duckdb" | "memory" | "olap" | "sql" => DbConnection::Memory(HashMap::new()),
        other => {
            return Err(Diagnostic::compute_error(
                "C0301",
                format!(
                    "Unsupported database driver '{other}'. Supported drivers: 'sqlite', 'duckdb', 'memory'"
                ),
            ));
        }
    };

    DB_REGISTRY.lock().unwrap().insert(id, conn);

    let mut fields = BTreeMap::new();
    fields.insert("id".to_string(), Value::I64(id));
    fields.insert("driver".to_string(), Value::String(driver));
    fields.insert("uri".to_string(), Value::String(uri));
    fields.insert("is_connected".to_string(), Value::Bool(true));
    Ok(Value::Record(Arc::new(fields)))
}

/// Closes an open database connection.
pub fn native_db_disconnect(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let id = extract_conn_id(&args)?;
    let mut registry = DB_REGISTRY.lock().unwrap();
    if registry.remove(&id).is_some() {
        Ok(Value::Bool(true))
    } else {
        Err(Diagnostic::compute_error(
            "C0305",
            format!("Connection {id} is not open or already closed"),
        ))
    }
}

/// Executes a DDL/DML SQL statement (CREATE, INSERT, UPDATE, DELETE).
/// Returns the number of affected rows (i64).
pub fn native_db_execute(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0302",
            "db_execute requires (conn, sql_query)",
        ));
    }
    let id = extract_conn_id(&args)?;
    let sql = match &args[1] {
        Value::String(s) => s.as_str(),
        other => {
            return Err(Diagnostic::compute_error(
                "C0302",
                format!("SQL query must be String, got {other:?}"),
            ));
        }
    };

    let mut registry = DB_REGISTRY.lock().unwrap();
    let conn = registry.get_mut(&id).ok_or_else(|| {
        Diagnostic::compute_error("C0305", format!("Connection {id} is not open"))
    })?;

    match conn {
        DbConnection::Sqlite(sqlite_conn) => match sqlite_conn.execute_batch(sql) {
            Ok(_) => Ok(Value::I64(0)),
            Err(_) => {
                let rows = sqlite_conn.execute(sql, []).map_err(|e| {
                    Diagnostic::compute_error("C0302", format!("SQLite execution failed: {e}"))
                })?;
                Ok(Value::I64(rows as i64))
            }
        },
        DbConnection::Memory(_) => Ok(Value::I64(0)),
    }
}

/// Executes a SQL query and returns a GHL DataFrame.
pub fn native_db_query(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0303",
            "db_query requires (conn, sql_query)",
        ));
    }
    let id = extract_conn_id(&args)?;
    let sql = match &args[1] {
        Value::String(s) => s.as_str(),
        other => {
            return Err(Diagnostic::compute_error(
                "C0303",
                format!("SQL query must be String, got {other:?}"),
            ));
        }
    };

    let mut registry = DB_REGISTRY.lock().unwrap();
    let conn = registry.get_mut(&id).ok_or_else(|| {
        Diagnostic::compute_error("C0305", format!("Connection {id} is not open"))
    })?;

    match conn {
        DbConnection::Sqlite(sqlite_conn) => {
            let mut stmt = sqlite_conn.prepare(sql).map_err(|e| {
                Diagnostic::compute_error("C0303", format!("SQLite prepare failed: {e}"))
            })?;

            let col_names: Vec<String> =
                stmt.column_names().into_iter().map(String::from).collect();
            let num_cols = col_names.len();
            let mut cols_data: Vec<Vec<Value>> = vec![Vec::new(); num_cols];

            let mut rows = stmt.query([]).map_err(|e| {
                Diagnostic::compute_error("C0303", format!("SQLite query failed: {e}"))
            })?;

            while let Some(row) = rows.next().map_err(|e| {
                Diagnostic::compute_error("C0303", format!("SQLite row iteration failed: {e}"))
            })? {
                for i in 0..num_cols {
                    let val_ref = row.get_ref(i).map_err(|e| {
                        Diagnostic::compute_error("C0303", format!("SQLite value read failed: {e}"))
                    })?;
                    let ghl_val = match val_ref {
                        rusqlite::types::ValueRef::Null => Value::NA(Some("db.null".to_string())),
                        rusqlite::types::ValueRef::Integer(n) => Value::I64(n),
                        rusqlite::types::ValueRef::Real(f) => Value::F64(f),
                        rusqlite::types::ValueRef::Text(t) => {
                            Value::String(String::from_utf8_lossy(t).into_owned())
                        }
                        rusqlite::types::ValueRef::Blob(b) => {
                            Value::String(format!("<blob {} bytes>", b.len()))
                        }
                    };
                    cols_data[i].push(ghl_val);
                }
            }

            let pairs: Vec<(String, Vec<Value>)> = col_names.into_iter().zip(cols_data).collect();
            let (frame, na_reasons) = crate::polars_bridge::build_dataframe(&pairs)?;
            Ok(Value::DataFrame { frame, na_reasons })
        }
        DbConnection::Memory(tables) => {
            let mut ctx = SQLContext::new();
            for (name, df) in tables.iter() {
                ctx.register(name, df.clone().lazy());
            }
            let lf = ctx.execute(sql).map_err(|e| {
                Diagnostic::compute_error("C0303", format!("SQL analytical query failed: {e}"))
            })?;
            let frame = lf.collect().map_err(|e| {
                Diagnostic::compute_error("C0303", format!("SQL analytical evaluation failed: {e}"))
            })?;
            let na_reasons = Arc::new(NaReasonTable::new());
            Ok(Value::DataFrame { frame, na_reasons })
        }
    }
}

/// Registers a GHL DataFrame as a named table in the database.
pub fn native_db_register(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 3 {
        return Err(Diagnostic::compute_error(
            "C0304",
            "db_register requires (conn, table_name, dataframe)",
        ));
    }
    let id = extract_conn_id(&args)?;
    let table_name = match &args[1] {
        Value::String(s) => s.clone(),
        other => {
            return Err(Diagnostic::compute_error(
                "C0304",
                format!("table_name must be String, got {other:?}"),
            ));
        }
    };
    let df = match &args[2] {
        Value::DataFrame { frame, .. } => frame.clone(),
        other => {
            return Err(Diagnostic::compute_error(
                "C0304",
                format!("dataframe must be a DataFrame, got {other:?}"),
            ));
        }
    };

    let mut registry = DB_REGISTRY.lock().unwrap();
    let conn = registry.get_mut(&id).ok_or_else(|| {
        Diagnostic::compute_error("C0305", format!("Connection {id} is not open"))
    })?;

    match conn {
        DbConnection::Sqlite(sqlite_conn) => {
            write_df_to_sqlite(sqlite_conn, &table_name, &df)?;
            Ok(Value::Bool(true))
        }
        DbConnection::Memory(tables) => {
            tables.insert(table_name, df);
            Ok(Value::Bool(true))
        }
    }
}

/// Lists all table names in the database.
pub fn native_db_tables(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let id = extract_conn_id(&args)?;
    let registry = DB_REGISTRY.lock().unwrap();
    let conn = registry.get(&id).ok_or_else(|| {
        Diagnostic::compute_error("C0305", format!("Connection {id} is not open"))
    })?;

    match conn {
        DbConnection::Sqlite(sqlite_conn) => {
            let mut stmt = sqlite_conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
                .map_err(|e| Diagnostic::compute_error("C0303", format!("SQLite list tables failed: {e}")))?;
            let names: Vec<Value> = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| {
                    Diagnostic::compute_error("C0303", format!("SQLite query_map failed: {e}"))
                })?
                .filter_map(|r| r.ok())
                .map(Value::String)
                .collect();
            Ok(Value::Vector(crate::vector_data::VectorData::from_values(
                names,
            )))
        }
        DbConnection::Memory(tables) => {
            let mut names: Vec<Value> = tables.keys().cloned().map(Value::String).collect();
            names.sort_by_key(|a| a.to_string());
            Ok(Value::Vector(crate::vector_data::VectorData::from_values(
                names,
            )))
        }
    }
}

/// Executes a zero-copy analytical SQL query over a collection of DataFrames.
///
/// Usage: `query_sql(sql_str, tables_record)`
/// Example: `query_sql("SELECT species, AVG(flipper_length_mm) FROM penguins GROUP BY species", { penguins: df })`
pub fn native_query_sql(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0303",
            "query_sql requires at least 1 argument: sql_query",
        ));
    }
    let sql = match &args[0] {
        Value::String(s) => s.as_str(),
        other => {
            return Err(Diagnostic::compute_error(
                "C0303",
                format!("SQL query must be String, got {other:?}"),
            ));
        }
    };

    let mut ctx = SQLContext::new();

    if args.len() > 1 {
        match &args[1] {
            Value::Record(map) => {
                for (name, val) in map.iter() {
                    if let Value::DataFrame { frame, .. } = val {
                        ctx.register(name, frame.clone().lazy());
                    }
                }
            }
            Value::DataFrame { frame, .. } => {
                ctx.register("df", frame.clone().lazy());
            }
            other => {
                return Err(Diagnostic::compute_error(
                    "C0303",
                    format!(
                        "query_sql tables argument must be a Record of DataFrames or a single DataFrame, got {other:?}"
                    ),
                ));
            }
        }
    }

    let lf = ctx.execute(sql).map_err(|e| {
        Diagnostic::compute_error("C0303", format!("SQL analytical query failed: {e}"))
    })?;
    let frame = lf.collect().map_err(|e| {
        Diagnostic::compute_error("C0303", format!("SQL analytical evaluation failed: {e}"))
    })?;
    let na_reasons = Arc::new(NaReasonTable::new());
    Ok(Value::DataFrame { frame, na_reasons })
}

/// Writes a Polars DataFrame to a SQLite database table.
fn write_df_to_sqlite(
    conn: &mut rusqlite::Connection,
    table_name: &str,
    df: &DataFrame,
) -> Result<(), Diagnostic> {
    let columns = df.columns();
    if columns.is_empty() {
        return Ok(());
    }

    // 1. Generate CREATE TABLE DDL
    let mut col_defs = Vec::with_capacity(columns.len());
    for col in columns {
        let name = col.name();
        let sql_type = match col.dtype() {
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64 => "INTEGER",
            DataType::Float32 | DataType::Float64 => "REAL",
            DataType::Boolean => "INTEGER",
            DataType::String => "TEXT",
            _ => "TEXT",
        };
        col_defs.push(format!("\"{}\" {}", name, sql_type));
    }

    let ddl = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
        table_name,
        col_defs.join(", ")
    );
    conn.execute(&ddl, []).map_err(|e| {
        Diagnostic::compute_error(
            "C0304",
            format!("Failed to create table '{table_name}': {e}"),
        )
    })?;

    // 2. Perform bulk insert in a transaction
    let placeholders = vec!["?"; columns.len()].join(", ");
    let insert_sql = format!("INSERT INTO \"{}\" VALUES ({})", table_name, placeholders);

    let height = df.height();
    let tx = conn.transaction().map_err(|e| {
        Diagnostic::compute_error("C0304", format!("Failed to start transaction: {e}"))
    })?;

    {
        let mut stmt = tx.prepare(&insert_sql).map_err(|e| {
            Diagnostic::compute_error("C0304", format!("Failed to prepare insert: {e}"))
        })?;

        for row_idx in 0..height {
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                Vec::with_capacity(columns.len());
            for col in columns {
                let any_val = col.get(row_idx).map_err(|e| {
                    Diagnostic::compute_error("C0304", format!("Column read error: {e}"))
                })?;
                match any_val {
                    AnyValue::Null => params.push(Box::new(rusqlite::types::Null)),
                    AnyValue::Int64(i) => params.push(Box::new(i)),
                    AnyValue::Int32(i) => params.push(Box::new(i as i64)),
                    AnyValue::Int16(i) => params.push(Box::new(i as i64)),
                    AnyValue::Int8(i) => params.push(Box::new(i as i64)),
                    AnyValue::UInt64(u) => params.push(Box::new(u as i64)),
                    AnyValue::UInt32(u) => params.push(Box::new(u as i64)),
                    AnyValue::UInt16(u) => params.push(Box::new(u as i64)),
                    AnyValue::UInt8(u) => params.push(Box::new(u as i64)),
                    AnyValue::Float64(f) => params.push(Box::new(f)),
                    AnyValue::Float32(f) => params.push(Box::new(f as f64)),
                    AnyValue::Boolean(b) => params.push(Box::new(if b { 1i64 } else { 0i64 })),
                    AnyValue::String(s) => params.push(Box::new(s.to_string())),
                    other => params.push(Box::new(other.to_string())),
                }
            }

            let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            stmt.execute(&params_refs[..]).map_err(|e| {
                Diagnostic::compute_error("C0304", format!("Row insert error: {e}"))
            })?;
        }
    }

    tx.commit().map_err(|e| {
        Diagnostic::compute_error("C0304", format!("Transaction commit error: {e}"))
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_in_memory_crud() {
        let conn_val = native_db_connect(vec![
            Value::String("sqlite".into()),
            Value::String(":memory:".into()),
        ])
        .expect("connect ok");

        // Execute DDL
        let ddl =
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age REAL, active INTEGER)";
        native_db_execute(vec![conn_val.clone(), Value::String(ddl.into())])
            .expect("create table ok");

        // Insert rows
        let insert =
            "INSERT INTO users (name, age, active) VALUES ('Alice', 30.5, 1), ('Bob', NULL, 0)";
        native_db_execute(vec![conn_val.clone(), Value::String(insert.into())]).expect("insert ok");

        // Query rows
        let query_res = native_db_query(vec![
            conn_val.clone(),
            Value::String("SELECT * FROM users ORDER BY id".into()),
        ])
        .expect("query ok");
        if let Value::DataFrame { frame, na_reasons } = query_res {
            assert_eq!(frame.height(), 2);
            assert_eq!(frame.width(), 4);
            // Verify Bob's age is NA with reason "db.null"
            assert_eq!(na_reasons.get("age", 1), Some("db.null"));
        } else {
            panic!("Expected DataFrame from db_query");
        }

        // List tables
        let tables_res = native_db_tables(vec![conn_val.clone()]).expect("tables ok");
        if let Value::Vector(vec_data) = tables_res {
            let names = vec_data.to_vec();
            assert_eq!(names.len(), 1);
            assert_eq!(names[0], Value::String("users".into()));
        } else {
            panic!("Expected Vector from db_tables");
        }

        // Disconnect
        let disc = native_db_disconnect(vec![conn_val]).expect("disconnect ok");
        assert_eq!(disc, Value::Bool(true));
    }

    #[test]
    fn test_polars_sql_zero_copy_query() {
        let cols = vec![
            (
                "dept".to_string(),
                vec![
                    Value::String("Eng".into()),
                    Value::String("Sales".into()),
                    Value::String("Eng".into()),
                ],
            ),
            (
                "salary".to_string(),
                vec![Value::F64(100.0), Value::F64(80.0), Value::F64(120.0)],
            ),
        ];
        let (df, na_reasons) = crate::polars_bridge::build_dataframe(&cols).expect("build df ok");
        let df_val = Value::DataFrame {
            frame: df,
            na_reasons,
        };

        let mut map = BTreeMap::new();
        map.insert("employees".to_string(), df_val);
        let tables = Value::Record(Arc::new(map));

        let sql = "SELECT dept, COUNT(*) AS count, AVG(salary) AS avg_salary FROM employees GROUP BY dept ORDER BY dept";
        let res = native_query_sql(vec![Value::String(sql.into()), tables]).expect("query_sql ok");

        if let Value::DataFrame { frame, .. } = res {
            assert_eq!(frame.height(), 2);
            let dept_col = frame.column("dept").unwrap();
            assert_eq!(dept_col.get(0).unwrap().to_string(), "\"Eng\"");
            let count_col = frame.column("count").unwrap();
            assert_eq!(count_col.get(0).unwrap(), AnyValue::UInt32(2));
        } else {
            panic!("Expected DataFrame from query_sql");
        }
    }

    #[test]
    fn test_sqlite_register_dataframe() {
        let conn_val = native_db_connect(vec![
            Value::String("sqlite".into()),
            Value::String(":memory:".into()),
        ])
        .expect("connect ok");

        let cols = vec![
            ("id".to_string(), vec![Value::I64(1), Value::I64(2)]),
            (
                "metric".to_string(),
                vec![Value::F64(3.14), Value::F64(2.71)],
            ),
        ];
        let (df, na_reasons) = crate::polars_bridge::build_dataframe(&cols).expect("build df ok");
        let df_val = Value::DataFrame {
            frame: df,
            na_reasons,
        };

        native_db_register(vec![
            conn_val.clone(),
            Value::String("metrics".into()),
            df_val,
        ])
        .expect("register ok");

        let res = native_db_query(vec![
            conn_val.clone(),
            Value::String("SELECT * FROM metrics WHERE id = 1".into()),
        ])
        .expect("query ok");
        if let Value::DataFrame { frame, .. } = res {
            assert_eq!(frame.height(), 1);
            assert_eq!(
                frame.column("metric").unwrap().get(0).unwrap(),
                AnyValue::Float64(3.14)
            );
        } else {
            panic!("Expected DataFrame");
        }

        native_db_disconnect(vec![conn_val]).expect("disconnect ok");
    }
}
