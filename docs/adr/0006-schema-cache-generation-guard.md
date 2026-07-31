# ADR-0006: Schema-cache generation guard — return but don't cache across a clear

- **Status:** Accepted
- **Date:** 2026-07-12
- **Related:** [ADR-0002](0002-connection-reuse-for-schema-introspection.md) (the session eviction
  that races this); bead `billz-rqb.7`

## Context

`SchemaCache::get_or_fetch` releases its `Mutex` across `fetch().await` — deliberately, since
holding it would serialize or deadlock concurrent expands — and then unconditionally inserts a
successful result:

```rust
if let Some(v) = map.lock().unwrap().get(&key) { return Ok(v.clone()); } // hit
let v = fetch().await;                                                   // no lock held
if let Ok(val) = &v { map.lock().unwrap().insert(key, val.clone()); }    // ← races
```

If a connection is edited or deleted while a `list_*` is in flight, `forget_connection` clears that
connection's entries and evicts its warm session — but the in-flight fetch, still talking to the
**old** server, returns afterwards and re-inserts pre-edit rows into the just-cleared cache. The
tree then shows stale schema until a manual Refresh.

Rare (it needs an edit concurrent with a subtree expand) and single-user, but a genuine correctness
gap surfaced during review of ADR-0002's work.

## Decision

**A global generation counter, stamped before each fetch and re-checked under the insert lock; a
result fetched across a clear is returned to its caller but not cached.**

- `SchemaCache` gains `generation: AtomicU64`.
- Every cache-clearing operation (`invalidate`, `invalidate_connection`, and therefore
  `forget_connection`) does `fetch_add(1, Release)` **before** clearing the maps.
- `get_or_fetch` loads the generation at entry, and after the fetch re-loads and compares it **while
  holding the target map's lock**, inserting only if unchanged. The value is returned either way.

**Why that is airtight, and why the ordering matters:** checking under the insert lock makes
"observed `gen0`" and "inserted" atomic with respect to that map. Since clears bump *before*
clearing, an insert that sees `gen0` necessarily precedes the clear's `lock().clear()`, which then
removes it; and if the bump is already visible, the insert is skipped. Either way no stale entry
survives. A plain "load, then separately lock and insert" would leave a sub-microsecond window on a
multi-threaded runtime.

`Acquire`/`Release` pair the load against the clears. `fetch_add` only ever increments, so two
clears net `+2` and can never ABA back to `gen0`. Cache **hits** are unguarded — a prior clear would
already have removed the entry.

**One global counter rather than per-connection**, because it is simplest with the shared generic
helper, and a cross-connection false invalidation merely costs a harmless re-fetch on a single-user
tool.

## Consequences

- **Positive:** no stale rows can survive a concurrent clear, and the in-flight caller still gets
  its result — no error, no retry, just an uncached value.
- **Positive:** fully deterministic to test. The fetch closure bumps the generation mid-`await`, so
  the race is exercised without timing or a database.
- **Negative:** a clear anywhere causes any in-flight fetch to skip caching, including for unrelated
  connections. Accepted: the cost is one re-fetch.
- **Negative:** the correctness argument depends on bump-**before**-clear and check-**under**-lock.
  Either reordered silently reopens the race, and neither is locally obvious — hence the comment in
  `get_or_fetch` and this record.
