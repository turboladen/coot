//! `sys.*` schema introspection — the object tree's data layer (beads rqb.1 +
//! rqb.3). Enumerates databases / tables / views / columns and derives each
//! column's **canonical** type string, plus an in-memory [`SchemaCache`] with an
//! invalidate seam for the Refresh action (rqb.5).
//!
//! Driver stays behind `core` (`CLAUDE.md`): every query goes through
//! [`crate::executor::run`] and every public item returns core-owned serde
//! types — no `mssql_client` type appears here. Pure Rust, headless-testable:
//! the SQL-execution half is split from a pure parsing half (build a
//! [`QueryResult`] by hand, assert the mapping) exactly as `executor` tests its
//! `cell_from_sql_value`.
//!
//! **Two type sources** (`PLAN.md` §7): this module's [`format_sql_type`] builds
//! the tree's *canonical* type from `sys.types` + length metadata. That is a
//! DIFFERENT source from the runner's [`crate::friendly_type_name`] (wire
//! tokens) — they are intentionally not shared.
//!
//! `run` takes no bind params (Phase 3), so `list_columns`'s schema/table are
//! embedded as escaped SQL string literals via [`quote_literal`] — they are
//! *values* compared against `sys.*.name`, injection-safe once single-quotes are
//! doubled.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::connection::{ConnectionConfig, ConnectionId, SecretStore};
use crate::context::ExecutionContext;
use crate::error::{CoreError, Result};
use crate::result::{CellValue, QueryResult};
use crate::session::SessionCache;

// ---------------------------------------------------------------------------
// §1. Model types (serde, camelCase, driver-free)
// ---------------------------------------------------------------------------

/// A database on the server. `state_desc` (`"ONLINE"`, `"OFFLINE"`, …) lets the
/// tree grey non-`ONLINE` databases (rqb.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub database_id: i32,
    pub state_desc: String,
}

/// A user table (schema-qualified).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub schema: String,
    pub name: String,
}

/// A view (schema-qualified). Structurally identical to [`TableInfo`] but kept
/// distinct — clearer tree code, and the two may diverge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewInfo {
    pub schema: String,
    pub name: String,
}

/// A column of a table. `data_type` is the canonical type string built by
/// [`format_sql_type`] (e.g. `"nvarchar(50)"`, `"decimal(19,4)"`, `"int"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
    pub ordinal: i32,
}

// ---------------------------------------------------------------------------
// §2. Introspection SQL (verified standard catalog views)
// ---------------------------------------------------------------------------

const SQL_LIST_DATABASES: &str =
    "SELECT name, database_id, state_desc FROM sys.databases ORDER BY name;";

const SQL_LIST_TABLES: &str = "SELECT s.name AS schema_name, t.name AS table_name \
     FROM sys.tables AS t \
     JOIN sys.schemas AS s ON s.schema_id = t.schema_id \
     ORDER BY s.name, t.name;";

const SQL_LIST_VIEWS: &str = "SELECT s.name AS schema_name, v.name AS view_name \
     FROM sys.views AS v \
     JOIN sys.schemas AS s ON s.schema_id = v.schema_id \
     ORDER BY s.name, v.name;";

/// The rqb.3 column query. Schema/table are embedded as escaped `N'…'` literals
/// (see [`quote_literal`]) because `run` has no bind params. PK/FK come from
/// derived tables that yield at most one row per column (PK: one PK index per
/// table; FK: `DISTINCT` collapses a column that appears in several FKs), so the
/// `LEFT JOIN`s never multiply the column rows.
fn list_columns_sql(schema: &str, table: &str) -> String {
    format!(
        "SELECT \
             c.name AS column_name, \
             ty.name AS type_name, \
             c.max_length AS max_length, \
             c.precision AS precision, \
             c.scale AS scale, \
             c.is_nullable AS is_nullable, \
             c.column_id AS column_id, \
             CAST(CASE WHEN pk.column_id IS NOT NULL THEN 1 ELSE 0 END AS bit) AS is_primary_key, \
             CAST(CASE WHEN fk.parent_column_id IS NOT NULL THEN 1 ELSE 0 END AS bit) AS is_foreign_key \
         FROM sys.columns AS c \
         JOIN sys.types AS ty ON ty.user_type_id = c.user_type_id \
         JOIN sys.objects AS o ON o.object_id = c.object_id \
         JOIN sys.schemas AS s ON s.schema_id = o.schema_id \
         LEFT JOIN ( \
             SELECT ic.object_id, ic.column_id \
             FROM sys.index_columns AS ic \
             JOIN sys.indexes AS i ON i.object_id = ic.object_id AND i.index_id = ic.index_id \
             WHERE i.is_primary_key = 1 \
         ) AS pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id \
         LEFT JOIN ( \
             SELECT DISTINCT fkc.parent_object_id, fkc.parent_column_id \
             FROM sys.foreign_key_columns AS fkc \
         ) AS fk ON fk.parent_object_id = c.object_id AND fk.parent_column_id = c.column_id \
         WHERE s.name = {schema} AND o.name = {table} \
         ORDER BY c.column_id;",
        schema = quote_literal(schema),
        table = quote_literal(table),
    )
}

// ---------------------------------------------------------------------------
// §3. Canonical type formatting (pure)
// ---------------------------------------------------------------------------

/// Build the canonical type string from `sys.types` metadata. `max_length` is
/// `sys.columns.max_length` (BYTES; `-1` = MAX); `precision`/`scale` are the
/// column's precision/scale. Unknown type names fall through to the bare name
/// (never panics, always shows *something*).
///
/// This is a **different type source** from [`crate::friendly_type_name`] (wire
/// tokens) and deliberately does not reuse it (`PLAN.md` §7).
fn format_sql_type(type_name: &str, max_length: i16, precision: u8, scale: u8) -> String {
    // `sys.types` returns lowercase system type names; lowercase defensively.
    let t = type_name.to_ascii_lowercase();
    match t.as_str() {
        // Unicode char types: max_length is bytes → halve for the char count.
        "nvarchar" | "nchar" => {
            if max_length == -1 {
                format!("{t}(MAX)")
            } else {
                format!("{t}({})", max_length / 2)
            }
        }
        // Byte-length char/binary types: bytes == units, no halving.
        "varchar" | "char" | "binary" | "varbinary" => {
            if max_length == -1 {
                format!("{t}(MAX)")
            } else {
                format!("{t}({max_length})")
            }
        }
        // Exact numeric: (precision, scale).
        "decimal" | "numeric" => format!("{t}({precision},{scale})"),
        // Scale-only temporal types.
        "datetime2" | "time" | "datetimeoffset" => format!("{t}({scale})"),
        // Everything else (int/bigint/bit/money/float/date/uniqueidentifier/…)
        // and any unknown type: the bare name. `float` shown bare (mantissa
        // precision ignored) by design.
        _ => t,
    }
}

// ---------------------------------------------------------------------------
// §5. Identifier safety
// ---------------------------------------------------------------------------

/// A SQL string literal: single-quotes doubled, `N`-prefixed (`sys.*.name` is
/// `sysname`/nvarchar). Injection-safe — a doubled `'` closes nothing. Used to
/// embed schema/table *values* into `list_columns`'s `WHERE` (not identifiers;
/// they are compared against `sys.*.name`, so literal quoting, not `[bracket]`).
fn quote_literal(s: &str) -> String {
    format!("N'{}'", s.replace('\'', "''"))
}

// ---------------------------------------------------------------------------
// §4. Row → model parsing (headless-testable seam)
// ---------------------------------------------------------------------------

/// Read a `CellValue::Text` at ordinal `i`. Missing index or wrong variant is a
/// result-shape problem → [`CoreError::Query`] (reused, not a new variant).
fn cell_text(row: &[CellValue], i: usize) -> Result<String> {
    match row.get(i) {
        Some(CellValue::Text(s)) => Ok(s.clone()),
        Some(other) => Err(shape_err(i, "Text", other)),
        None => Err(missing_err(i, row.len())),
    }
}

/// Read a `CellValue::Int` at ordinal `i` (int/smallint/tinyint all widen here).
fn cell_int(row: &[CellValue], i: usize) -> Result<i64> {
    match row.get(i) {
        Some(CellValue::Int(n)) => Ok(*n),
        Some(other) => Err(shape_err(i, "Int", other)),
        None => Err(missing_err(i, row.len())),
    }
}

/// Read a `CellValue::Bool` at ordinal `i` (a `bit` column).
fn cell_bool(row: &[CellValue], i: usize) -> Result<bool> {
    match row.get(i) {
        Some(CellValue::Bool(b)) => Ok(*b),
        Some(other) => Err(shape_err(i, "Bool", other)),
        None => Err(missing_err(i, row.len())),
    }
}

fn shape_err(i: usize, expected: &str, got: &CellValue) -> CoreError {
    CoreError::Query(format!(
        "schema introspection: column {i} expected {expected}, got {got:?}"
    ))
}

fn missing_err(i: usize, len: usize) -> CoreError {
    CoreError::Query(format!(
        "schema introspection: column {i} missing (row has {len} cells)"
    ))
}

fn parse_databases(qr: &QueryResult) -> Result<Vec<DatabaseInfo>> {
    qr.rows
        .iter()
        .map(|row| {
            Ok(DatabaseInfo {
                name: cell_text(row, 0)?,
                database_id: cell_int(row, 1)? as i32,
                state_desc: cell_text(row, 2)?,
            })
        })
        .collect()
}

fn parse_tables(qr: &QueryResult) -> Result<Vec<TableInfo>> {
    qr.rows
        .iter()
        .map(|row| {
            Ok(TableInfo {
                schema: cell_text(row, 0)?,
                name: cell_text(row, 1)?,
            })
        })
        .collect()
}

fn parse_views(qr: &QueryResult) -> Result<Vec<ViewInfo>> {
    qr.rows
        .iter()
        .map(|row| {
            Ok(ViewInfo {
                schema: cell_text(row, 0)?,
                name: cell_text(row, 1)?,
            })
        })
        .collect()
}

fn parse_columns(qr: &QueryResult) -> Result<Vec<ColumnInfo>> {
    qr.rows
        .iter()
        .map(|row| {
            let type_name = cell_text(row, 1)?;
            let max_length = cell_int(row, 2)?;
            let precision = cell_int(row, 3)?;
            let scale = cell_int(row, 4)?;
            Ok(ColumnInfo {
                name: cell_text(row, 0)?,
                data_type: format_sql_type(
                    &type_name,
                    max_length as i16,
                    precision as u8,
                    scale as u8,
                ),
                nullable: cell_bool(row, 5)?,
                ordinal: cell_int(row, 6)? as i32,
                is_primary_key: cell_bool(row, 7)?,
                is_foreign_key: cell_bool(row, 8)?,
            })
        })
        .collect()
}

/// The first result set, or a shape error if the batch produced none.
fn first_result(results: Vec<QueryResult>) -> Result<QueryResult> {
    results.into_iter().next().ok_or_else(|| {
        CoreError::Query("schema introspection: query returned no result set".into())
    })
}

// ---------------------------------------------------------------------------
// Async fetchers (thin run + parse)
// ---------------------------------------------------------------------------

/// Enumerate the server's databases. Runs on the connection default (no `USE`).
pub async fn list_databases(
    sessions: &SessionCache,
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
) -> Result<Vec<DatabaseInfo>> {
    let ctx = ExecutionContext::new(cfg.id.clone());
    let results = sessions.run(cfg, store, &ctx, SQL_LIST_DATABASES).await?;
    parse_databases(&first_result(results)?)
}

/// Enumerate `db`'s user tables. Runs inside `db` (`USE [db]`).
pub async fn list_tables(
    sessions: &SessionCache,
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    db: &str,
) -> Result<Vec<TableInfo>> {
    let ctx = ExecutionContext::new(cfg.id.clone()).with_database(db);
    let results = sessions.run(cfg, store, &ctx, SQL_LIST_TABLES).await?;
    parse_tables(&first_result(results)?)
}

/// Enumerate `db`'s views. Runs inside `db` (`USE [db]`).
pub async fn list_views(
    sessions: &SessionCache,
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    db: &str,
) -> Result<Vec<ViewInfo>> {
    let ctx = ExecutionContext::new(cfg.id.clone()).with_database(db);
    let results = sessions.run(cfg, store, &ctx, SQL_LIST_VIEWS).await?;
    parse_views(&first_result(results)?)
}

/// Enumerate the columns of `schema.table` in `db`. A nonexistent schema/table
/// yields an empty result set → `Ok(vec![])`, NOT an error.
pub async fn list_columns(
    sessions: &SessionCache,
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    db: &str,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnInfo>> {
    let ctx = ExecutionContext::new(cfg.id.clone()).with_database(db);
    let sql = list_columns_sql(schema, table);
    let results = sessions.run(cfg, store, &ctx, &sql).await?;
    parse_columns(&first_result(results)?)
}

// ---------------------------------------------------------------------------
// §6. The cache
// ---------------------------------------------------------------------------

/// In-memory schema cache with an invalidate seam (rqb.5 Refresh). Interior
/// mutability via `std::sync::Mutex`; keyed by [`ConnectionId`] (+ the db /
/// schema / table tuple). Fetch-or-return-cached; a first-call race may
/// double-fetch — acceptable for a single-user tool.
/// Cache key for a database-scoped list (tables/views): connection + database.
type DbKey = (ConnectionId, String);
/// Cache key for a column list: connection + database + schema + table.
type ColumnKey = (ConnectionId, String, String, String);

#[derive(Default)]
pub struct SchemaCache {
    databases: Mutex<HashMap<ConnectionId, Vec<DatabaseInfo>>>,
    tables: Mutex<HashMap<DbKey, Vec<TableInfo>>>,
    views: Mutex<HashMap<DbKey, Vec<ViewInfo>>>,
    columns: Mutex<HashMap<ColumnKey, Vec<ColumnInfo>>>,
    /// Reused live connections for introspection (billz-lpb) — one login per
    /// connection amortized across expands, instead of one per `sys.*` query.
    sessions: SessionCache,
}

/// Return the cached value for `key`, or run `fetch` and cache a successful
/// result. **Never holds the `Mutex` guard across the `.await`** — the
/// edition-2024 `if let` scrutinee temporary drops at the end of the `if`, so
/// the lookup guard is released before `fetch().await`. An `Err` is not cached
/// (the next call retries).
async fn get_or_fetch<K, V, Fut>(
    map: &Mutex<HashMap<K, V>>,
    key: K,
    fetch: impl FnOnce() -> Fut,
) -> Result<V>
where
    K: Eq + Hash + Clone,
    V: Clone,
    Fut: Future<Output = Result<V>>,
{
    if let Some(v) = map.lock().unwrap().get(&key) {
        return Ok(v.clone()); // guard dropped at the end of this `if`
    }
    let v = fetch().await; // no guard held across the await
    if let Ok(val) = &v {
        map.lock().unwrap().insert(key, val.clone());
    }
    v
}

impl SchemaCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Databases, fetched once per connection and cached.
    pub async fn databases(
        &self,
        cfg: &ConnectionConfig,
        store: &dyn SecretStore,
    ) -> Result<Vec<DatabaseInfo>> {
        get_or_fetch(&self.databases, cfg.id.clone(), || {
            list_databases(&self.sessions, cfg, store)
        })
        .await
    }

    /// Tables of `db`, fetched once per (connection, db) and cached.
    pub async fn tables(
        &self,
        cfg: &ConnectionConfig,
        store: &dyn SecretStore,
        db: &str,
    ) -> Result<Vec<TableInfo>> {
        get_or_fetch(&self.tables, (cfg.id.clone(), db.to_string()), || {
            list_tables(&self.sessions, cfg, store, db)
        })
        .await
    }

    /// Views of `db`, fetched once per (connection, db) and cached.
    pub async fn views(
        &self,
        cfg: &ConnectionConfig,
        store: &dyn SecretStore,
        db: &str,
    ) -> Result<Vec<ViewInfo>> {
        get_or_fetch(&self.views, (cfg.id.clone(), db.to_string()), || {
            list_views(&self.sessions, cfg, store, db)
        })
        .await
    }

    /// Columns of `schema.table` in `db`, fetched once per key and cached.
    pub async fn columns(
        &self,
        cfg: &ConnectionConfig,
        store: &dyn SecretStore,
        db: &str,
        schema: &str,
        table: &str,
    ) -> Result<Vec<ColumnInfo>> {
        let key = (
            cfg.id.clone(),
            db.to_string(),
            schema.to_string(),
            table.to_string(),
        );
        get_or_fetch(&self.columns, key, || {
            list_columns(&self.sessions, cfg, store, db, schema, table)
        })
        .await
    }

    /// Clear every cached map — the Refresh seam (rqb.5).
    pub fn invalidate(&self) {
        self.databases.lock().unwrap().clear();
        self.tables.lock().unwrap().clear();
        self.views.lock().unwrap().clear();
        self.columns.lock().unwrap().clear();
    }

    /// Drop every entry belonging to one connection (e.g. on disconnect).
    pub fn invalidate_connection(&self, id: &ConnectionId) {
        self.databases.lock().unwrap().retain(|k, _| k != id);
        self.tables.lock().unwrap().retain(|k, _| &k.0 != id);
        self.views.lock().unwrap().retain(|k, _| &k.0 != id);
        self.columns.lock().unwrap().retain(|k, _| &k.0 != id);
    }

    /// Drop one connection's cached data AND its live session client. Use on
    /// connection edit/delete (creds/server may have changed) — distinct from
    /// [`Self::invalidate_connection`] (the Refresh path), which keeps the warm
    /// client so a Refresh re-queries `sys.*` without re-logging in.
    pub fn forget_connection(&self, id: &ConnectionId) {
        self.invalidate_connection(id);
        self.sessions.evict(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::run; // live tests use executor::run for DDL setup/teardown
    use std::sync::atomic::{AtomicUsize, Ordering};

    // ---- §3. format_sql_type: exhaustive matrix ----

    #[test]
    fn format_sql_type_matrix() {
        // Unicode char: max_length (bytes) halved; -1 → MAX.
        assert_eq!(format_sql_type("nvarchar", 100, 0, 0), "nvarchar(50)");
        assert_eq!(format_sql_type("nvarchar", -1, 0, 0), "nvarchar(MAX)");
        assert_eq!(format_sql_type("nchar", 20, 0, 0), "nchar(10)");
        // Byte char/binary: no halving; -1 → MAX.
        assert_eq!(format_sql_type("varchar", 50, 0, 0), "varchar(50)");
        assert_eq!(format_sql_type("varchar", -1, 0, 0), "varchar(MAX)");
        assert_eq!(format_sql_type("char", 10, 0, 0), "char(10)");
        assert_eq!(format_sql_type("binary", 16, 0, 0), "binary(16)");
        assert_eq!(format_sql_type("varbinary", 8, 0, 0), "varbinary(8)");
        assert_eq!(format_sql_type("varbinary", -1, 0, 0), "varbinary(MAX)");
        // Exact numeric: (precision, scale).
        assert_eq!(format_sql_type("decimal", 0, 19, 4), "decimal(19,4)");
        assert_eq!(format_sql_type("numeric", 0, 10, 0), "numeric(10,0)");
        // Scale-only temporal.
        assert_eq!(format_sql_type("datetime2", 0, 0, 7), "datetime2(7)");
        assert_eq!(format_sql_type("time", 0, 0, 3), "time(3)");
        assert_eq!(
            format_sql_type("datetimeoffset", 0, 0, 7),
            "datetimeoffset(7)"
        );
        // Bare types.
        assert_eq!(format_sql_type("int", 4, 10, 0), "int");
        assert_eq!(format_sql_type("bigint", 8, 19, 0), "bigint");
        assert_eq!(format_sql_type("bit", 1, 0, 0), "bit");
        assert_eq!(format_sql_type("money", 8, 19, 4), "money");
        assert_eq!(
            format_sql_type("uniqueidentifier", 16, 0, 0),
            "uniqueidentifier"
        );
        assert_eq!(format_sql_type("date", 3, 0, 0), "date");
        assert_eq!(format_sql_type("datetime", 8, 0, 0), "datetime");
        // float shown bare (mantissa precision ignored).
        assert_eq!(format_sql_type("float", 8, 53, 0), "float");
        // Unknown → bare, never panics.
        assert_eq!(format_sql_type("geography", -1, 0, 0), "geography");
        // Uppercase input lowercased defensively.
        assert_eq!(format_sql_type("NVarChar", 100, 0, 0), "nvarchar(50)");
    }

    // ---- §5. quote_literal ----

    #[test]
    fn quote_literal_wraps_and_doubles_quotes() {
        assert_eq!(quote_literal("dbo"), "N'dbo'");
        assert_eq!(quote_literal("O'Brien"), "N'O''Brien'");
        assert_eq!(quote_literal(""), "N''");
    }

    #[test]
    fn list_columns_sql_embeds_escaped_literals() {
        let sql = list_columns_sql("dbo", "O'Brien");
        assert!(sql.contains("s.name = N'dbo'"));
        assert!(sql.contains("o.name = N'O''Brien'"));
    }

    // ---- §4. parsers over hand-built QueryResults ----

    fn qr(rows: Vec<Vec<CellValue>>) -> QueryResult {
        QueryResult {
            columns: vec![],
            rows,
            rows_affected: None,
        }
    }

    #[test]
    fn parse_databases_reads_all_three_columns() {
        let result = qr(vec![
            vec![
                CellValue::Text("master".into()),
                CellValue::Int(1),
                CellValue::Text("ONLINE".into()),
            ],
            vec![
                CellValue::Text("appdb".into()),
                CellValue::Int(7),
                CellValue::Text("OFFLINE".into()),
            ],
        ]);
        let got = parse_databases(&result).unwrap();
        assert_eq!(
            got,
            vec![
                DatabaseInfo {
                    name: "master".into(),
                    database_id: 1,
                    state_desc: "ONLINE".into(),
                },
                DatabaseInfo {
                    name: "appdb".into(),
                    database_id: 7,
                    state_desc: "OFFLINE".into(),
                },
            ]
        );
    }

    #[test]
    fn parse_tables_and_views_read_schema_and_name() {
        let t = qr(vec![vec![
            CellValue::Text("dbo".into()),
            CellValue::Text("Orders".into()),
        ]]);
        assert_eq!(
            parse_tables(&t).unwrap(),
            vec![TableInfo {
                schema: "dbo".into(),
                name: "Orders".into()
            }]
        );
        assert_eq!(
            parse_views(&t).unwrap(),
            vec![ViewInfo {
                schema: "dbo".into(),
                name: "Orders".into()
            }]
        );
    }

    #[test]
    fn parse_columns_builds_full_struct_incl_type_and_pk_fk() {
        // Row 0: nvarchar(50) PK, NOT NULL. Row 1: decimal(19,4) FK, nullable.
        let result = qr(vec![
            vec![
                CellValue::Text("id".into()),       // name
                CellValue::Text("nvarchar".into()), // type_name
                CellValue::Int(100),                // max_length (bytes → 50 chars)
                CellValue::Int(0),                  // precision
                CellValue::Int(0),                  // scale
                CellValue::Bool(false),             // is_nullable
                CellValue::Int(1),                  // column_id
                CellValue::Bool(true),              // is_primary_key
                CellValue::Bool(false),             // is_foreign_key
            ],
            vec![
                CellValue::Text("amount".into()),
                CellValue::Text("decimal".into()),
                CellValue::Int(9), // max_length (irrelevant for decimal)
                CellValue::Int(19),
                CellValue::Int(4),
                CellValue::Bool(true),
                CellValue::Int(2),
                CellValue::Bool(false),
                CellValue::Bool(true),
            ],
        ]);
        let got = parse_columns(&result).unwrap();
        assert_eq!(
            got,
            vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "nvarchar(50)".into(),
                    nullable: false,
                    is_primary_key: true,
                    is_foreign_key: false,
                    ordinal: 1,
                },
                ColumnInfo {
                    name: "amount".into(),
                    data_type: "decimal(19,4)".into(),
                    nullable: true,
                    is_primary_key: false,
                    is_foreign_key: true,
                    ordinal: 2,
                },
            ]
        );
    }

    #[test]
    fn parse_columns_wrong_variant_is_query_error() {
        // Int where Text (column_name) is expected.
        let result = qr(vec![vec![
            CellValue::Int(0),
            CellValue::Text("int".into()),
            CellValue::Int(4),
            CellValue::Int(0),
            CellValue::Int(0),
            CellValue::Bool(false),
            CellValue::Int(1),
            CellValue::Bool(false),
            CellValue::Bool(false),
        ]]);
        let err = parse_columns(&result).unwrap_err();
        assert!(matches!(err, CoreError::Query(_)));
        assert!(err.to_string().contains("schema introspection"));
    }

    #[test]
    fn parse_columns_short_row_is_query_error() {
        // Only 3 cells — reading column_id (ordinal 6) must fail cleanly.
        let result = qr(vec![vec![
            CellValue::Text("id".into()),
            CellValue::Text("int".into()),
            CellValue::Int(4),
        ]]);
        assert!(matches!(
            parse_columns(&result).unwrap_err(),
            CoreError::Query(_)
        ));
    }

    // Note C — an empty result set is "no columns", not a failure.
    #[test]
    fn parse_columns_empty_result_is_ok_empty() {
        assert_eq!(parse_columns(&qr(vec![])).unwrap(), vec![]);
    }

    #[test]
    fn parse_databases_empty_result_is_ok_empty() {
        assert_eq!(parse_databases(&qr(vec![])).unwrap(), vec![]);
    }

    // ---- §6. cache: get_or_fetch fetch-once / invalidate / Err-not-cached ----

    #[tokio::test]
    async fn get_or_fetch_runs_closure_once_per_key() {
        let map: Mutex<HashMap<String, i32>> = Mutex::new(HashMap::new());
        let calls = AtomicUsize::new(0);
        let fetch = || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(99) }
        };
        for _ in 0..3 {
            assert_eq!(
                get_or_fetch(&map, "k".to_string(), fetch).await.unwrap(),
                99
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn get_or_fetch_refetches_after_clear() {
        let map: Mutex<HashMap<String, i32>> = Mutex::new(HashMap::new());
        let calls = AtomicUsize::new(0);
        let fetch = || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(1) }
        };
        get_or_fetch(&map, "k".to_string(), fetch).await.unwrap();
        map.lock().unwrap().clear(); // == invalidate
        get_or_fetch(&map, "k".to_string(), fetch).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn get_or_fetch_does_not_cache_err() {
        let map: Mutex<HashMap<String, i32>> = Mutex::new(HashMap::new());
        let calls = AtomicUsize::new(0);
        let fetch = || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(CoreError::Query("boom".into())) }
        };
        assert!(get_or_fetch(&map, "k".to_string(), fetch).await.is_err());
        assert!(get_or_fetch(&map, "k".to_string(), fetch).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn invalidate_clears_all_maps() {
        let cache = SchemaCache::new();
        cache
            .databases
            .lock()
            .unwrap()
            .insert(ConnectionId("a".into()), vec![]);
        cache
            .tables
            .lock()
            .unwrap()
            .insert((ConnectionId("a".into()), "db".into()), vec![]);
        cache.invalidate();
        assert!(cache.databases.lock().unwrap().is_empty());
        assert!(cache.tables.lock().unwrap().is_empty());
    }

    #[test]
    fn invalidate_connection_drops_only_that_connection() {
        let cache = SchemaCache::new();
        let a = ConnectionId("a".into());
        let b = ConnectionId("b".into());
        cache.databases.lock().unwrap().insert(a.clone(), vec![]);
        cache.databases.lock().unwrap().insert(b.clone(), vec![]);
        cache
            .tables
            .lock()
            .unwrap()
            .insert((a.clone(), "db".into()), vec![]);
        cache
            .columns
            .lock()
            .unwrap()
            .insert((a.clone(), "db".into(), "dbo".into(), "t".into()), vec![]);
        cache.invalidate_connection(&a);
        assert!(!cache.databases.lock().unwrap().contains_key(&a));
        assert!(cache.databases.lock().unwrap().contains_key(&b));
        assert!(cache.tables.lock().unwrap().is_empty());
        assert!(cache.columns.lock().unwrap().is_empty());
    }

    #[test]
    fn forget_connection_clears_data_for_that_connection() {
        let cache = SchemaCache::new();
        let a = ConnectionId("a".into());
        cache.databases.lock().unwrap().insert(a.clone(), vec![]);
        cache
            .tables
            .lock()
            .unwrap()
            .insert((a.clone(), "db".into()), vec![]);
        cache.forget_connection(&a); // clears cached data + evicts the live client
        assert!(!cache.databases.lock().unwrap().contains_key(&a));
        assert!(cache.tables.lock().unwrap().is_empty());
    }

    // ---- §7. live (env-gated) smoke tests — clean-skip when MSSQL_* unset ----

    use crate::test_support::env_connection;

    #[tokio::test]
    async fn live_list_databases_contains_online_master() {
        let Some((cfg, store, _)) = env_connection() else {
            eprintln!("skipping live_list_databases: MSSQL_* env not set");
            return;
        };
        let sessions = SessionCache::new();
        let dbs = list_databases(&sessions, &cfg, &store).await.unwrap();
        assert!(!dbs.is_empty());
        let master = dbs
            .iter()
            .find(|d| d.name == "master")
            .expect("master database present");
        assert!(!master.state_desc.is_empty());
        assert_eq!(master.state_desc, "ONLINE");
    }

    // Note A — assert the bit → CellValue::Bool decode by pointing list_columns
    // at a column with a KNOWN NOT-NULL primary key. Self-contained: create a
    // throwaway table, introspect, then drop it. A `bit`-decoding surprise fails
    // loudly here on the first live run instead of silently mis-parsing.
    #[tokio::test]
    async fn live_list_columns_decodes_pk_and_nullable_flags() {
        let Some((cfg, store, db)) = env_connection() else {
            eprintln!("skipping live_list_columns: MSSQL_* env not set");
            return;
        };
        let ctx = ExecutionContext::new(cfg.id.clone()).with_database(&db);
        let table = "__billz_schema_smoke";
        // Setup (idempotent): drop-if-exists, then create.
        run(
            &cfg,
            &store,
            &ctx,
            "IF OBJECT_ID(N'dbo.__billz_schema_smoke', N'U') IS NOT NULL \
                 DROP TABLE dbo.__billz_schema_smoke; \
             CREATE TABLE dbo.__billz_schema_smoke \
                 (id int NOT NULL PRIMARY KEY, note nvarchar(50) NULL);",
        )
        .await
        .expect("create smoke table");

        let sessions = SessionCache::new();
        let cols = list_columns(&sessions, &cfg, &store, &db, "dbo", table)
            .await
            .unwrap();

        // Teardown (best-effort) before assertions so a failure still cleans up.
        let _ = run(
            &cfg,
            &store,
            &ctx,
            "IF OBJECT_ID(N'dbo.__billz_schema_smoke', N'U') IS NOT NULL \
                 DROP TABLE dbo.__billz_schema_smoke;",
        )
        .await;

        let id = cols
            .iter()
            .find(|c| c.name == "id")
            .expect("id column present");
        assert_eq!(id.data_type, "int");
        assert!(id.is_primary_key, "id must decode is_primary_key == true");
        assert!(!id.nullable, "id must decode nullable == false");
        assert!(!id.is_foreign_key);

        let note = cols
            .iter()
            .find(|c| c.name == "note")
            .expect("note column present");
        assert_eq!(note.data_type, "nvarchar(50)");
        assert!(note.nullable);
        assert!(!note.is_primary_key);
    }

    // Live: list_columns on a nonexistent table → empty result → Ok(vec![]).
    #[tokio::test]
    async fn live_list_columns_unknown_table_is_empty() {
        let Some((cfg, store, db)) = env_connection() else {
            eprintln!("skipping live_list_columns_unknown_table: MSSQL_* env not set");
            return;
        };
        let sessions = SessionCache::new();
        let cols = list_columns(&sessions, &cfg, &store, &db, "dbo", "__no_such_table_xyz__")
            .await
            .unwrap();
        assert!(cols.is_empty());
    }
}
