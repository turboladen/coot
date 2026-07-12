# billz-lpb — Connection Reuse for Schema Introspection — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reuse one live `Client` per connection-id for the object tree's `sys.*`
queries, so a database expand pays one amortized login instead of two fresh ones.

**Architecture:** A new `core::session::SessionCache` owns a lazily-connected,
evictable `Client` per connection behind a per-connection `tokio::Mutex`. Schema
fetchers route through it instead of connecting per call. `executor::run`'s
connect-less core is extracted as `pub(crate) run_batch` and shared. The editor
runner is intentionally left connecting-per-call.

**Tech Stack:** Rust (edition 2024), `mssql-client` 0.20.2 (private to `core`),
`tokio::sync::Mutex`, Rust async closures (`AsyncFnMut`, stable ≥1.85).

## Global Constraints

- The driver stays behind `core`: no `mssql_client::` type in any public API or in
  the `app` crate. Driver usage confined to `core`'s `executor` + `session` modules.
- `cargo fmt` + `cargo clippy` clean before done. **Warnings are errors.**
- `SqlValue` / any driver enum match needs a wildcard arm (not touched here, but hold).
- Reused sessions carry state → `USE [db]` MUST be re-issued per call (via `run_batch`).
- `SessionCache::run`'s future MUST be `Send` — it is awaited inside the four
  `#[tauri::command]` schema commands (`SecretStore: Send + Sync`,
  `core/src/connection.rs:92-95`). This forces the `async move` closure in Task 2
  Step 3 and is guarded by the `run_future_is_send` test.
- Integration tests hit the real DEV box, gated on `MSSQL_SERVER`/`MSSQL_USER`/
  `MSSQL_PASSWORD`/`MSSQL_DATABASE`; they must clean-skip when unset.
- Crate name for cargo: `billz-core`.

---

### Task 1: Extract `run_batch` in the executor (behavior-preserving)

Split `executor::run` into `connect → run_batch → close` so the connect-less,
close-less middle can be reused by `SessionCache`. No behavior change.

**Files:**
- Modify: `core/src/executor.rs` (`run` body ~lines 30–53; `connect` ~line 128)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `pub(crate) async fn connect(cfg: &ConnectionConfig, store: &dyn SecretStore) -> Result<Client<Ready>>`
  - `pub(crate) async fn run_batch(client: &mut Client<Ready>, ctx: &ExecutionContext, sql: &str) -> Result<Vec<QueryResult>>`

- [ ] **Step 1: Extract `run_batch` and rewrite `run` to use it**

In `core/src/executor.rs`, replace the current `run` body (the
`let mut client = connect…` through `Ok(out)` block) with:

```rust
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

    let multi = client
        .query_multiple(sql, &[])
        .await
        .map_err(|e| CoreError::Query(e.to_string()))?;

    let streams = multi.into_query_streams();
    let mut out = Vec::with_capacity(streams.len());
    for stream in streams {
        out.push(query_stream_to_result(stream)?);
    }
    Ok(out)
}
```

- [ ] **Step 2: Make `connect` crate-visible**

Change the signature at `core/src/executor.rs` ~line 128 from:

```rust
async fn connect(cfg: &ConnectionConfig, store: &dyn SecretStore) -> Result<Client<Ready>> {
```

to:

```rust
pub(crate) async fn connect(cfg: &ConnectionConfig, store: &dyn SecretStore) -> Result<Client<Ready>> {
```

Leave `apply_use_statement` and `query_stream_to_result` private (only `run_batch`
uses them). Leave `run_with_params` unchanged.

- [ ] **Step 3: Verify the crate still compiles and existing tests pass**

Run: `cargo test -p billz-core executor`
Expected: PASS — the `cell_from_sql_value` / `column_meta` unit tests are
unaffected; the env-gated `run_returns_clean_query_result` clean-skips without
`MSSQL_*`.

- [ ] **Step 4: Lint + format**

Run: `just lint && just fmt`
Expected: clean, no warnings.

- [ ] **Step 5: Commit**

```bash
git add core/src/executor.rs
git commit -m "billz-lpb: extract executor::run_batch (connect-less run core)

Split run into connect → run_batch → close so the connect-less, close-less
middle is reusable. Expose connect + run_batch as pub(crate). No behavior change.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: `SessionCache` — reused connection with retry-once

New module owning one lazily-connected, evictable `Client` per connection-id.

**Files:**
- Create: `core/src/session.rs`
- Modify: `core/src/lib.rs` (add `pub mod session;` + `pub use session::SessionCache;`)
- Modify: `core/src/executor.rs` (module-doc line 1 wording only)

**Interfaces:**
- Consumes: `executor::connect`, `executor::run_batch` (Task 1); `ConnectionConfig`,
  `ConnectionId`, `SecretStore`, `ExecutionContext`, `QueryResult`, `Result`.
- Produces:
  - `pub struct SessionCache` with `pub fn new() -> Self`
  - `pub async fn run(&self, cfg: &ConnectionConfig, store: &dyn SecretStore, ctx: &ExecutionContext, sql: &str) -> Result<Vec<QueryResult>>`
  - `pub fn evict(&self, id: &ConnectionId)`
  - `SessionCache: Default`

- [ ] **Step 1: Write the failing `with_retry` tests**

Create `core/src/session.rs` with the header, imports, and a `tests` module
containing ONLY the `with_retry` tests for now:

```rust
//! Connection reuse for schema introspection (bead billz-lpb). Owns one live
//! `Client` per connection-id, lazily connected and reused across the tree's
//! `sys.*` queries so an expand pays one amortized login, not one per call.
//!
//! One of the two modules (with `executor`) where `mssql-client` is used — no
//! driver type appears in this module's public API (`PLAN.md` §3, `CLAUDE.md`).
//! Ops on a single connection serialize behind a `tokio::Mutex` (TDS is strictly
//! one-request-at-a-time; no MARS), which is correct, not a limitation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use mssql_client::{Client, Ready};
use tokio::sync::Mutex as AsyncMutex;

use crate::connection::{ConnectionConfig, ConnectionId, SecretStore};
use crate::context::ExecutionContext;
use crate::error::Result;
use crate::executor::{connect, run_batch};
use crate::result::QueryResult;

/// Run `attempt`; on error, run it once more. `attempt` receives `fresh`: `false`
/// on the first try (reuse the live client if any), `true` on the retry (drop the
/// stale one and reconnect). The second error is surfaced. Owning the whole
/// try→retry cycle here keeps the control flow unit-testable without a `Client`.
async fn with_retry<T, F>(mut attempt: F) -> Result<T>
where
    F: AsyncFnMut(bool) -> Result<T>,
{
    match attempt(false).await {
        Ok(v) => Ok(v),
        Err(_first) => attempt(true).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;
    use std::cell::Cell;

    #[tokio::test]
    async fn ok_first_try_runs_attempt_once() {
        let calls = Cell::new(0);
        let got: Result<i32> = with_retry(async |_fresh| {
            calls.set(calls.get() + 1);
            Ok(7)
        })
        .await;
        assert_eq!(got.unwrap(), 7);
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn err_first_try_retries_with_fresh_true() {
        let calls = Cell::new(0);
        let got: Result<i32> = with_retry(async |fresh| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                assert!(!fresh, "first attempt reuses");
                Err(CoreError::Query("stale socket".into()))
            } else {
                assert!(fresh, "retry forces a fresh connection");
                Ok(42)
            }
        })
        .await;
        assert_eq!(got.unwrap(), 42);
        assert_eq!(calls.get(), 2);
    }

    #[tokio::test]
    async fn second_error_is_surfaced() {
        let got: Result<i32> =
            with_retry(async |_fresh| Err::<i32, _>(CoreError::Query("boom".into()))).await;
        assert!(matches!(got.unwrap_err(), CoreError::Query(_)));
    }
}
```

Add to `core/src/lib.rs` — a `pub mod session;` line between `pub mod schema;`
and `pub mod types;`:

```rust
pub mod schema;
pub mod session;
pub mod types;
```

- [ ] **Step 2: Run the `with_retry` tests to verify they pass**

Run: `cargo test -p billz-core session::tests::`
Expected: PASS (3 tests). `with_retry` is exercised by tests; it is not yet used
in non-test code.

**Ordering trap — do NOT run `just lint` / `cargo clippy` between this step and
Step 3.** `with_retry` is a private non-test `async fn` used only by
`#[cfg(test)]` code until `run` (Step 3) calls it, so the lib target sees it as
`dead_code` → `-D warnings` fails. `cargo test` here is safe (test cfg uses it);
clippy is only valid once Step 3 lands. This is why the clippy gate is Step 7.

- [ ] **Step 3: Add `SessionCache` (struct, `new`, `slot`, `evict`, `run`)**

Insert this into `core/src/session.rs` after the `with_retry` fn (before the
`tests` module):

```rust
/// One connection's live client: lazily connected (`None`), evictable, and
/// locked across the query `.await` so ops on that connection serialize.
type Slot = Arc<AsyncMutex<Option<Client<Ready>>>>;

/// A cache of reused connections, keyed by [`ConnectionId`]. The outer
/// `std::Mutex` guards only the map (no `.await` held); each per-connection
/// [`Slot`] is an async mutex whose guard is held across the query.
#[derive(Default)]
pub struct SessionCache {
    sessions: StdMutex<HashMap<ConnectionId, Slot>>,
}

impl SessionCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get-or-create the per-connection slot. Short critical section, no `.await`.
    fn slot(&self, id: &ConnectionId) -> Slot {
        self.sessions.lock().unwrap().entry(id.clone()).or_default().clone()
    }

    /// Drop the cached client for `id` (idempotent). Use on connection
    /// edit/delete, where creds/server may have changed.
    pub fn evict(&self, id: &ConnectionId) {
        self.sessions.lock().unwrap().remove(id);
    }

    /// Reuse (or lazily open) the connection for `cfg.id`, apply `ctx`'s `USE`,
    /// run `sql`, and return every result set — WITHOUT closing (that is the
    /// reuse). On any error, drop the possibly-dirty client and retry once on a
    /// fresh connection. If BOTH attempts fail, the slot keeps a possibly-dirty
    /// client; the next call's first attempt fails and its retry reconnects, so
    /// it self-heals within one call.
    pub async fn run(
        &self,
        cfg: &ConnectionConfig,
        store: &dyn SecretStore,
        ctx: &ExecutionContext,
        sql: &str,
    ) -> Result<Vec<QueryResult>> {
        let slot = self.slot(&cfg.id);
        let mut guard = slot.lock().await; // serializes ops on this connection

        // `async move`: move the guard into the closure. `guard` is used only
        // here (never after `with_retry(..).await`), so moving is behaviour-
        // identical — the lock releases when the closure drops as `run` returns.
        // It is also REQUIRED for `Send`: an `async` (non-move) closure that
        // captures `&mut guard` and is passed through the generic `AsyncFnMut`
        // bound produces a future the compiler cannot prove `Send`, which would
        // break the four schema Tauri commands (their futures must be `Send`).
        with_retry(async move |fresh: bool| {
            // fresh=true (the retry) overwrites — and thus drops — the stale
            // client; connect also runs when the slot was never populated.
            if fresh || guard.is_none() {
                *guard = Some(connect(cfg, store).await?);
            }
            let client = guard.as_mut().expect("slot is Some: just ensured above");
            run_batch(client, ctx, sql).await
        })
        .await
    }
}
```

- [ ] **Step 4: Add the `evict` idempotency test + env-gated live test**

Add to the `tests` module in `core/src/session.rs`:

```rust
    use crate::connection::InMemorySecretStore;
    use crate::result::CellValue;

    #[test]
    fn evict_absent_id_is_noop() {
        let sessions = SessionCache::new();
        sessions.evict(&ConnectionId("never-connected".into())); // must not panic
    }

    #[test]
    fn run_future_is_send() {
        // The four schema Tauri commands await SessionCache::run transitively, and
        // those command futures MUST be Send (core/src/connection.rs:92-95). Assert
        // it HERE in `core` so a regression (e.g. dropping `async move`) fails with
        // a local error instead of a cryptic Send error in the `app` crate. Type-
        // check only — the future is never polled, so no runtime and no DB contact.
        fn assert_send<T: Send>(_: T) {}
        let sessions = SessionCache::new();
        let store = InMemorySecretStore::default();
        let cfg = ConnectionConfig {
            id: ConnectionId("send-check".into()),
            name: "send-check".into(),
            server: "unused".into(),
            username: "unused".into(),
            default_database: None,
            encrypt: false,
            trust_server_certificate: true,
        };
        let ctx = ExecutionContext::new(cfg.id.clone());
        assert_send(sessions.run(&cfg, &store, &ctx, "SELECT 1"));
    }

    /// Live `(cfg, store)` from `MSSQL_*`, or `None` (runtime skip, NOT #[ignore])
    /// — mirrors executor.rs / schema.rs.
    fn env_connection() -> Option<(ConnectionConfig, InMemorySecretStore)> {
        let server = std::env::var("MSSQL_SERVER").ok()?;
        let username = std::env::var("MSSQL_USER").ok()?;
        let password = std::env::var("MSSQL_PASSWORD").ok()?;
        let database = std::env::var("MSSQL_DATABASE").ok()?;
        let cfg = ConnectionConfig {
            id: ConnectionId("smoke".into()),
            name: "smoke".into(),
            server,
            username,
            default_database: Some(database),
            encrypt: false,
            trust_server_certificate: true,
        };
        let store = InMemorySecretStore::default();
        store.set_password(&cfg.id, &password).unwrap();
        Some((cfg, store))
    }

    #[tokio::test]
    async fn live_session_reuses_then_reconnects_after_evict() {
        let Some((cfg, store)) = env_connection() else {
            eprintln!("skipping live_session_reuses_then_reconnects_after_evict: MSSQL_* not set");
            return;
        };
        let sessions = SessionCache::new();
        let ctx = ExecutionContext::new(cfg.id.clone());

        // Two queries reuse the SAME client; both correct.
        let a = sessions.run(&cfg, &store, &ctx, "SELECT CAST(1 AS int) AS a").await.unwrap();
        assert_eq!(a[0].rows[0][0], CellValue::Int(1));
        let b = sessions.run(&cfg, &store, &ctx, "SELECT CAST(2 AS int) AS a").await.unwrap();
        assert_eq!(b[0].rows[0][0], CellValue::Int(2));

        // After evict, the next call reconnects and still works.
        sessions.evict(&cfg.id);
        let c = sessions.run(&cfg, &store, &ctx, "SELECT CAST(3 AS int) AS a").await.unwrap();
        assert_eq!(c[0].rows[0][0], CellValue::Int(3));
    }
```

- [ ] **Step 5: Update the executor module-doc invariant wording**

In `core/src/executor.rs`, change the first module-doc line from:

```rust
//! The executor — the ONE module where `mssql-client` is used in non-test code.
```

to:

```rust
//! The executor — one of two modules (with `session`) where `mssql-client` is
//! used in non-test code.
```

- [ ] **Step 6: Add the crate-root re-export**

In `core/src/lib.rs`, add after the `schema::{…}` re-export block (line ~41):

```rust
pub use session::SessionCache;
```

- [ ] **Step 7: Run headless tests + full lint/format**

Run: `cargo test -p billz-core session:: && just lint && just fmt`
Expected: PASS (5 headless tests: 3 × `with_retry`, `evict_absent_id_is_noop`,
`run_future_is_send`; the live test clean-skips). Clippy clean, no warnings —
this is the first point clippy is valid (see the Step 2 ordering trap).

- [ ] **Step 8: Commit**

```bash
git add core/src/session.rs core/src/lib.rs core/src/executor.rs
git commit -m "billz-lpb: SessionCache — reused connection with retry-once

One lazily-connected, evictable Client per connection-id behind a per-connection
tokio::Mutex. run() reuses the client (re-issuing USE per call) and, on any
error, drops it and retries once on a fresh connection. with_retry models the
policy headlessly; live reuse/reconnect covered by an env-gated smoke test.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Route schema introspection through `SessionCache`

Thread `&SessionCache` through the four fetchers, give `SchemaCache` its own
`SessionCache`, and add `forget_connection` (data + client eviction).

**Files:**
- Modify: `core/src/schema.rs` (imports; 4 fetcher signatures; `SchemaCache`
  struct + methods; add `forget_connection`; test module imports + live tests)

**Interfaces:**
- Consumes: `SessionCache` (Task 2).
- Produces:
  - Fetchers now take `sessions: &SessionCache` as their first parameter.
  - `SchemaCache` gains a private `sessions: SessionCache` field.
  - `pub fn forget_connection(&self, id: &ConnectionId)` on `SchemaCache`.
  - `SchemaCache::databases/tables/views/columns` and `invalidate*` signatures
    are UNCHANGED (so `app` commands are untouched).

- [ ] **Step 1: Write the failing `forget_connection` test**

Add to the `#[cfg(test)] mod tests` in `core/src/schema.rs` (near
`invalidate_connection_drops_only_that_connection`):

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p billz-core schema::tests::forget_connection_clears_data_for_that_connection`
Expected: FAIL — `no method named forget_connection`.

- [ ] **Step 3: Swap the fetcher import and add the four `&SessionCache` params**

In `core/src/schema.rs`, replace the top-level import (line ~33):

```rust
use crate::executor::run;
```

with:

```rust
use crate::session::SessionCache;
```

Then change each fetcher to take `sessions: &SessionCache` first and call
`sessions.run(...)` instead of `run(...)`:

```rust
pub async fn list_databases(
    sessions: &SessionCache,
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
) -> Result<Vec<DatabaseInfo>> {
    let ctx = ExecutionContext::new(cfg.id.clone());
    let results = sessions.run(cfg, store, &ctx, SQL_LIST_DATABASES).await?;
    parse_databases(&first_result(results)?)
}

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
```

- [ ] **Step 4: Give `SchemaCache` a `SessionCache` and pass it through**

Add the field to the struct (`core/src/schema.rs` ~line 366):

```rust
#[derive(Default)]
pub struct SchemaCache {
    databases: Mutex<HashMap<ConnectionId, Vec<DatabaseInfo>>>,
    tables: Mutex<HashMap<DbKey, Vec<TableInfo>>>,
    views: Mutex<HashMap<DbKey, Vec<ViewInfo>>>,
    columns: Mutex<HashMap<ColumnKey, Vec<ColumnInfo>>>,
    sessions: SessionCache,
}
```

Update each cache method's fetch closure to pass `&self.sessions` first:

```rust
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
```

- [ ] **Step 5: Add `forget_connection` (keep `invalidate_connection` data-only)**

Add this method to `impl SchemaCache` (right after `invalidate_connection`):

```rust
    /// Drop one connection's cached data AND its live session client. Use on
    /// connection edit/delete (creds/server may have changed) — distinct from
    /// [`Self::invalidate_connection`] (the Refresh path), which keeps the warm
    /// client so a Refresh re-queries `sys.*` without re-logging in.
    pub fn forget_connection(&self, id: &ConnectionId) {
        self.invalidate_connection(id);
        self.sessions.evict(id);
    }
```

- [ ] **Step 6: Fix the test module for the new signatures**

In `core/src/schema.rs`'s `#[cfg(test)] mod tests`, the env-gated live tests call
the free fetchers (now needing a `&SessionCache`) and use `executor::run` for DDL
setup. Add both imports inside the tests module (after `use super::*;`):

```rust
    use crate::executor::run;
```

Update the live-test fetcher call sites to construct and pass a session:

- In `live_list_databases_contains_online_master`, replace
  `let dbs = list_databases(&cfg, &store).await.unwrap();` with:

```rust
        let sessions = SessionCache::new();
        let dbs = list_databases(&sessions, &cfg, &store).await.unwrap();
```

- In `live_list_columns_decodes_pk_and_nullable_flags`, replace
  `let cols = list_columns(&cfg, &store, &db, "dbo", table).await.unwrap();` with:

```rust
        let sessions = SessionCache::new();
        let cols = list_columns(&sessions, &cfg, &store, &db, "dbo", table).await.unwrap();
```

- In `live_list_columns_unknown_table_is_empty`, replace
  `let cols = list_columns(&cfg, &store, &db, "dbo", "__no_such_table_xyz__")` with:

```rust
        let sessions = SessionCache::new();
        let cols = list_columns(&sessions, &cfg, &store, &db, "dbo", "__no_such_table_xyz__")
```

(The `run(&cfg, &store, &ctx, "…CREATE TABLE…")` setup/teardown calls stay as-is —
they use `executor::run`, now imported in the tests module.)

- [ ] **Step 7: Run it to verify the new test passes + suite is green**

Run: `cargo test -p billz-core schema::`
Expected: PASS — `forget_connection_clears_data_for_that_connection` passes; all
existing parser/`format_sql_type`/cache tests pass; live tests clean-skip.

- [ ] **Step 8: Lint + format**

Run: `just lint && just fmt`
Expected: clean. (No unused `use crate::executor::run;` at module top — it now
lives inside the tests module.)

- [ ] **Step 9: Commit**

```bash
git add core/src/schema.rs
git commit -m "billz-lpb: route schema introspection through SessionCache

Fetchers take &SessionCache and reuse the connection; SchemaCache owns one and
threads it through. forget_connection clears cached data AND evicts the live
client (connection edit/delete); invalidate_connection stays data-only so
Refresh keeps the warm client. App command signatures unchanged.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Evict the live client on connection edit/delete

Wire `forget_connection` into the two `app` commands so a reused client is never
kept with stale creds (and fixes the latent stale-cache-on-edit bug).

**Files:**
- Modify: `app/src/lib.rs` (`save_connection` ~lines 58–71; `delete_connection`
  ~lines 73–78)

**Interfaces:**
- Consumes: `SchemaCache::forget_connection` (Task 3), reached via `state.schema`.
- Produces: nothing new.

- [ ] **Step 1: Evict on save (edit) and delete**

In `app/src/lib.rs`, in `save_connection`, add a final line after
`state.connections.upsert(&cfg)?;`:

```rust
    state.connections.upsert(&cfg)?;
    // Creds/server may have changed on an edit — drop any cached client + schema
    // for this connection so the next use reconnects with fresh config (no-op for
    // a brand-new connection). Fixes stale-schema-after-edit too.
    state.schema.forget_connection(&cfg.id);
    Ok(())
```

In `delete_connection`, add after `state.secrets.delete_password(&id)?;`:

```rust
    state.secrets.delete_password(&id)?; // idempotent
    state.schema.forget_connection(&id); // drop cached client + schema
    Ok(())
```

- [ ] **Step 2: Verify the workspace builds and the full gate is green**

Run: `just verify`
Expected: PASS — Rust build/clippy/tests + frontend check/test/build all green.

- [ ] **Step 3: Manual smoke (live app)**

Run: `just dev`
Then, against `E4DEVSQL01`:
1. Expand a database in the tree — should populate in well under the old ~10s
   (one login, not two).
2. Hit Refresh (↻) — objects re-query quickly with no visible re-login.
3. Edit the connection (Edit → save), then expand again — still works (client was
   evicted and reconnects). Delete a throwaway connection — no error.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib.rs
git commit -m "billz-lpb: evict reused client on connection edit/delete

save_connection/delete_connection call SchemaCache::forget_connection so a
reused Client is never kept with stale creds, and a deleted connection leaks no
client. Also drops stale cached schema on edit.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- New `core/src/session.rs` `SessionCache` (outer std::Mutex map, inner
  tokio::Mutex<Option<Client>> slot, lazy connect, evict) → Task 2. ✓
- `run` reuses + re-issues `USE` per call, no close → Task 2 Step 3 (`run_batch`
  applies USE; `run` doesn't close). ✓
- Evict + one silent retry via `fresh` flag → Task 2 (`with_retry` + `run`). ✓
- Reuse of executor internals via `pub(crate)` → Task 1 (`connect` + `run_batch`;
  the spec's "three helpers" refined to these two). ✓
- Executor module-doc invariant wording → Task 2 Step 5. ✓
- `SchemaCache` owns `SessionCache`; fetchers take `&SessionCache`; app command
  signatures unchanged → Task 3. ✓
- Lifecycle: Refresh keeps warm client (`invalidate_connection` data-only); edit
  /delete evict (`forget_connection`) → Task 3 + Task 4. ✓
- Testing: headless `with_retry` + `evict` + `run_future_is_send` (Send guard) +
  `forget_connection`; env-gated live reuse/reconnect; live-test signature ripple
  → Tasks 2 & 3. ✓
- `lib.rs` module wiring + `SessionCache` export → Task 2 Steps 1 & 6. ✓
- No frontend changes → confirmed (no `app/ui` files touched). ✓

**Placeholder scan:** No TBD/TODO/"handle errors"/"similar to"; every code step
shows complete code; every command has an expected result. ✓

**Type consistency:** `SessionCache::{new,run,evict,slot}`, `Slot`, `with_retry`,
`executor::{connect,run_batch}`, `SchemaCache::{forget_connection,
invalidate_connection}` are referenced with identical names/signatures across
tasks. Fetchers' `sessions: &SessionCache`-first shape is consistent between
their definitions (Task 3 Step 3) and every call site (Task 3 Steps 4, 6). ✓
