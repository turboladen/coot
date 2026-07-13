//! DEV-box integration tests — the Phase-0 exit gate (bead `billz-ce1.7`).
//!
//! Ports the two spike probes (`examples/dynamic_dump.rs` untyped, and
//! `examples/typed_probe.rs` typed) into `core`'s first integration tests. As an
//! integration-test crate this sees only `billz_core`'s **public API** plus the
//! crate's normal deps (`tokio`, `mssql_client`, `chrono`, `rust_decimal`,
//! `uuid`) — no `Cargo.toml` change is needed; `tests/` already resolves the
//! `[dependencies]` table.
//!
//! Every test is env-gated: it skips cleanly at **runtime** (NOT `#[ignore]`)
//! when any `MSSQL_*` var is unset, so `cargo test` stays fully green with no DB.
//! The real green-against-the-box run is the user's manual Phase-0 exit step:
//!
//! ```fish
//! set -x MSSQL_SERVER   "<host,1433>"
//! set -x MSSQL_USER     "<user>"
//! set -x MSSQL_PASSWORD (op read "op://.../password")
//! set -x MSSQL_DATABASE "<a real DEV db>"
//! cargo test -p billz-core
//! ```
//!
//! Re-run that after any `mssql-client` bump (CLAUDE.md standing order).

use billz_core::{
    CellValue, ConnectionConfig, ConnectionId, ExecutionContext, InMemorySecretStore, QueryResult,
    ResolvedParam, SecretStore, SqlType, build_connection_string, run, run_with_params,
};

/// Build a live `(cfg, store)` from `MSSQL_*` env, or `None` when any required
/// var is unset — a runtime skip (NOT `#[ignore]`), so `cargo test` stays green
/// without a DB. Mirrors the private helper in `executor.rs`.
fn env_connection() -> Option<(ConnectionConfig, InMemorySecretStore)> {
    let server = std::env::var("MSSQL_SERVER").ok()?;
    let username = std::env::var("MSSQL_USER").ok()?;
    let password = std::env::var("MSSQL_PASSWORD").ok()?;
    let database = std::env::var("MSSQL_DATABASE").ok()?;

    // encrypt=false / trust_server_certificate=true = the DEV-box defaults the
    // spikes used.
    let cfg = ConnectionConfig {
        id: ConnectionId("itest".into()),
        name: "itest".into(),
        server,
        username,
        default_database: Some(database),
        encrypt: false,
        trust_server_certificate: true,
        remember_password: true,
    };
    let store = InMemorySecretStore::default();
    store.set_password(&cfg.id, &password).unwrap();
    Some((cfg, store))
}

/// Index of the column named `name` in a result set (columns keep SELECT order,
/// but look up by name so a reordered SELECT can't silently pass).
fn idx(r: &QueryResult, name: &str) -> usize {
    r.columns
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("no column named {name}"))
}

// ---------------------------------------------------------------------------
// §3 — the untyped `run()` matrix (the exact path the app uses).
// ---------------------------------------------------------------------------

/// 3.1 — a synthetic mixed-type row exercising most `CellValue` variants at once.
/// Mirrors `dynamic_dump.rs`. Proves the anti-precision-loss guarantee (decimal
/// AND money → `Decimal(String)`), lowercase `0x…` binary, lowercased uuid,
/// NULL → `Null`, and that `sql_type` headers are the friendly names from
/// metadata (PLAN §7: header from wire token, cell from `SqlValue`).
#[tokio::test]
async fn mixed_type_row_maps_to_expected_cellvalues() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping mixed_type_row_maps_to_expected_cellvalues: MSSQL_* env not set");
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone());

    // SYSDATETIMEOFFSET(), NOT a datetimeoffset string literal — a bad literal
    // throws server error 241 (PLAN footgun appendix).
    let sql = r#"SELECT
        CAST(42 AS int)                                                 AS the_int,
        CAST(N'héllo ☃' AS nvarchar(20))                                AS the_text,
        CAST(1 AS bit)                                                  AS the_flag,
        CAST(1234.5678 AS decimal(19,4))                                AS the_decimal,
        CAST(1234.5678 AS money)                                        AS the_money,
        CAST('6F9619FF-8B86-D011-B42D-00C04FC964FF' AS uniqueidentifier) AS the_guid,
        SYSDATETIMEOFFSET()                                             AS the_dto,
        CAST('2024-03-17 12:34:56.123' AS datetime2)                    AS the_dt2,
        CAST('2024-03-17' AS date)                                      AS the_date,
        CAST(0xDEADBEEF AS varbinary(8))                                AS the_binary,
        CAST('<r><i k="1"/></r>' AS xml)                                AS the_xml,
        CAST(NULL AS int)                                               AS the_null"#;

    let results = run(&cfg, &store, &ctx, sql).await.unwrap();
    assert_eq!(results.len(), 1, "one SELECT → one result set");
    let r = &results[0];
    assert_eq!(r.rows.len(), 1, "one row");
    let row = &r.rows[0];

    // Per-cell values (exact).
    assert_eq!(row[idx(r, "the_int")], CellValue::Int(42));
    assert_eq!(row[idx(r, "the_text")], CellValue::Text("héllo ☃".into()));
    assert_eq!(row[idx(r, "the_flag")], CellValue::Bool(true));
    // Anti-precision-loss guarantee: decimal AND money → exact `Decimal(String)`.
    // If ever flaky on trailing-zero variance, fall back to a parse-equals check
    // (`s.parse::<f64>() == 1234.5678`) — but keep the string form, it IS the
    // guarantee under test; do not weaken to a variant-only match.
    assert_eq!(
        row[idx(r, "the_decimal")],
        CellValue::Decimal("1234.5678".into())
    );
    assert_eq!(
        row[idx(r, "the_money")],
        CellValue::Decimal("1234.5678".into())
    );
    // uuid rendered lowercase, hyphenated.
    assert_eq!(
        row[idx(r, "the_guid")],
        CellValue::Uuid("6f9619ff-8b86-d011-b42d-00c04fc964ff".into())
    );
    // datetimeoffset value is `now` (non-deterministic) → variant only.
    assert!(matches!(
        row[idx(r, "the_dto")],
        CellValue::DateTimeOffset(_)
    ));
    // datetime2 exact. Single most likely live-run adjudication point: if the
    // driver renders trailing fractional precision differently, relax to
    // `matches!(cell, CellValue::DateTime(s) if s.starts_with("2024-03-17 12:34:56"))`
    // — prefer the exact assert first, only relax if the box disagrees.
    assert_eq!(
        row[idx(r, "the_dt2")],
        CellValue::DateTime("2024-03-17 12:34:56.123".into())
    );
    assert_eq!(
        row[idx(r, "the_date")],
        CellValue::Date("2024-03-17".into())
    );
    // binary: lowercase hex, `0x` prefix.
    assert_eq!(
        row[idx(r, "the_binary")],
        CellValue::Binary("0xdeadbeef".into())
    );
    // xml: SQL Server may re-serialize → assert the payload survives, variant only.
    assert!(
        matches!(&row[idx(r, "the_xml")], CellValue::Xml(s) if s.contains("k=\"1\"")),
        "xml cell should be Xml(..) containing k=\"1\", got {:?}",
        row[idx(r, "the_xml")]
    );
    assert_eq!(row[idx(r, "the_null")], CellValue::Null);

    // Column `sql_type` headers = friendly names from metadata.
    assert_eq!(r.columns[idx(r, "the_int")].sql_type, "int");
    assert_eq!(r.columns[idx(r, "the_text")].sql_type, "nvarchar");
    assert_eq!(r.columns[idx(r, "the_flag")].sql_type, "bit");
    assert_eq!(r.columns[idx(r, "the_decimal")].sql_type, "decimal");
    assert_eq!(r.columns[idx(r, "the_money")].sql_type, "money");
    assert_eq!(r.columns[idx(r, "the_guid")].sql_type, "uniqueidentifier");
    assert_eq!(r.columns[idx(r, "the_dto")].sql_type, "datetimeoffset");
    assert_eq!(r.columns[idx(r, "the_dt2")].sql_type, "datetime2");
    assert_eq!(r.columns[idx(r, "the_date")].sql_type, "date");
    assert_eq!(r.columns[idx(r, "the_binary")].sql_type, "varbinary");
    assert_eq!(r.columns[idx(r, "the_xml")].sql_type, "xml");
    // the_null's type comes from metadata, not its (NULL) value.
    assert_eq!(r.columns[idx(r, "the_null")].sql_type, "int");

    // decimal precision/scale carried from the driver `Column`.
    let dec_col = &r.columns[idx(r, "the_decimal")];
    assert_eq!(dec_col.precision, Some(19u8));
    assert_eq!(dec_col.scale, Some(4u8));
}

/// 3.2 — `run` returns a `Vec<QueryResult>` of len 2 for a multi-statement batch.
/// This behavior is untested anywhere else; proving it is the single most
/// important thing ce1.7 adds. One batch, no `GO` — SQL Server emits two result
/// sets and `into_query_streams()` yields two streams.
#[tokio::test]
async fn multi_result_batch_returns_two_query_results() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping multi_result_batch_returns_two_query_results: MSSQL_* env not set");
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone());

    let sql = "SELECT CAST(1 AS int) AS a; SELECT CAST(2 AS int) AS b, CAST(3 AS int) AS c";
    let results = run(&cfg, &store, &ctx, sql).await.unwrap();

    assert_eq!(results.len(), 2, "two SELECTs → two result sets");

    assert_eq!(results[0].columns.len(), 1);
    assert_eq!(results[0].columns[0].name, "a");
    assert_eq!(results[0].rows.len(), 1);
    assert_eq!(results[0].rows[0][0], CellValue::Int(1));

    assert_eq!(results[1].columns.len(), 2);
    let names: Vec<&str> = results[1].columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["b", "c"]);
    assert_eq!(results[1].rows.len(), 1);
    assert_eq!(
        results[1].rows[0],
        vec![CellValue::Int(2), CellValue::Int(3)]
    );
}

/// 3.3 — a real (non-`CAST`) multi-row result set from a permission-free catalog.
/// `sys.databases` is readable by any login (no special perms). A restricted
/// SQL-auth login sees only databases it has permission for, but master + tempdb
/// are always visible — so assert on those, not a magic row count.
#[tokio::test]
async fn catalog_query_returns_multiple_rows_with_friendly_types() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!(
            "skipping catalog_query_returns_multiple_rows_with_friendly_types: MSSQL_* env not set"
        );
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone());

    let sql = "SELECT name, database_id, state_desc FROM sys.databases ORDER BY database_id";
    let results = run(&cfg, &store, &ctx, sql).await.unwrap();

    assert_eq!(results.len(), 1);
    let r = &results[0];

    let names: Vec<&str> = r.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["name", "database_id", "state_desc"]);
    // sysname = nvarchar(128); database_id = int.
    assert_eq!(r.columns[0].sql_type, "nvarchar");
    assert_eq!(r.columns[1].sql_type, "int");
    assert_eq!(r.columns[2].sql_type, "nvarchar");

    // A restricted login always sees at least master + tempdb.
    assert!(
        r.rows.len() >= 2,
        "expected >= 2 databases, got {}",
        r.rows.len()
    );
    // Every database_id cell is an Int.
    assert!(
        r.rows.iter().all(|row| matches!(row[1], CellValue::Int(_))),
        "every database_id must be Int"
    );
    // master and tempdb are always present by name.
    let has_name = |want: &str| {
        r.rows
            .iter()
            .any(|row| row[0] == CellValue::Text(want.into()))
    };
    assert!(has_name("master"), "sys.databases must include master");
    assert!(has_name("tempdb"), "sys.databases must include tempdb");
}

/// 3.4 — the `ExecutionContext` database switch, end-to-end (proves PLAN §4).
/// `master`/`tempdb` both always exist and are distinct, so the assertion holds
/// regardless of what `MSSQL_DATABASE` is. Also proves the default path (no
/// `with_database` → no `USE` → the connection's `;Database=`).
#[tokio::test]
async fn execution_context_switches_database() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping execution_context_switches_database: MSSQL_* env not set");
        return;
    };
    let q = "SELECT DB_NAME() AS db";

    // Primary proof: a pinned database is applied via `USE [db];`.
    let ctx_master = ExecutionContext::new(cfg.id.clone()).with_database("master");
    let ctx_tempdb = ExecutionContext::new(cfg.id.clone()).with_database("tempdb");
    let rm = run(&cfg, &store, &ctx_master, q).await.unwrap();
    let rt = run(&cfg, &store, &ctx_tempdb, q).await.unwrap();
    assert_eq!(rm[0].rows[0][0], CellValue::Text("master".into()));
    assert_eq!(rt[0].rows[0][0], CellValue::Text("tempdb".into()));

    // Secondary: the default context (no `with_database`) stays on the
    // connection's default database. This `==` is case-sensitive against the raw
    // `MSSQL_DATABASE` string — if the box normalizes DB name casing this may
    // need a case-insensitive compare on the live run.
    let ctx_default = ExecutionContext::new(cfg.id.clone());
    let rd = run(&cfg, &store, &ctx_default, q).await.unwrap();
    let want = cfg.default_database.as_deref().unwrap();
    assert_eq!(rd[0].rows[0][0], CellValue::Text(want.into()));
}

/// 3.5 — an empty result set is clean (not an error). Belt-and-suspenders edge.
#[tokio::test]
async fn empty_result_set_is_clean() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping empty_result_set_is_clean: MSSQL_* env not set");
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone());

    let results = run(&cfg, &store, &ctx, "SELECT CAST(1 AS int) AS a WHERE 1 = 0")
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].columns.len(), 1);
    assert!(results[0].rows.is_empty(), "no rows for a false WHERE");
}

// ---------------------------------------------------------------------------
// §4 — the typed-decode regression half (port of `typed_probe.rs`).
//
// This is the ONLY test that touches `mssql_client` directly. Allowed: the
// boundary invariant (PLAN §3 / CLAUDE.md) confines the driver away from the
// `app`/Svelte side, NOT away from `core`'s own tests. The app never uses the
// typed `get::<T>()` path, but CLAUDE.md mandates re-running the typed probe
// after any driver bump: a decode regression here returns a real `Err` naming
// the type that broke, where the untyped §3 path silently collapses to `Null`.
// ---------------------------------------------------------------------------

/// 4c — port of `typed_probe.rs`'s `get::<T>` matrix. Builds the connection the
/// driver way, reusing `core`'s own `build_connection_string` so we don't
/// duplicate the connection-string format.
#[tokio::test]
async fn driver_typed_decode_matrix() {
    use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping driver_typed_decode_matrix: MSSQL_* env not set");
        return;
    };

    let pw = store.get_password(&cfg.id).unwrap().unwrap();
    let conn_str = build_connection_string(&cfg, &pw);
    let config = mssql_client::Config::from_connection_string(&conn_str).unwrap();
    let mut client = mssql_client::Client::connect(config).await.unwrap();

    // value known → assert decode Ok AND equals expected.
    macro_rules! decode_eq {
        ($client:expr, $sql:expr, $ty:ty, $expected:expr) => {{
            let rows = $client
                .query($sql, &[])
                .await
                .expect(concat!("query: ", $sql));
            let mut seen = false;
            for r in rows {
                let row = r.expect("row error");
                let v = row.get::<$ty>(0).expect(concat!("DECODE ERR: ", $sql));
                assert_eq!(v, $expected, "decoded value mismatch: {}", $sql);
                seen = true;
                break;
            }
            assert!(seen, concat!("no rows: ", $sql));
        }};
    }
    // value non-deterministic (now) → assert decode Ok only.
    macro_rules! decode_ok {
        ($client:expr, $sql:expr, $ty:ty) => {{
            let rows = $client
                .query($sql, &[])
                .await
                .expect(concat!("query: ", $sql));
            let mut seen = false;
            for r in rows {
                let row = r.expect("row error");
                let _v: $ty = row.get::<$ty>(0).expect(concat!("DECODE ERR: ", $sql));
                seen = true;
                break;
            }
            assert!(seen, concat!("no rows: ", $sql));
        }};
    }

    decode_eq!(client, "SELECT CAST(42 AS int)", i32, 42);
    decode_eq!(
        client,
        "SELECT CAST(9000000000 AS bigint)",
        i64,
        9_000_000_000
    );
    decode_eq!(client, "SELECT CAST(1 AS bit)", bool, true);
    // A plain non-constant float (avoid a value clippy flags as ~PI/E); the
    // point is that `float` still decodes to `f64`, not the specific value.
    decode_eq!(client, "SELECT CAST(1.5 AS float)", f64, 1.5);
    decode_eq!(
        client,
        "SELECT CAST(N'héllo ☃' AS nvarchar(50))",
        String,
        "héllo ☃".to_string()
    );
    decode_eq!(
        client,
        "SELECT CAST(1234.5678 AS decimal(19,4))",
        Decimal,
        Decimal::from_str_exact("1234.5678").unwrap()
    );
    decode_eq!(
        client,
        "SELECT CAST(1234.5678 AS money)",
        Decimal,
        Decimal::from_str_exact("1234.5678").unwrap()
    );
    decode_eq!(
        client,
        "SELECT CAST('6F9619FF-8B86-D011-B42D-00C04FC964FF' AS uniqueidentifier)",
        Uuid,
        Uuid::parse_str("6F9619FF-8B86-D011-B42D-00C04FC964FF").unwrap()
    );
    decode_eq!(
        client,
        "SELECT CAST(0xDEADBEEF AS varbinary(8))",
        Vec<u8>,
        vec![0xDEu8, 0xAD, 0xBE, 0xEF]
    );
    decode_eq!(
        client,
        "SELECT CAST('2024-03-17 12:34:56.123' AS datetime2)",
        NaiveDateTime,
        NaiveDate::from_ymd_opt(2024, 3, 17)
            .unwrap()
            .and_hms_milli_opt(12, 34, 56, 123)
            .unwrap()
    );
    decode_eq!(
        client,
        "SELECT CAST('2024-03-17' AS date)",
        NaiveDate,
        NaiveDate::from_ymd_opt(2024, 3, 17).unwrap()
    );
    decode_eq!(
        client,
        "SELECT CAST('12:34:56' AS time)",
        NaiveTime,
        NaiveTime::from_hms_opt(12, 34, 56).unwrap()
    );
    // SYSDATETIMEOFFSET() (not a literal — error-241 footgun) is `now` → decode
    // succeeds into the driver's `DateTime<FixedOffset>`; the value is not pinned.
    // sql_variant is deliberately omitted: the driver strictly refuses a wrong
    // target type (a known, expected non-decode), which would only add noise here.
    decode_ok!(client, "SELECT SYSDATETIMEOFFSET()", DateTime<FixedOffset>);

    let _ = client.close().await; // best-effort, mirrors `run`.
}

// ---------------------------------------------------------------------------
// d28.2 — parameterized execution (`run_with_params`): typed bind vs raw-text.
//
// AC: "Typed params bind via `sp_executesql`; raw-text params splice." These
// tests prove the MECHANISM end-to-end against the box (skip-clean when
// `MSSQL_*` unset). The strong discriminator between bind and splice is d2:
// `WHERE x = @n` with a bind param can ONLY work as a real `sp_executesql`
// parameter — a client-side splice of an int name would fail "must declare the
// scalar variable @n".
// ---------------------------------------------------------------------------

/// Build a bind param (`Some(sql_type)`).
fn bind(name: &str, sql_type: SqlType, value: &str) -> ResolvedParam {
    ResolvedParam {
        name: name.into(),
        sql_type: Some(sql_type),
        value: value.into(),
    }
}

/// Build a raw-text param (`None` → spliced literally).
fn raw(name: &str, value: &str) -> ResolvedParam {
    ResolvedParam {
        name: name.into(),
        sql_type: None,
        value: value.into(),
    }
}

/// d1 — typed bind round-trips a spread of types through `sp_executesql`. Each
/// `SELECT @p AS c` returns the bound value decoded back through the untyped
/// `run` mapping path, proving the whole parse→SqlValue→bind→decode chain and
/// that the driver derived a correct type declaration from the value alone.
#[tokio::test]
async fn param_typed_bind_round_trips_all_types() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping param_typed_bind_round_trips_all_types: MSSQL_* env not set");
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone());

    // (sql_type, input value, expected CellValue) for a single `@p` bind.
    let cases: Vec<(SqlType, &str, CellValue)> = vec![
        (SqlType::Int, "12345", CellValue::Int(12345)),
        (SqlType::BigInt, "9000000000", CellValue::Int(9_000_000_000)),
        (
            SqlType::NVarChar,
            "héllo ☃",
            CellValue::Text("héllo ☃".into()),
        ),
        (
            SqlType::Decimal,
            "1234.5678",
            CellValue::Decimal("1234.5678".into()),
        ),
        (SqlType::Money, "12.34", CellValue::Decimal("12.34".into())),
        (
            SqlType::Date,
            "2024-03-17",
            CellValue::Date("2024-03-17".into()),
        ),
        (
            SqlType::UniqueIdentifier,
            "6F9619FF-8B86-D011-B42D-00C04FC964FF",
            CellValue::Uuid("6f9619ff-8b86-d011-b42d-00c04fc964ff".into()),
        ),
        (SqlType::Bit, "1", CellValue::Bool(true)),
    ];

    for (sql_type, value, expected) in cases {
        let params = [bind("@p", sql_type, value)];
        let results = run_with_params(&cfg, &store, &ctx, "SELECT @p AS c", &params)
            .await
            .unwrap_or_else(|e| panic!("{sql_type:?} bind of {value:?} failed: {e}"));
        // F5: a bound query returns exactly one result set.
        assert_eq!(results.len(), 1, "{sql_type:?}");
        assert_eq!(
            results[0].rows[0][0], expected,
            "{sql_type:?} bind of {value:?}"
        );
    }
}

/// d2 — a REAL bind, not a splice. `WHERE x = @n` filters via `sp_executesql`;
/// a client-side splice of an int-typed `@n` would raise "must declare the
/// scalar variable @n". Exactly one matching row proves the parameterized RPC.
#[tokio::test]
async fn param_bind_filters_via_sp_executesql_not_splice() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping param_bind_filters_via_sp_executesql_not_splice: MSSQL_* env not set");
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone());

    let sql = "SELECT x FROM (VALUES (1),(2)) t(x) WHERE x = @n";
    let params = [bind("@n", SqlType::Int, "2")];
    let results = run_with_params(&cfg, &store, &ctx, sql, &params)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].rows.len(), 1, "exactly one row matches x = 2");
    assert_eq!(results[0].rows[0][0], CellValue::Int(2));
}

/// e — raw-text splice reaches the sent SQL. `ORDER BY @dir` with `@dir` →
/// `x DESC` sorts descending, so the rows come back 3,2,1. No bind params → the
/// multi-result path (still one result set here).
#[tokio::test]
async fn param_raw_text_splice_orders_rows() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping param_raw_text_splice_orders_rows: MSSQL_* env not set");
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone());

    let sql = "SELECT x FROM (VALUES (1),(2),(3)) t(x) ORDER BY @dir";
    let params = [raw("@dir", "x DESC")];
    let results = run_with_params(&cfg, &store, &ctx, sql, &params)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    let got: Vec<CellValue> = results[0].rows.iter().map(|r| r[0].clone()).collect();
    assert_eq!(
        got,
        vec![CellValue::Int(3), CellValue::Int(2), CellValue::Int(1)]
    );
}

/// f — mixed bind + raw-text in one query. `TOP (@lim)` binds while `ORDER BY
/// @dir` splices; a bind param is present so this goes through the single-result
/// bind path. Descending order + a limit of 2 → rows [3, 2].
#[tokio::test]
async fn param_mixed_bind_and_raw_text() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping param_mixed_bind_and_raw_text: MSSQL_* env not set");
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone());

    let sql = "SELECT TOP (@lim) x FROM (VALUES (1),(2),(3)) t(x) ORDER BY @dir";
    let params = [bind("@lim", SqlType::Int, "2"), raw("@dir", "x DESC")];
    let results = run_with_params(&cfg, &store, &ctx, sql, &params)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    let got: Vec<CellValue> = results[0].rows.iter().map(|r| r[0].clone()).collect();
    assert_eq!(got, vec![CellValue::Int(3), CellValue::Int(2)]);
}

/// h — the `ExecutionContext` `USE` is applied BEFORE the parameterized call.
/// Pin the database to `tempdb` and select `DB_NAME()` alongside a bind param;
/// the reported database must be `tempdb`.
#[tokio::test]
async fn param_run_applies_use_before_parameterized_call() {
    let Some((cfg, store)) = env_connection() else {
        eprintln!("skipping param_run_applies_use_before_parameterized_call: MSSQL_* env not set");
        return;
    };
    let ctx = ExecutionContext::new(cfg.id.clone()).with_database("tempdb");

    let sql = "SELECT DB_NAME() AS db, @x AS x";
    let params = [bind("@x", SqlType::Int, "1")];
    let results = run_with_params(&cfg, &store, &ctx, sql, &params)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].rows[0][0], CellValue::Text("tempdb".into()));
    assert_eq!(results[0].rows[0][1], CellValue::Int(1));
}
