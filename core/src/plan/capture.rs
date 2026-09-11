//! The one server-touching file in `plan`. Compiles a query and never executes
//! it — [`capture_xml`] to get the estimated plan, [`compile_check`] to learn
//! only whether the server accepts it.
//!
//! Two hazards live here, both load-bearing, and both apply to either `SET`:
//!
//! 1. **`USE` must precede `SET SHOWPLAN_XML ON`.** With SHOWPLAN on, the server
//!    compiles but does not EXECUTE any statement — including `USE`. Flipping the
//!    order compiles the plan against the wrong database, with no error, which on
//!    a cross-tenant comparison yields 27 identical plans from the default
//!    database and a feature that confidently reports nothing differs. The
//!    ordering is made STRUCTURAL below: the context and the `ON` go out as one
//!    [`crate::executor::run_batch`] call, so there is no pair of statements to
//!    transpose.
//! 2. **SHOWPLAN is session state.** `SessionCache` reuses a live client and does
//!    not close it, so a leaked `ON` would make every later query silently return
//!    plan XML instead of running. Capture therefore uses its OWN connection and
//!    closes it — poisoning is impossible by construction rather than by care.

// Hazard 2 is what ADR-0002 decided,
// `docs/adr/0002-connection-reuse-for-schema-introspection.md`.

use crate::connection::{ConnectionConfig, SecretStore};
use crate::context::ExecutionContext;
use crate::error::{CoreError, Result};
use crate::result::{CellValue, QueryResult};

/// Must be the only statement in its batch.
const SHOWPLAN_ON: &str = "SET SHOWPLAN_XML ON";
const SHOWPLAN_OFF: &str = "SET SHOWPLAN_XML OFF";
const NOEXEC_ON: &str = "SET NOEXEC ON";
const NOEXEC_OFF: &str = "SET NOEXEC OFF";

/// Whether the server accepts `sql` under `ctx` — `Ok(())` when it compiles,
/// `Err` carrying the server's complaint when it does not.
///
/// Everything [`capture_xml`] validates except the plan: syntax, every table and
/// column resolved against the real schema, and types. Needs no SHOWPLAN
/// permission, only the read access the statement itself requires, so it reaches
/// databases where a plan cannot be had.
///
/// Nothing executes. `SET NOEXEC ON` compiles each statement that follows and
/// runs none, and the guarantee has the same shape as the one on `capture_xml`:
/// it holds provided the `ON` engaged, which `run_batch` returning `Err`
/// whenever it did not is what secures.
pub async fn compile_check(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    ctx: &ExecutionContext,
    sql: &str,
) -> Result<()> {
    // Our OWN connection, and the `USE` bound into the same call as the `ON` —
    // hazards 2 and 1, for the same reasons, since NOEXEC suppresses `USE` too.
    let mut client = crate::executor::connect(cfg, store).await?;

    let out = async {
        crate::executor::run_batch(&mut client, ctx, NOEXEC_ON).await?;

        // Never an early `?` between these two: the session must be restored
        // even when the statement is rejected, which is the common case here.
        let compiled = crate::executor::run_batch_no_use(&mut client, sql).await;
        let off = crate::executor::run_batch_no_use(&mut client, NOEXEC_OFF).await;

        compiled?;
        off?;
        Ok(())
    }
    .await;

    crate::executor::close_client(client).await;
    out
}

/// The raw ShowPlanXML document for `sql` under `ctx`, for `.sqlplan` export and
/// as the input to the (pure) parser. Plans get large, so this stays a separate
/// on-demand call rather than a field on
/// [`PlanCapture`](crate::plan::model::PlanCapture).
///
/// The query is COMPILED, not run — but state that dependency honestly rather
/// than as a promise: `sql` never executes *provided* SHOWPLAN actually engaged,
/// and what guarantees that is `run_batch(…, SHOWPLAN_ON)` returning `Err`
/// whenever it does not. A login denied SHOWPLAN gets that `Err`, observed
/// against a real server.
// The server checks SHOWPLAN against the databases holding the objects a
// statement NAMES, not against the session's current database. A denied login
// still captures `SELECT … FROM sys.objects` successfully, because a
// catalog-only statement never reaches the denial. So a probe built from
// `sys.*` reports a permission this path does not have, and
// `capture_fails_when_the_login_lacks_showplan` in `core/tests/dev_box.rs`
// targets a user table for that reason (billz-bkm).
pub async fn capture_xml(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    ctx: &ExecutionContext,
    sql: &str,
) -> Result<String> {
    // Our OWN connection — never SessionCache (hazard 2 in the module doc).
    let mut client = crate::executor::connect(cfg, store).await?;

    let out = async {
        // ORDER IS LOAD-BEARING (hazard 1): `run_batch` applies `ctx`'s `USE` and
        // THEN runs the batch, so the context is established before SHOWPLAN is on
        // — in one indivisible call. Everything after this point must go through
        // `run_batch_no_use`.
        crate::executor::run_batch(&mut client, ctx, SHOWPLAN_ON).await?;

        // Capture the result of the explained batch, then ALWAYS turn SHOWPLAN
        // off — never an early `?` between these two lines. `Drop` can't help: it
        // cannot await.
        let explained = crate::executor::run_batch_no_use(&mut client, sql).await;
        let off = crate::executor::run_batch_no_use(&mut client, SHOWPLAN_OFF).await;

        let results = explained?;
        // A failure to restore session state must not be swallowed: this
        // connection is about to be closed, but a silent failure here would hide
        // a real protocol problem.
        off?;

        first_xml_cell(results)
    }
    .await;

    // Always close — this connection existed only to hold the SHOWPLAN state.
    crate::executor::close_client(client).await;
    out
}

/// The plan arrives as a single-column result set holding the XML document.
///
/// Scans for the first non-empty string cell rather than indexing `[0][0]`.
/// Note this only ever sees the results of the EXPLAINED batch — the `SET`
/// batches' results are discarded by the caller — so the scan is not about
/// stepping over those. It buys two other things: `[0][0]` would PANIC on an
/// empty result set where we want [`CoreError::Query`], and the exact
/// result-set shape of an explained batch is not pinned against a real server.
/// Accepting `Text` as well as `Xml` is the same defense — the column is `xml` on
/// the wire, but we do not bet the feature on the driver decoding it to
/// `SqlValue::Xml`.
///
/// Returns only the FIRST document. Harmless for one batch — SHOWPLAN emits a
/// single document covering every statement in it — but silently lossy if
/// `GO`-split SQL (multiple batches) ever reaches this path.
// Every fixture `just dump-plans` writes goes through here, so the success path
// runs against a real server's result-set shape on each capture.
fn first_xml_cell(results: Vec<QueryResult>) -> Result<String> {
    for r in results {
        for row in r.rows {
            for cell in row {
                match cell {
                    CellValue::Xml(s) | CellValue::Text(s) if !s.is_empty() => return Ok(s),
                    _ => {}
                }
            }
        }
    }
    Err(CoreError::Query(
        "the server returned no execution plan (is SHOWPLAN permitted on this database?)".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{ConnectionId, InMemorySecretStore};
    use crate::test_support::env_connection;

    // ---- first_xml_cell: the only DB-free logic in this module ----

    fn result_of(rows: Vec<Vec<CellValue>>) -> QueryResult {
        QueryResult {
            columns: Vec::new(),
            rows,
            rows_affected: None,
        }
    }

    #[test]
    fn first_xml_cell_skips_a_leading_empty_result_set() {
        // A row-less result set must neither stop the search nor panic — `[0][0]`
        // indexing would do the latter.
        let results = vec![
            result_of(Vec::new()),
            result_of(vec![vec![CellValue::Xml("<ShowPlanXML/>".into())]]),
        ];
        assert_eq!(first_xml_cell(results).unwrap(), "<ShowPlanXML/>");
    }

    #[test]
    fn first_xml_cell_skips_empty_strings() {
        // An empty cell is not a plan — keep looking rather than returning "".
        let results = vec![result_of(vec![
            vec![CellValue::Xml(String::new())],
            vec![CellValue::Text(String::new())],
            vec![CellValue::Xml("<ShowPlanXML/>".into())],
        ])];
        assert_eq!(first_xml_cell(results).unwrap(), "<ShowPlanXML/>");
    }

    #[test]
    fn first_xml_cell_accepts_a_text_cell() {
        // The plan column is `xml` on the wire, but we do not bet the feature on
        // the driver decoding it to `SqlValue::Xml` rather than a string.
        let results = vec![result_of(vec![vec![CellValue::Text(
            "<ShowPlanXML/>".into(),
        )]])];
        assert_eq!(first_xml_cell(results).unwrap(), "<ShowPlanXML/>");
    }

    #[test]
    fn first_xml_cell_ignores_other_string_carrying_cells() {
        // The variants that MATTER here are the ones that also carry a String —
        // they are what a careless widening of the match arm would sweep in.
        // `Null`/`Int` cannot match by construction, so they would prove nothing.
        let results = vec![result_of(vec![vec![
            CellValue::BigInt("9007199254740993".into()),
            CellValue::Decimal("1234.5678".into()),
            CellValue::Binary("0xdeadbeef".into()),
            CellValue::Uuid("6f9619ff-8b86-d011-b42d-00c04fc964ff".into()),
            CellValue::Xml("<ShowPlanXML/>".into()),
        ]])];
        assert_eq!(first_xml_cell(results).unwrap(), "<ShowPlanXML/>");
    }

    #[test]
    fn first_xml_cell_returns_only_the_first_document() {
        // Pins the documented first-document-only contract, which is silently
        // lossy the day `GO`-split SQL reaches this path.
        let results = vec![
            result_of(vec![vec![CellValue::Xml("<first/>".into())]]),
            result_of(vec![vec![CellValue::Xml("<second/>".into())]]),
        ];
        assert_eq!(first_xml_cell(results).unwrap(), "<first/>");
    }

    #[test]
    fn first_xml_cell_with_no_document_is_a_query_error() {
        let results = vec![result_of(vec![vec![CellValue::Int(1)]])];
        let err = first_xml_cell(results).unwrap_err();
        assert!(matches!(err, CoreError::Query(_)), "got {err:?}");
        assert!(err.to_string().contains("no execution plan"), "got {err}");
    }

    #[test]
    fn first_xml_cell_of_nothing_at_all_is_a_query_error() {
        assert!(matches!(
            first_xml_cell(Vec::new()).unwrap_err(),
            CoreError::Query(_)
        ));
    }

    // ---- Send proof (type-check only; never polled, no DB) ----

    #[test]
    fn capture_xml_future_is_send() {
        // A later unit awaits this transitively inside `#[tauri::command]`s, whose
        // futures MUST be Send. Mirrors `session.rs`'s `run_future_is_send`: assert
        // it HERE in `core` so a regression fails with a legible local error rather
        // than a cryptic one in the `app` crate.
        fn assert_send<T: Send>(_: T) {}
        let store = InMemorySecretStore::default();
        let cfg = ConnectionConfig {
            id: ConnectionId("send-check".into()),
            name: "send-check".into(),
            server: "unused".into(),
            username: "unused".into(),
            default_database: None,
            encrypt: false,
            trust_server_certificate: true,
            remember_password: true,
        };
        let ctx = ExecutionContext::new(cfg.id.clone());
        assert_send(capture_xml(&cfg, &store, &ctx, "SELECT 1"));
    }

    // ---- env-gated live tests (clean runtime skip with no DEV box) ----

    // The named regression test for the session-state footgun.
    //
    // Be clear about what this does and does not prove. `capture_xml` opens and
    // closes its own connection, and SHOWPLAN is per-SESSION state, so a leak
    // cannot cross into the fresh connection `executor::run` opens — this test
    // cannot detect one today. It is a construction check: it fails if a future
    // refactor routes capture through a shared/reused client (a
    // `SessionCache`-style change) and the `OFF` stops being reached, which is
    // exactly the regression ADR-0002 warns about. It is not a leak detector.
    #[tokio::test]
    async fn showplan_is_off_after_a_capture() {
        let Some((cfg, store, _)) = env_connection() else {
            eprintln!("skipping showplan_is_off_after_a_capture: MSSQL_* env not set");
            return;
        };
        let ctx = ExecutionContext::new(cfg.id.clone());

        capture_xml(&cfg, &store, &ctx, "SELECT 1").await.unwrap();

        // A NORMAL run must still execute, not explain.
        let results = crate::executor::run(&cfg, &store, &ctx, "SELECT 1")
            .await
            .unwrap();
        // Assert the VALUE, not the shape: a leaked SHOWPLAN also yields exactly
        // one result set of exactly one row — the row holding the plan document —
        // so a `rows.len() == 1` check would pass under the very leak it names.
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].rows[0][0],
            CellValue::Int(1),
            "SHOWPLAN leaked — got plan XML, not a row"
        );
    }

    #[tokio::test]
    async fn capture_xml_returns_a_showplan_document() {
        let Some((cfg, store, _)) = env_connection() else {
            eprintln!("skipping capture_xml_returns_a_showplan_document: MSSQL_* env not set");
            return;
        };
        let ctx = ExecutionContext::new(cfg.id.clone());
        let xml = capture_xml(&cfg, &store, &ctx, "SELECT 1 AS a")
            .await
            .unwrap();
        assert!(xml.contains("ShowPlanXML"), "got {xml}");
        assert!(xml.contains("StmtSimple"), "got {xml}");
    }

    // The only automated check of footgun 1 — `USE` must be applied BEFORE
    // SHOWPLAN, or every plan compiles against the login's default database with
    // no error at all.
    //
    // Two captures under two DIFFERENT contexts, each asserting the document names
    // its own database; a single-context assertion would be vacuous when
    // `MSSQL_DATABASE` is the login default. The second database is chosen
    // dynamically so `MSSQL_DATABASE=master` cannot collapse it back to that form;
    // `master` and `tempdb` both exist and both have `sys.objects`. The SQL must
    // touch a real object: `Database=` is on `<Object>`, never on `StmtSimple`.
    #[tokio::test]
    async fn capture_xml_applies_the_use_before_showplan() {
        let Some((cfg, store, database)) = env_connection() else {
            eprintln!("skipping capture_xml_applies_the_use_before_showplan: MSSQL_* env not set");
            return;
        };
        const SQL: &str = "SELECT TOP 1 * FROM sys.objects";
        let other = if database.eq_ignore_ascii_case("master") {
            "tempdb"
        } else {
            "master"
        };

        let base = ExecutionContext::new(cfg.id.clone());
        for db in [database.as_str(), other] {
            let ctx = base.clone().with_database(db);
            let xml = capture_xml(&cfg, &store, &ctx, SQL).await.unwrap();
            assert!(
                xml.contains(&format!("Database=\"[{db}]\"")),
                "plan compiled against the wrong database (expected [{db}]): {xml}"
            );
        }
    }

    // A query that will not COMPILE is an error here — the fan-out layer is what
    // turns it into data.
    #[tokio::test]
    async fn a_binding_error_surfaces_as_a_query_error() {
        let Some((cfg, store, _)) = env_connection() else {
            eprintln!("skipping a_binding_error_surfaces_as_a_query_error: MSSQL_* env not set");
            return;
        };
        let ctx = ExecutionContext::new(cfg.id.clone());
        let err = capture_xml(&cfg, &store, &ctx, "SELECT no_such_column FROM sys.objects")
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Query(_)), "got {err:?}");
    }
}
