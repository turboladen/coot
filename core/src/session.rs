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

/// One attempt at running `sql` on the connection's `slot`. `fresh` forces a new
/// connection (dropping any stale client in the slot); otherwise an existing
/// client is reused and a `None` slot is connected lazily. Re-issues `USE` every
/// call via `run_batch` (a reused session carries state).
///
/// A free `async fn`, NOT an async closure through a generic `AsyncFnMut` bound:
/// the latter produces a future the compiler cannot prove `Send` ("implementation
/// of `Send` is not general enough" — a higher-ranked-lifetime limitation), which
/// would break the schema Tauri commands (their futures must be `Send`).
async fn attempt(
    slot: &mut Option<Client<Ready>>,
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    ctx: &ExecutionContext,
    sql: &str,
    fresh: bool,
) -> Result<Vec<QueryResult>> {
    if fresh || slot.is_none() {
        *slot = Some(connect(cfg, store).await?);
    }
    let client = slot.as_mut().expect("slot is Some: just ensured above");
    run_batch(client, ctx, sql).await
}

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
        self.sessions
            .lock()
            .unwrap()
            .entry(id.clone())
            .or_default()
            .clone()
    }

    /// Drop the cached client for `id` (idempotent). Use on connection
    /// edit/delete, where creds/server may have changed.
    pub fn evict(&self, id: &ConnectionId) {
        self.sessions.lock().unwrap().remove(id);
    }

    /// Reuse (or lazily open) the connection for `cfg.id`, apply `ctx`'s `USE`,
    /// run `sql`, and return every result set — WITHOUT closing (that is the
    /// reuse). On any error, drop the possibly-dirty client and retry once on a
    /// fresh connection; if BOTH attempts fail, the ORIGINAL error is surfaced
    /// (the reconnect-retry's error tends to mask the real cause). If both fail,
    /// the slot keeps a possibly-dirty client; the next call's first attempt
    /// fails and its retry reconnects, so it self-heals within one call.
    ///
    /// REUSE HAZARD: the retry re-executes the WHOLE `sql` batch. That is safe
    /// for the idempotent, read-only `sys.*` introspection this serves today, but
    /// a future caller wiring this to non-idempotent statements (INSERT/UPDATE —
    /// the `billz-0gh.1` fan-out TODO) would double-apply side effects on a
    /// mid-batch failure. Keep this path for idempotent reads, or add idempotency
    /// handling before reusing it for writes.
    pub async fn run(
        &self,
        cfg: &ConnectionConfig,
        store: &dyn SecretStore,
        ctx: &ExecutionContext,
        sql: &str,
    ) -> Result<Vec<QueryResult>> {
        let slot = self.slot(&cfg.id);
        let mut guard = slot.lock().await; // serializes ops on this connection

        // Retry-once inline (a plain `match`, not a generic `AsyncFnMut` helper —
        // that defeats the `Send` proof the schema Tauri commands need; see
        // `attempt`). On the first error, `attempt(.., true)` drops the stale
        // client and reconnects before retrying. If the retry also fails, surface
        // the FIRST error — the retry ran on a fresh connection and its error
        // (e.g. a re-login failure) tends to mask the real cause the first
        // attempt saw (permission denied, offline database, …).
        match attempt(&mut guard, cfg, store, ctx, sql, false).await {
            Ok(v) => Ok(v),
            Err(first) => match attempt(&mut guard, cfg, store, ctx, sql, true).await {
                Ok(v) => Ok(v),
                Err(_second) => Err(first),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let a = sessions
            .run(&cfg, &store, &ctx, "SELECT CAST(1 AS int) AS a")
            .await
            .unwrap();
        assert_eq!(a[0].rows[0][0], CellValue::Int(1));
        let b = sessions
            .run(&cfg, &store, &ctx, "SELECT CAST(2 AS int) AS a")
            .await
            .unwrap();
        assert_eq!(b[0].rows[0][0], CellValue::Int(2));

        // After evict, the next call reconnects and still works.
        sessions.evict(&cfg.id);
        let c = sessions
            .run(&cfg, &store, &ctx, "SELECT CAST(3 AS int) AS a")
            .await
            .unwrap();
        assert_eq!(c[0].rows[0][0], CellValue::Int(3));
    }
}
