# billz-lpb — Connection reuse for schema introspection

**Bead:** billz-lpb (P2, feature) · **Date:** 2026-07-11 · **Status:** design approved

## Problem

Expanding a database in the object tree takes ~10s. Each schema query
(`list_databases` / `list_tables` / `list_views` / `list_columns`) goes through
`core::executor::run`, which **connects fresh** — a full TLS handshake + SQL
login + `USE [db]` — per call. A single DB expand fires `tables` + `views` in
parallel, so it pays ~two logins before any rows come back. The `sys.*` queries
themselves are milliseconds; the cost is the repeated login.

`SchemaCache` already dedups *repeat* expands, so this targets the **first**
expand of each object (and, as a bonus, makes Refresh fast too).

## Goal

Reuse one live connection per connection-id for schema introspection, so an
expand pays at most one amortized login instead of two fresh ones.

## Scope

**In:** connection reuse for the four schema fetchers and the `SchemaCache`
that wraps them.

**Out (deferred, not designed-out):**
- Editor query-runner reuse (`run` / `run_with_params` keep connecting fresh —
  avoids session-state bleed between user queries; decision recorded in the
  bead discussion).
- Connection pooling / true concurrency (the driver ships no pool type; a
  single-user tree never has two overlapping schema ops a pool would help).
- Streaming/incremental row delivery (the latency is upfront login, not row
  transfer, so streaming wouldn't help here).

## Driver constraints (verified against `mssql-client` 0.20.2)

- `Client` is **not `Clone`** and its query methods take `&mut self`, so an
  `Arc<Client>` cannot be shared for concurrent ops.
- A TDS connection is strictly **one request/response at a time** (no MARS); the
  `Client` even tracks a "dirty" (request-sent, response-unread) state. Two ops
  genuinely cannot overlap on one physical connection.

Conclusion: reuse means *one owned `Client` per connection, serialized by a
lock* — not sharing.

## Design

### New module: `core/src/session.rs`

```rust
pub struct SessionCache {
    // Outer std::Mutex guards only the map; the per-connection slot is an Arc
    // to a tokio::Mutex holding the (lazily connected, evictable) Client.
    sessions: std::sync::Mutex<
        HashMap<ConnectionId, Arc<tokio::sync::Mutex<Option<Client<Ready>>>>>,
    >,
}
```

- **Outer lock (`std::sync::Mutex`)**: get-or-insert the per-connection slot,
  clone the `Arc`, release. Critical section holds **no `.await`**.
- **Inner lock (`tokio::sync::Mutex<Option<Client>>`)**: the session. Held
  across the query `.await`, which serializes ops on that one connection (must
  be `tokio::sync::Mutex`, not `std`, because the guard crosses `.await` — the
  same guard-across-await rule `SchemaCache::get_or_fetch` already respects).
  `Option` = lazily connected (`None` until first use) and evictable (`None`
  again on failure / disconnect).

Public API (driver-free signature — no `mssql_client` type escapes):

```rust
impl SessionCache {
    pub fn new() -> Self;

    /// Reuse (or lazily open) the connection for `cfg.id`, apply `ctx`'s USE,
    /// run `sql`, and return every result set. Does NOT close the client.
    pub async fn run(
        &self,
        cfg: &ConnectionConfig,
        store: &dyn SecretStore,
        ctx: &ExecutionContext,
        sql: &str,
    ) -> Result<Vec<QueryResult>>;

    /// Drop the cached client for `id` (idempotent). Used on connection
    /// edit/delete, where creds/server may have changed.
    pub fn evict(&self, id: &ConnectionId);
}
```

`run` re-issues `USE [db]` **every call** — a reused session carries state
(current database, SET options), so applying the execution context per call is
mandatory, not optional.

### Reuse of executor internals

`session.rs` is a **second** module that touches `mssql_client`. To avoid
duplicating driver logic, promote executor's three private helpers to
`pub(crate)` and call them from `SessionCache`:

- `connect(cfg, store) -> Client<Ready>`
- `apply_use_statement(&mut Client, ctx)`
- `query_stream_to_result(stream) -> QueryResult`

Update `executor.rs`'s module doc from "the ONE module where `mssql-client` is
used" to "confined to core's `executor` / `session` modules." **Invariant
preserved:** still no `mssql_client` type in any public API or in the `app`
crate.

### Error handling: evict + one silent retry

`run` holds the connection's slot guard and applies a retry-once policy:

1. **Attempt** = ensure connected (connect iff the slot is `None`), apply
   `USE`, run the batch, return results — **without** closing.
2. On **any** `Err` from the first attempt: set the slot to `None` (drop the
   possibly-dirty client), then run the attempt a **second** time — which, with
   the slot now `None`, reconnects before running.
3. A second failure is surfaced as `CoreError`.

Rationale: a stale socket (laptop sleep, server restart, idle timeout) heals
transparently; a genuine failure fails again the same way, costing one wasted
login (rare — offline DBs are already greyed out by rqb.4).

The policy is a small generic helper so its control flow is unit-testable
without a live `Client`. It owns the **whole** "try → on error, reset → try
once more" cycle, including the reset between tries:

```rust
// Runs `attempt`; on error, runs `reset` then `attempt` once more.
// `attempt(fresh: bool)` connects when `fresh` is true (or the slot is None).
async fn with_retry<T>(
    mut attempt: impl AsyncFnMut(/* fresh */ bool) -> Result<T>,
    mut reset: impl FnMut(),
) -> Result<T>;
```

`run` supplies the real attempt (the step-1 closure over the slot guard) and a
`reset` of `*guard = None`. The exact closure signatures are an implementation
detail to finalize in the plan; the **semantics are fixed**: first error →
reset the slot → one reconnecting retry → surface the second error.

### Wiring: `SchemaCache` owns the `SessionCache`

- The four fetchers gain a `&SessionCache` parameter and call `session.run(...)`
  instead of the free `executor::run(...)`:
  `list_databases(session, cfg, store)`, `list_tables(session, cfg, store, db)`,
  etc.
- `SchemaCache` gains a `sessions: SessionCache` field; each cache method passes
  `&self.sessions` to its fetcher.
- **App command signatures are unchanged** — `state.schema.tables(&cfg,
  &state.secrets, &db)` still compiles as-is.
- `AppState` is unchanged: the `SessionCache` lives inside the managed
  `SchemaCache`.

### Lifecycle / eviction

| Trigger | Cached data | Session client |
| --- | --- | --- |
| **Refresh** (`refresh_schema` → `SchemaCache::invalidate_connection`) | cleared | **kept warm** (Refresh re-queries `sys.*` with no re-login — bonus speedup) |
| **Connection edit** (`save_connection`) | cleared | **evicted** (creds/server may have changed) |
| **Connection delete** (`delete_connection`) | cleared | **evicted** |

Add `SchemaCache::forget_connection(id)` = clear that connection's cached data
**and** `sessions.evict(id)`. Call it from `save_connection` and
`delete_connection`. This also fixes a latent pre-existing bug: editing a
connection currently leaves its stale schema cached.

`invalidate_connection` (Refresh) keeps its current "clear data only" behavior —
it must **not** evict the session, or Refresh would re-login.

## Data flow (DB expand, after)

```
Tree expands DB "Foo"
  → invoke list_tables + list_views (parallel from JS)
  → both hit SchemaCache.{tables,views}(cfg, store, "Foo")
  → both call SessionCache.run(cfg, store, ctx=USE[Foo], SQL)
  → serialize on Foo's connection's tokio::Mutex:
        login once (first call) → USE [Foo] → run tables query
        (reuse client)          → USE [Foo] → run views query
  → net ≈ one login + two fast queries  (was: two fresh logins)
```

## Testing

**Headless (no DB):**
- `with_retry`: on `Ok` first try, runs `attempt` once and never calls `reset`;
  on `Err` first try, calls `reset` once then `attempt` a second time; surfaces
  the second error. Counter-closure style, mirroring `schema::get_or_fetch`
  tests. Also assert `attempt` receives `fresh = true` on the retry.
- `SessionCache::evict` on an absent id is a no-op (idempotent).
- Existing `format_sql_type` / parser / `get_or_fetch` tests remain green.

The four free fetchers gain a `&SessionCache` parameter, so `schema.rs`'s own
env-gated live tests (`live_list_databases`, `live_list_columns*`) update to
construct a `SessionCache::new()` and pass it — mechanical, and they still
clean-skip when `MSSQL_*` is unset.

**Env-gated live (`MSSQL_*`, clean-skip when unset):**
- Two schema queries via one `SessionCache` on one connection both return
  correct data (reuse doesn't corrupt the session).
- After an explicit `evict`, the next call still succeeds (reconnect works).

**No frontend changes.** The existing loading spinner covers residual latency.

## Acceptance criteria

- A first DB expand issues **one** login (not two); verified by the reuse path
  running `tables` + `views` on a single client.
- Refresh re-queries `sys.*` **without** a re-login (warm client retained).
- Editing or deleting a connection evicts its cached client (no reuse with stale
  creds; no leaked client).
- `cargo fmt` + `cargo clippy` clean; `just verify` green; no `mssql_client` type
  in any `core` public API or in the `app` crate.

## Files touched

- `core/src/session.rs` — **new**: `SessionCache`, `with_retry`.
- `core/src/executor.rs` — `connect` / `apply_use_statement` /
  `query_stream_to_result` → `pub(crate)`; module-doc invariant wording.
- `core/src/schema.rs` — fetchers take `&SessionCache`; `SchemaCache` gains
  `sessions` field + `forget_connection`.
- `core/src/lib.rs` — export `SessionCache` (module wiring).
- `app/src/lib.rs` — `save_connection` / `delete_connection` call
  `forget_connection`.
