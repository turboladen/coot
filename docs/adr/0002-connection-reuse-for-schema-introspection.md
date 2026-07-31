# ADR-0002: Connection reuse for schema introspection; the query runner connects fresh

- **Status:** Accepted
- **Date:** 2026-07-11
- **Related:** [ADR-0006](0006-schema-cache-generation-guard.md) (the cache this sits behind); bead
  `billz-lpb`

## Context

Expanding a database in the object tree took roughly ten seconds. Each schema query
(`list_databases` / `list_tables` / `list_views` / `list_columns`) went through
`core::executor::run`, which **connects fresh** — a full TLS handshake, SQL login, and `USE [db]`
per call. A single expand fires `tables` and `views` in parallel, so it paid about two logins before
any rows came back. The `sys.*` queries themselves take milliseconds; the cost was the repeated
login.

Driver constraints, verified against `mssql-client` 0.20.2:

- `Client` is **not `Clone`** and its query methods take `&mut self`, so an `Arc<Client>` cannot be
  shared for concurrent operations.
- A TDS connection is strictly **one request/response at a time** (no MARS); the `Client` even
  tracks a request-sent/response-unread "dirty" state.

So reuse necessarily means *one owned `Client` per connection, serialized by a lock* — not sharing.

## Decision

**Schema introspection reuses one live client per connection; the query runner deliberately does
not.**

- `core/src/session.rs` holds a `SessionCache`. An outer `std::sync::Mutex` guards only the map
  (get-or-insert the per-connection slot, clone the `Arc`, release — the critical section holds no
  `.await`). An inner `tokio::sync::Mutex<Option<Client>>` is the session itself, held across the
  query `.await`, lazily connected (`None` until first use) and evictable (`None` again on failure).
- `SessionCache::run` re-issues `USE [db]` **every call**. A reused session carries state — current
  database and `SET` options — so applying the execution context per call is mandatory, not an
  optimization.
- **`executor::run` / `run_with_params` keep connecting fresh, on purpose**, to avoid session-state
  bleed between user queries. This is a deliberate asymmetry, not an oversight.
- `session.rs` becomes a **second** module permitted to touch `mssql_client`; rather than duplicate
  driver logic, it calls `executor`'s `connect` and `run_batch`, which are `pub(crate)` for that
  purpose. The boundary invariant is unchanged: no `mssql_client` type appears in any public API or
  in the `app` crate.

  > **Correction, 2026-07-31 (`billz-68o`).** This bullet originally claimed `connect`,
  > `apply_use_statement`, and `query_stream_to_result` were promoted. Only `connect` was;
  > `run_batch` was promoted instead, and `apply_use_statement` / `query_stream_to_result` are
  > private. Ground truth is `session.rs`'s import list. The error came from distilling this ADR
  > from `billz-lpb`'s *design spec* — what was planned — rather than from the code that shipped,
  > which chose a higher-level seam. It misled an implementer before being caught; see the
  > Consequences note below.
- **Error policy — evict and retry once.** On any error from the first attempt, drop the possibly
  dirty client and run the attempt again, which reconnects. A second failure surfaces as
  `CoreError`. A stale socket (laptop sleep, server restart, idle timeout) heals transparently; a
  genuine failure fails the same way, costing one wasted login.
- **Lifecycle:** Refresh clears cached data but keeps the client **warm** (so Refresh re-queries
  without a re-login); connection edit or delete **evicts** it, since credentials or server may have
  changed.

## Consequences

- **Positive:** a first expand pays one login instead of two, and Refresh pays none. Editing a
  connection also stopped leaving stale schema cached — a latent bug fixed by the same eviction
  path.
- **Positive:** the retry policy is a small generic helper, so its control flow is unit-testable
  without a live client.
- **Negative:** the driver-boundary rule had to relax from "the one module that touches
  `mssql-client`" to "confined to `executor` and `session`." Two modules is still auditable; a third
  should require justification.

  > **Third module admitted, 2026-07-31 (`billz-xi6.1`).** `plan::capture` is that third module. The
  > justification demanded above: estimated-plan capture must issue `SET SHOWPLAN_XML ON`, which is
  > *session state*, so it cannot run on a `SessionCache` client without poisoning every later query
  > on it. It therefore opens its own connection and closes it — the hazard this ADR identifies,
  > handled by the mitigation this ADR prescribes. Recorded in `executor.rs`'s module doc.

- **Negative / process:** this record was written from a design spec rather than from the shipped
  code, and one of its factual claims was wrong for months (see the correction above). An ADR is
  trusted *more* than a design doc precisely because it claims durability, so a factual error here
  costs more than the same error in a retired spec. Claims about what the code does should be
  verified against the code before an ADR is marked `Accepted`.
- **Negative / the important one:** session state is now a real hazard class. Anything that mutates
  connection state — `SET` options in particular — can bleed into later queries on a reused client.
  The runner's connect-fresh rule is the general mitigation, and any future feature that issues
  `SET` must either use its own connection or guarantee it restores state unconditionally.
