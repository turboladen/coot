//! The executor — one of two modules (with `session`) where `mssql-client` is
//! used in non-test code.
//!
//! [`run`] connects (per call), applies the [`ExecutionContext`]'s `USE`, runs a
//! SQL batch, and maps the driver's `SqlValue`→[`CellValue`] and
//! `Column`→[`ColumnMeta`], returning `core`'s own [`QueryResult`]s. **No
//! `mssql_client` type appears in this module's public API** — the driver is
//! confined to `run`'s private body and the two private mappers below
//! (`PLAN.md` §3, `CLAUDE.md`). Errors are stringified into [`CoreError`];
//! the driver's `Error` is never `#[from]`.

use futures::StreamExt;
use mssql_client::{
    Client, Column, Config, Error as DriverError, NamedParam, QueryStream, Ready, SqlValue,
};

use crate::connection::{ConnectionConfig, SecretStore, build_connection_string};
use crate::context::ExecutionContext;
use crate::error::{CoreError, Result};
use crate::param_bind::{BindValue, ResolvedParam, parse_bind_value, partition, splice_raw_text};
use crate::result::{CellValue, ColumnMeta, DbRunOutcome, QueryResult};
use crate::types::friendly_type_name;

/// Connect, apply the execution context, run the batch, and return every result
/// set it produced. The driver never crosses this boundary — the return type is
/// core's own [`QueryResult`] and errors are [`CoreError`].
///
/// Connects per call (`cfg` supplies the connection metadata; `ctx` supplies the
/// target database via `USE`). No `params` (bind params are Phase 3) and no `GO`
/// splitting (the runner's batch semantics are Phase 1) — `sql` is sent as one
/// batch.
///
// Cross-tenant fan-out ships via [`run_fanout`] below: N *parallel* per-call
// connects (one login per DB), bounded by a concurrency cap. Pooled connection
// *reuse* across a fan-out — a live client per (connection, database) — stays
// deferred as an optimization (bead billz-0gh.1.1).
pub async fn run(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    ctx: &ExecutionContext,
    sql: &str,
) -> Result<Vec<QueryResult>> {
    let mut client = connect(cfg, store).await?;
    // Close even on error: capture the result, close, then return it.
    let out = run_batch(&mut client, ctx, sql).await;
    let _ = client.close().await;
    out
}

/// Run the same `batches` against many `databases` on one server **in parallel**,
/// returning a per-database [`DbRunOutcome`] — the cross-tenant fan-out primitive
/// (`PLAN.md` §4). Each database is an independent unit of work: connect once,
/// apply `base.clone().with_database(db)`, run every batch on that one connection,
/// close. One login per DB; no pooled reuse (bead billz-0gh.1.1).
///
/// **Never returns `Result`.** A failing database (unreachable, a bad `USE`, a SQL
/// error) is captured into that DB's `DbRunOutcome.error` — the other databases
/// still run. `base` supplies the connection; its database is overridden per DB.
///
/// `max_concurrency` caps in-flight connections (`buffer_unordered`). That
/// combinator yields completions out of order, so outcomes are re-sorted back to
/// **input order** before returning — the caller's status strip/grid stay stable.
pub async fn run_fanout(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    base: &ExecutionContext,
    databases: &[String],
    batches: &[String],
    max_concurrency: usize,
) -> Vec<DbRunOutcome> {
    // Nothing to fan out to, or nothing to run: return early WITHOUT opening any
    // connection. The `run_fanout` command already guards empty `batches`, but a
    // core primitive shouldn't depend on its caller doing so — an empty batch list
    // would otherwise pay one pointless login per DB. (Copilot review on PR #55.)
    if databases.is_empty() || batches.is_empty() {
        return Vec::new();
    }
    // `buffer_unordered(0)` never polls its futures — floor the cap at 1.
    let concurrency = max_concurrency.max(1);

    // Own each db before the async block: a borrowed `&String` argument would make
    // each future's type depend on the closure-argument lifetime, which the HRTB
    // can't express once this whole future is used in a `#[tauri::command]`
    // (`buffer_unordered` + Tauri limitation). Cloning keeps the argument
    // lifetime-free.
    let pairs: Vec<(usize, DbRunOutcome)> =
        futures::stream::iter(databases.iter().cloned().enumerate())
            .map(|(i, db)| async move {
                let started = std::time::Instant::now();
                let result = run_one_db(cfg, store, base, &db, batches).await;
                let elapsed_ms = started.elapsed().as_millis() as u64;
                (i, outcome_from(db, result, elapsed_ms))
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

    restore_order(pairs)
}

/// One database's unit of work: connect once, then run every batch on that single
/// connection under `base`'s context rebound to `db`, flattening all result sets
/// into one `Vec`. Closes even on error (capture → close → return), mirroring
/// [`run`]. Any failure short-circuits to `Err`; [`run_fanout`]'s caller turns it
/// into a captured `DbRunOutcome.error`.
async fn run_one_db(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    base: &ExecutionContext,
    db: &str,
    batches: &[String],
) -> Result<Vec<QueryResult>> {
    let ctx = base.clone().with_database(db);
    let mut client = connect(cfg, store).await?;
    // Close even on error (matches `run`): capture the result, close, return it.
    let out = run_all_batches(&mut client, &ctx, batches).await;
    let _ = client.close().await;
    out
}

/// Run every batch on an ALREADY-connected client under `ctx`, flattening all
/// result sets into one `Vec`. Split out of [`run_one_db`] so that fn can
/// `close()` the client even when a batch errors (mirrors [`run`]).
async fn run_all_batches(
    client: &mut Client<Ready>,
    ctx: &ExecutionContext,
    batches: &[String],
) -> Result<Vec<QueryResult>> {
    let mut out = Vec::new();
    for batch in batches {
        out.append(&mut run_batch(client, ctx, batch).await?);
    }
    Ok(out)
}

/// Assemble a [`DbRunOutcome`] from one database's run: `Ok` carries the result
/// sets with no error; `Err` maps to empty results plus the stringified error
/// (via [`CoreError`]'s `Display`) — the fan-out's capture-don't-propagate seam.
fn outcome_from(
    database: String,
    result: Result<Vec<QueryResult>>,
    elapsed_ms: u64,
) -> DbRunOutcome {
    match result {
        Ok(results) => DbRunOutcome {
            database,
            results,
            error: None,
            elapsed_ms,
        },
        Err(e) => DbRunOutcome {
            database,
            results: Vec::new(),
            error: Some(e.to_string()),
            elapsed_ms,
        },
    }
}

/// Restore input order after `buffer_unordered` (which yields completions as they
/// finish): sort the `(input_index, outcome)` pairs by index, then drop the index.
fn restore_order(mut pairs: Vec<(usize, DbRunOutcome)>) -> Vec<DbRunOutcome> {
    pairs.sort_by_key(|(i, _)| *i);
    pairs.into_iter().map(|(_, outcome)| outcome).collect()
}

/// Apply `ctx`'s `USE` and run `sql` on an ALREADY-connected client, returning
/// every result set. Neither connects nor closes — shared by [`run`]
/// (connect → run_batch → close) and [`crate::session::SessionCache`] (reuse a
/// live client → run_batch, no close). `pub(crate)`: the driver stays inside
/// `core`. A reused session carries state, so applying the context's `USE` here
/// (every call) is mandatory, not optional.
pub(crate) async fn run_batch(
    client: &mut Client<Ready>,
    ctx: &ExecutionContext,
    sql: &str,
) -> Result<Vec<QueryResult>> {
    apply_use_statement(client, ctx).await?;
    collect_multi(client, sql).await
}

/// Run a multi-result batch on a connected client and drain every result set
/// into core's [`QueryResult`]s. Does NOT apply `USE` (the caller does) and does
/// not close. Shared by [`run_batch`] and the no-bind branch of
/// [`run_params_on_client`] so the collection logic — and any future
/// `rows_affected` / PRINT capture (`billz-38l` / `billz-mfd`) — lives in ONE
/// place instead of drifting between the two paths.
async fn collect_multi(client: &mut Client<Ready>, sql: &str) -> Result<Vec<QueryResult>> {
    let multi = client
        .query_multiple(sql, &[])
        .await
        .map_err(map_driver_error)?;

    // Streams borrow `&mut client`; the borrow ends once they're all consumed
    // below, so callers can reuse or close the client afterward.
    let streams = multi.into_query_streams();
    let mut out = Vec::with_capacity(streams.len());
    for stream in streams {
        out.push(query_stream_to_result(stream)?);
    }
    Ok(out)
}

/// Run `sql` with parameters, applying the [`ExecutionContext`], and return the
/// result(s). **Core types only** in the signature — `params` is `core`'s own
/// [`ResolvedParam`] and the return is [`QueryResult`]; no `NamedParam`/`SqlValue`/
/// `Client` leaks past this boundary (`PLAN.md` §3/§7, `CLAUDE.md`).
///
/// Two mechanisms, decided by each param's `sql_type` (`PLAN.md` §5):
///   - `None` → a **raw-text** fragment, spliced literally into the SQL before
///     send (injectable BY DESIGN; d28.6 flags it loud).
///   - `Some(_)` → a **bind** param: its value is parsed to a typed value and sent
///     via `sp_executesql` (safe, typed) — the driver derives the type declaration
///     from the value at runtime.
///
/// **Single-result-set on the bind path (driver limitation, `PLAN.md` §0 F5):** the
/// driver has no named multi-result API, so a query that actually has bind params
/// returns only its first result set. A query with only raw-text params (or none)
/// still routes through the multi-result path after splicing, preserving
/// multi-result there. Follow-up: bead for a positional `@cust`→`@p1` remap if
/// multi-result-with-bind is ever needed.
pub async fn run_with_params(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    ctx: &ExecutionContext,
    sql: &str,
    params: &[ResolvedParam],
) -> Result<Vec<QueryResult>> {
    let mut client = connect(cfg, store).await?;
    // Close even on error (matches `run`): capture the result, close, return it.
    let out = run_params_on_client(&mut client, ctx, sql, params).await;
    let _ = client.close().await;
    out
}

/// Splice raw-text params, bind typed params, apply `ctx`'s `USE`, and run on an
/// ALREADY-connected client — neither connects nor closes. Named-empty keeps the
/// multi-result path (raw-text-only or no params); named-present uses
/// `sp_executesql` (single result set, F5). Split out of [`run_with_params`] so
/// that fn can `close()` the client even when this errors.
async fn run_params_on_client(
    client: &mut Client<Ready>,
    ctx: &ExecutionContext,
    sql: &str,
    params: &[ResolvedParam],
) -> Result<Vec<QueryResult>> {
    apply_use_statement(client, ctx).await?;

    let (raw, bind) = partition(params);
    let sent_sql = splice_raw_text(sql, &raw);

    // Parse each bind value pre-flight (a bad value is `CoreError::Param`, not a
    // driver error) and build the driver's `NamedParam`s. `sql_value_from_bind` is
    // the ONE place `BindValue` meets the driver's `SqlValue`.
    let mut named = Vec::with_capacity(bind.len());
    for p in &bind {
        // `partition` only puts `Some(sql_type)` params in `bind`.
        let sql_type = p
            .sql_type
            .expect("partition guarantees bind params carry Some(sql_type)");
        let value = parse_bind_value(sql_type, &p.value)?;
        named.push(NamedParam::new(p.name.clone(), sql_value_from_bind(value)));
    }

    if named.is_empty() {
        // No bind params → keep the multi-result path (raw-text-only or no params).
        collect_multi(client, &sent_sql).await
    } else {
        // Bind params → `sp_executesql`, a single result set (F5).
        let stream = client
            .query_named(&sent_sql, &named)
            .await
            .map_err(map_driver_error)?;
        Ok(vec![query_stream_to_result(stream)?])
    }
}

/// Connect per call: resolve the stored password, build the driver `Config`, and
/// connect. Shared by [`run`] and [`run_with_params`]. A missing stored password
/// is a *configuration* gap, not a store *failure* (`get_password` returned
/// `Ok(None)`), so it maps to `Config`, not `Secret`.
pub(crate) async fn connect(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
) -> Result<Client<Ready>> {
    // Reachability preflight FIRST — before touching the Keychain. On this DEV
    // setup the server sits behind an Azure VPN tunnel; when it's down the driver
    // takes ~15s to fail with a cryptic timeout. A quick TCP probe fails fast with
    // a friendly message AND avoids a pointless macOS Keychain prompt on a connect
    // that can't succeed. Skipped when no static host:port is derivable (see
    // `preflight_target`), so it never blocks a working connection on a guess.
    if let Some((host, port)) = cfg.preflight_target() {
        preflight_reachable(&host, port).await?;
    }
    let password = store.get_password(&cfg.id)?.ok_or_else(|| {
        CoreError::Config(format!("no stored password for connection {}", cfg.id.0))
    })?;
    let conn_str = build_connection_string(cfg, &password);
    let config =
        Config::from_connection_string(&conn_str).map_err(|e| CoreError::Config(e.to_string()))?;
    Client::connect(config)
        .await
        .map_err(|e| CoreError::Config(e.to_string()))
}

/// TCP-probe `host:port` with a short timeout, so a down VPN (or any unreachable
/// server) surfaces as a friendly [`CoreError::Unreachable`] instead of the
/// driver's slow, cryptic login timeout. A bare connect + immediate drop (no TDS
/// handshake) is harmless to the server. Both failure shapes — a dial error
/// (connection refused / DNS failure) and an elapsed timeout — collapse to the
/// same human-facing message carrying `host:port`.
async fn preflight_reachable(host: &str, port: u16) -> Result<()> {
    const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
    let unreachable = || {
        CoreError::Unreachable(format!(
            "Can't reach SQL Server at {host}:{port}. Your VPN tunnel may be down \
             — bring it up and retry."
        ))
    };
    match tokio::time::timeout(DIAL_TIMEOUT, tokio::net::TcpStream::connect((host, port))).await {
        Ok(Ok(_stream)) => Ok(()), // reachable; the probe socket drops here
        Ok(Err(_dial_err)) => Err(unreachable()), // connection refused / DNS failure
        Err(_elapsed) => Err(unreachable()), // timed out (the classic VPN-down case)
    }
}

/// Map a driver error to a [`CoreError`], distinguishing **transport-level**
/// failures (dropped/closed socket, TLS, TDS protocol/codec desync — retryable
/// on a fresh connection) from deterministic server/query errors. This is the
/// ONE place `mssql_client::Error`'s variants are inspected; the driver type
/// never escapes. Drives the session's retry-only-on-transport decision
/// (`billz-lpb.1`). `Error` is `#[non_exhaustive]`, so the wildcard arm is
/// mandatory — an unknown future variant is treated as deterministic (no retry).
fn map_driver_error(e: DriverError) -> CoreError {
    match &e {
        DriverError::Io(_)
        | DriverError::Connection(_)
        | DriverError::ConnectionClosed
        | DriverError::Tls(_)
        | DriverError::ProtocolError(_)
        | DriverError::Protocol(_)
        | DriverError::Codec(_) => CoreError::Transport(e.to_string()),
        // Server/Query/Authentication/Type/ResponseTooLarge/… are deterministic:
        // re-running would just repeat them, so they are NOT retried.
        _ => CoreError::Query(e.to_string()),
    }
}

/// Apply the database context as a separate statement so the user's SQL stays
/// unmodified (accurate server error line numbers). `use_statement` does the
/// identifier bracket-quoting — never hand-splice `USE` here.
async fn apply_use_statement(client: &mut Client<Ready>, ctx: &ExecutionContext) -> Result<()> {
    if let Some(use_stmt) = ctx.use_statement() {
        client
            .execute(&use_stmt, &[])
            .await
            .map_err(map_driver_error)?;
    }
    Ok(())
}

/// Drain one driver `QueryStream` into core's `QueryResult`: snapshot the column
/// metadata, then map every row's `SqlValue`s to `CellValue`s. Shared by the
/// multi-result path ([`run`], the raw-text-only branch) and the single-stream
/// bind path. The driver types never escape this fn.
fn query_stream_to_result(stream: QueryStream<'_>) -> Result<QueryResult> {
    // Snapshot column metadata before the `for row` loop consumes the stream.
    let columns: Vec<ColumnMeta> = stream.columns().iter().map(column_meta).collect();
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for row in stream {
        let row = row.map_err(map_driver_error)?;
        let cells = (0..row.len())
            .map(|i| {
                // `get_raw` returns `None` on out-of-range *or* decode error
                // (`parse_value(..).ok()`); either way we surface `Null`.
                // Acceptable for a personal tool: a value that fails to decode is
                // indistinguishable from a real SQL NULL here.
                row.get_raw(i)
                    .map(|v| cell_from_sql_value(&v))
                    .unwrap_or(CellValue::Null)
            })
            .collect();
        rows.push(cells);
    }
    // TODO(later): capture PRINT/info messages — bead billz-mfd.
    // TODO(later): populate rows_affected for DML — bead billz-38l.
    Ok(QueryResult {
        columns,
        rows,
        rows_affected: None,
    })
}

/// Map core's [`BindValue`] to the driver's `SqlValue` — the 9→9 trivial map and
/// the ONLY place `BindValue` meets `mssql_client`. Closed match (no wildcard):
/// `BindValue` is core-owned, so a new variant is a compile error here.
fn sql_value_from_bind(v: BindValue) -> SqlValue {
    match v {
        BindValue::Int(n) => SqlValue::Int(n),
        BindValue::BigInt(n) => SqlValue::BigInt(n),
        BindValue::Text(s) => SqlValue::String(s),
        BindValue::Bool(b) => SqlValue::Bool(b),
        BindValue::Date(d) => SqlValue::Date(d),
        BindValue::DateTime(dt) => SqlValue::DateTime(dt),
        BindValue::Decimal(d) => SqlValue::Decimal(d),
        BindValue::Uuid(u) => SqlValue::Uuid(u),
        BindValue::Money(d) => SqlValue::Money(d),
    }
}

/// Map a driver `Column` to core's `ColumnMeta`. Private — the driver type never
/// crosses `core`'s boundary. `type_name` is the Debug name of a TDS `TypeId`
/// (`"Int4"`, `"DecimalN"`, `"NVarChar"`, …); [`friendly_type_name`] turns it
/// into the friendly name. `max_length`/`collation` are dropped (no field for
/// them; width-aware disambiguation is deferred — see `types.rs` / bead billz-9qg).
fn column_meta(col: &Column) -> ColumnMeta {
    ColumnMeta {
        name: col.name.clone(),
        sql_type: friendly_type_name(&col.type_name, col.max_length).to_string(),
        nullable: col.nullable,
        precision: col.precision,
        scale: col.scale,
    }
}

/// Map a driver `SqlValue` to core's `CellValue`. Private — the driver type never
/// crosses `core`'s boundary. `SqlValue` is `#[non_exhaustive]`, so the wildcard
/// arm is mandatory: `Tvp`, feature-off `Json`, and any future variant fall
/// through to a `Text("<TYPE>")` placeholder (mirrors the spike's `<TVP>`).
fn cell_from_sql_value(v: &SqlValue) -> CellValue {
    match v {
        SqlValue::Null => CellValue::Null,
        SqlValue::Bool(b) => CellValue::Bool(*b),
        // 8/16/32-bit integers widen into an i64 number cell (all fit f64 exactly).
        SqlValue::TinyInt(n) => CellValue::Int(i64::from(*n)),
        SqlValue::SmallInt(n) => CellValue::Int(i64::from(*n)),
        SqlValue::Int(n) => CellValue::Int(i64::from(*n)),
        // bigint exceeds f64's safe integer range → string cell (billz-s7p).
        SqlValue::BigInt(n) => CellValue::BigInt(n.to_string()),
        SqlValue::Float(f) => CellValue::Float(f64::from(*f)),
        SqlValue::Double(f) => CellValue::Float(*f),
        SqlValue::String(s) => CellValue::Text(s.clone()),
        // Full lowercase hex, `0x` prefix, no truncation (spike `render_cell`).
        SqlValue::Binary(bytes) => {
            use std::fmt::Write as _;
            let mut hex = String::with_capacity(2 + bytes.len() * 2);
            hex.push_str("0x");
            for byte in bytes.iter() {
                // write! into the buffer — no per-byte String allocation.
                let _ = write!(hex, "{byte:02x}");
            }
            CellValue::Binary(hex)
        }
        // Decimal/Money/SmallMoney all render as a string — no f64 precision loss.
        // Money/SmallMoney are send-side variants (they decode back to `Decimal`
        // on read); the arms keep the match total and honest.
        SqlValue::Decimal(d) => CellValue::Decimal(d.to_string()),
        SqlValue::Money(d) => CellValue::Decimal(d.to_string()),
        SqlValue::SmallMoney(d) => CellValue::Decimal(d.to_string()),
        SqlValue::Uuid(u) => CellValue::Uuid(u.to_string()),
        SqlValue::Date(d) => CellValue::Date(d.to_string()),
        SqlValue::Time(t) => CellValue::Time(t.to_string()),
        SqlValue::DateTime(dt) => CellValue::DateTime(dt.to_string()),
        // SmallDateTime decodes back to `DateTime` on read; defensive arm.
        SqlValue::SmallDateTime(dt) => CellValue::DateTime(dt.to_string()),
        SqlValue::DateTimeOffset(dt) => CellValue::DateTimeOffset(dt.to_string()),
        SqlValue::Xml(s) => CellValue::Xml(s.clone()),
        // `Tvp`, feature-off `Json`, any future variant.
        other => CellValue::Text(format!("<{}>", other.type_name())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- cell_from_sql_value: one assertion per constructible variant ----

    fn dec(s: &str) -> rust_decimal::Decimal {
        rust_decimal::Decimal::from_str_exact(s).unwrap()
    }

    #[test]
    fn null_and_bool() {
        assert_eq!(cell_from_sql_value(&SqlValue::Null), CellValue::Null);
        assert_eq!(
            cell_from_sql_value(&SqlValue::Bool(true)),
            CellValue::Bool(true)
        );
    }

    #[test]
    fn small_integer_families_widen_to_int() {
        assert_eq!(
            cell_from_sql_value(&SqlValue::TinyInt(7)),
            CellValue::Int(7)
        );
        assert_eq!(
            cell_from_sql_value(&SqlValue::SmallInt(-3)),
            CellValue::Int(-3)
        );
        assert_eq!(cell_from_sql_value(&SqlValue::Int(42)), CellValue::Int(42));
    }

    #[test]
    fn bigint_maps_to_string_to_survive_json() {
        // billz-s7p: a bigint beyond f64's safe integer range (2^53 + 1) must
        // become a string cell, not an i64 number cell.
        assert_eq!(
            cell_from_sql_value(&SqlValue::BigInt(9_007_199_254_740_993)),
            CellValue::BigInt("9007199254740993".into())
        );
    }

    #[test]
    fn floats_widen_to_f64() {
        assert_eq!(
            cell_from_sql_value(&SqlValue::Float(1.5_f32)),
            CellValue::Float(1.5)
        );
        assert_eq!(
            cell_from_sql_value(&SqlValue::Double(2.5_f64)),
            CellValue::Float(2.5)
        );
    }

    #[test]
    fn string_and_xml() {
        assert_eq!(
            cell_from_sql_value(&SqlValue::String("héllo".into())),
            CellValue::Text("héllo".into())
        );
        assert_eq!(
            cell_from_sql_value(&SqlValue::Xml("<r/>".into())),
            CellValue::Xml("<r/>".into())
        );
    }

    #[test]
    fn binary_renders_full_lowercase_hex() {
        let v = SqlValue::Binary(bytes::Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(
            cell_from_sql_value(&v),
            CellValue::Binary("0xdeadbeef".into())
        );
    }

    #[test]
    fn binary_pads_each_byte_to_two_hex_digits() {
        let v = SqlValue::Binary(bytes::Bytes::from(vec![0x00, 0x0f, 0xff]));
        assert_eq!(
            cell_from_sql_value(&v),
            CellValue::Binary("0x000fff".into())
        );
    }

    #[test]
    fn decimal_money_smallmoney_all_render_as_string() {
        let expected = CellValue::Decimal("1234.5678".into());
        assert_eq!(
            cell_from_sql_value(&SqlValue::Decimal(dec("1234.5678"))),
            expected
        );
        assert_eq!(
            cell_from_sql_value(&SqlValue::Money(dec("1234.5678"))),
            expected
        );
        assert_eq!(
            cell_from_sql_value(&SqlValue::SmallMoney(dec("1234.5678"))),
            expected
        );
    }

    #[test]
    fn uuid_renders_lowercase_hyphenated() {
        let u = uuid::Uuid::parse_str("6f9619ff-8b86-d011-b42d-00c04fc964ff").unwrap();
        assert_eq!(
            cell_from_sql_value(&SqlValue::Uuid(u)),
            CellValue::Uuid("6f9619ff-8b86-d011-b42d-00c04fc964ff".into())
        );
    }

    #[test]
    fn date_time_datetime_offset() {
        let d = chrono::NaiveDate::from_ymd_opt(2024, 3, 17).unwrap();
        assert_eq!(
            cell_from_sql_value(&SqlValue::Date(d)),
            CellValue::Date("2024-03-17".into())
        );

        let t = chrono::NaiveTime::from_hms_opt(12, 34, 56).unwrap();
        assert_eq!(
            cell_from_sql_value(&SqlValue::Time(t)),
            CellValue::Time("12:34:56".into())
        );

        let dt = d.and_time(t);
        assert_eq!(
            cell_from_sql_value(&SqlValue::DateTime(dt)),
            CellValue::DateTime("2024-03-17 12:34:56".into())
        );
        // SmallDateTime maps to the same DateTime cell.
        assert_eq!(
            cell_from_sql_value(&SqlValue::SmallDateTime(dt)),
            CellValue::DateTime("2024-03-17 12:34:56".into())
        );

        let off = chrono::FixedOffset::east_opt(0).unwrap();
        let dto = dt.and_local_timezone(off).unwrap();
        // Literal expected string (not `dto.to_string()`) so this pins the
        // rendered format instead of re-applying the mapper's own conversion.
        assert_eq!(
            cell_from_sql_value(&SqlValue::DateTimeOffset(dto)),
            CellValue::DateTimeOffset("2024-03-17 12:34:56 +00:00".into())
        );
    }

    // ---- column_meta: wire-token → friendly, via constructible Column ----

    #[test]
    fn column_meta_maps_int_and_not_nullable() {
        let col = Column::new("id", 0, "Int4").with_nullable(false);
        let meta = column_meta(&col);
        assert_eq!(meta.name, "id");
        assert_eq!(meta.sql_type, "int");
        assert!(!meta.nullable);
        assert_eq!(meta.precision, None);
        assert_eq!(meta.scale, None);
    }

    #[test]
    fn column_meta_carries_precision_and_scale_for_decimal() {
        let col = Column::new("amount", 1, "DecimalN").with_precision_scale(19, 4);
        let meta = column_meta(&col);
        assert_eq!(meta.sql_type, "decimal");
        assert_eq!(meta.precision, Some(19));
        assert_eq!(meta.scale, Some(4));
    }

    #[test]
    fn column_meta_defaults_nullable_true_for_nvarchar() {
        let col = Column::new("note", 2, "NVarChar");
        let meta = column_meta(&col);
        assert_eq!(meta.sql_type, "nvarchar");
        assert!(meta.nullable);
        assert_eq!(meta.precision, None);
    }

    #[test]
    fn map_driver_error_classifies_transport_vs_deterministic() {
        // Transport (retryable — a reused stale socket): dropped/closed socket,
        // TDS protocol/codec desync.
        assert!(map_driver_error(DriverError::ConnectionClosed).is_transport());
        assert!(map_driver_error(DriverError::Connection("reset by peer".into())).is_transport());
        assert!(map_driver_error(DriverError::Protocol("token desync".into())).is_transport());
        // Deterministic (NOT retried — re-running just repeats it).
        assert!(!map_driver_error(DriverError::Query("permission denied".into())).is_transport());
        assert!(
            !map_driver_error(DriverError::ResponseTooLarge { size: 10, limit: 5 }).is_transport()
        );
    }

    #[test]
    fn column_meta_passes_unknown_token_through() {
        let col = Column::new("x", 3, "SomeFutureType");
        assert_eq!(column_meta(&col).sql_type, "SomeFutureType");
    }

    #[test]
    fn column_meta_is_width_aware_for_nullable_bigint() {
        // billz-9qg: a nullable bigint arrives as IntN with max_length 8 → bigint,
        // not the old collapsed "int".
        let col = Column::new("id", 0, "IntN").with_max_length(8);
        assert_eq!(column_meta(&col).sql_type, "bigint");
    }

    // ---- preflight_reachable: no DB, no VPN needed (pure loopback) ----

    #[tokio::test]
    async fn preflight_reachable_ok_when_port_is_listening() {
        // Bind a loopback listener and keep it alive → the probe connects.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        preflight_reachable(&addr.ip().to_string(), addr.port())
            .await
            .expect("a listening port must be reachable");
    }

    #[tokio::test]
    async fn preflight_reachable_friendly_error_on_closed_port() {
        // Bind then drop → the port is (almost certainly) closed, so the dial is
        // refused immediately and we get the friendly Unreachable message.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let err = preflight_reachable("127.0.0.1", port).await.unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, CoreError::Unreachable(_)), "got {err:?}");
        assert!(msg.contains(&format!("127.0.0.1:{port}")), "msg: {msg}");
        assert!(msg.contains("VPN"), "msg: {msg}");
    }

    // ---- run_fanout assembly seams (pure, no DB) ----

    fn outcome(database: &str) -> DbRunOutcome {
        DbRunOutcome {
            database: database.into(),
            results: Vec::new(),
            error: None,
            elapsed_ms: 0,
        }
    }

    #[test]
    fn restore_order_sorts_completions_back_to_input_order() {
        // buffer_unordered yields out of order (2, 0, 1); restore_order must put
        // them back by input index so the caller sees stable order.
        let shuffled = vec![(2, outcome("c")), (0, outcome("a")), (1, outcome("b"))];
        let ordered = restore_order(shuffled);
        let names: Vec<&str> = ordered.iter().map(|o| o.database.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn outcome_from_ok_carries_results_and_no_error() {
        let results = vec![QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            rows_affected: None,
        }];
        let o = outcome_from("master".into(), Ok(results), 7);
        assert_eq!(o.database, "master");
        assert_eq!(o.results.len(), 1);
        assert_eq!(o.error, None);
        assert_eq!(o.elapsed_ms, 7);
    }

    #[test]
    fn outcome_from_err_captures_message_and_empty_results() {
        let o = outcome_from("nope".into(), Err(CoreError::Query("bad db".into())), 3);
        assert_eq!(o.database, "nope");
        assert!(o.results.is_empty());
        assert!(
            o.error.as_deref().is_some_and(|m| m.contains("bad db")),
            "error should carry the CoreError Display, got {:?}",
            o.error
        );
        assert_eq!(o.elapsed_ms, 3);
    }

    #[tokio::test]
    async fn run_fanout_short_circuits_empty_batches_without_connecting() {
        // No batches ⇒ nothing to run: run_fanout must return early and NEVER open
        // a connection (else one pointless login per DB). The dummy config points at
        // an unroutable server, so a regressed guard would attempt a connect and
        // yield error outcomes (len 2) instead of the empty Vec we assert. (Copilot
        // review on PR #55.)
        use crate::connection::{ConnectionId, InMemorySecretStore};
        let cfg = ConnectionConfig {
            id: ConnectionId("unused".into()),
            name: "unused".into(),
            server: "0.0.0.0:0".into(),
            username: "sa".into(),
            default_database: None,
            encrypt: false,
            trust_server_certificate: true,
            remember_password: false,
        };
        let store = InMemorySecretStore::default();
        let base = ExecutionContext::new(cfg.id.clone());
        let out = run_fanout(&cfg, &store, &base, &["db_a".into(), "db_b".into()], &[], 8).await;
        assert!(
            out.is_empty(),
            "empty batches must short-circuit before connecting, got {} outcome(s)",
            out.len()
        );
    }

    // ---- one env-gated live smoke test (clean skip with no DB) ----

    use crate::test_support::env_connection;

    #[tokio::test]
    async fn run_returns_clean_query_result() {
        let Some((cfg, store, _)) = env_connection() else {
            eprintln!("skipping run_returns_clean_query_result: MSSQL_* env not set");
            return;
        };
        let ctx = ExecutionContext::new(cfg.id.clone());
        let results = run(&cfg, &store, &ctx, "SELECT CAST(1 AS int) AS a")
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].columns[0].sql_type, "int");
        assert_eq!(results[0].rows[0][0], CellValue::Int(1));
    }
}
