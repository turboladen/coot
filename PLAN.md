# Personal SQL Server client for macOS — build plan

**Working name:** TBD (pick one before scaffolding). **Audience:** Claude Code + me. **Purpose:**
Replace the deprecated Azure Data Studio for my own day-to-day use against on-prem SQL Server DEV
boxes. Single user, single machine, no multi-user or distribution concerns. "Good enough for me"
beats "general-purpose."

This plan is the output of a design spike. The driver question is **closed** — a throwaway Rust
probe (see "Spike results" at the end) validated the chosen driver against the real DEV server on
both the typed and untyped read paths. Decisions below are made; rationale is included so they don't
get re-opened, not so they get re-debated.

---

## 1. Scope

**In (this is the whole product):**

- Save named connections; SQL-auth only (username/password, remember password, optional default
  database, encrypt=optional, trust cert=true).
- A SQL editor / runner: comments, run-the-selection-or-current-batch, results grid, editor contents
  persist across sessions.
- A **saved-query library** with **parameterization** (the headline feature — see §5).
- An object browser tree: Databases → Tables → Columns, plus Views. Nothing else at first.

**Explicitly out (do not build, do not gold-plate toward):**

- Entra ID / Windows / AAD auth. SQL auth only.
- The full SSMS node ontology (Synonyms, Programmability, Service Broker, Storage, Security…). Omit
  these nodes entirely — don't render disabled placeholders.
- Relationship / ER diagrams.
- "Statement under caret" precision (v1 runs the selection or the batch — see §6).
- Cross-database fan-out ("run across all `ESP_Nomad_*`"). Deferred, but the execution model is
  shaped so it's a later addition, not a rewrite (see §4).
- MCP server. Ruled out on purpose — the two core jobs (see a schema, hand-run curated SQL) are
  inherently human-in-the-loop and visual.

---

## 2. Locked technical decisions

| Decision                  | Choice                                                                  | Why                                                                                                                                                                                                                           |
| ------------------------- | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SQL Server driver         | **`mssql-client`** (praxiomlabs/rust-mssql-driver), v0.20+              | Only actively-maintained option; tiberius is quiet and its native-tls path is broken against SQL Server on macOS. This crate is **rustls-native → no OpenSSL, no macOS TLS pain**. Validated end-to-end against the real box. |
| GUI stack                 | **Tauri + Svelte**                                                      | The two hardest UI pieces are a real code editor and a virtualized results grid. Webland hands both over (CodeMirror/Monaco + TanStack Table). Reinventing either in a Rust-native GUI would be the whole project.            |
| Secret storage            | **macOS Keychain via the `keyring` crate**                              | Never store SQL passwords in plaintext config. Connection _metadata_ in config/SQLite; the password in Keychain, keyed by connection id.                                                                                      |
| Connection string default | `Encrypt=false;TrustServerCertificate=true`                             | Matches my environment ("encrypt optional, always trust cert"). Confirmed working against `E4-DEV-ESP-01`. Expose `strict` / `no_tls` as options later if ever needed.                                                        |
| Type rendering            | Column metadata for headers/types; `SqlValue` (via `get_raw`) for cells | Confirmed: the driver exposes `row.columns()` + `row.get_raw(i) -> Option<SqlValue>`. See §7 for the two-type-sources subtlety.                                                                                               |
| datetimeoffset target     | `chrono::DateTime<FixedOffset>`                                         | Both `FixedOffset` and `Utc` decode; `FixedOffset` preserves the zone.                                                                                                                                                        |

**Caveat to hold in mind:** `mssql-client` is fast-moving and AI-assisted with a single maintainer.
Mitigation is architectural, not a driver swap: **the driver lives behind the `core` crate's
boundary and never leaks into the UI** (§3). If it ever goes bad, only `core` changes.

---

## 3. Architecture: the shared spine

A Cargo **workspace**, two crates. The whole point is that the driver, the schema cache, and the
render layer meet in _one_ place — `core` — and the Tauri app is a thin shell.

```
workspace/
├─ core/          # pure Rust. no Tauri. the entire spine. unit-testable headless.
│  └─ src/
│     ├─ connection.rs   # Connection config + secrets (keyring). connect().
│     ├─ context.rs      # ExecutionContext { database }. THE key seam (§4).
│     ├─ executor.rs     # run SQL -> QueryResult (driver-agnostic). owns mssql-client.
│     ├─ result.rs       # QueryResult, ColumnMeta, CellValue. NO mssql_client types.
│     ├─ schema.rs       # sys.* introspection cache: databases/tables/columns/views.
│     ├─ types.rs        # TDS wire-token -> friendly SQL type name map.
│     ├─ query_store.rs  # saved queries + parameters (§5). persistence.
│     └─ error.rs
└─ app/           # Tauri + Svelte. thin. #[tauri::command]s delegate straight into core.
   └─ src/ (Rust commands) + ui/ (Svelte)
```

**The load-bearing boundary:** `core` owns `mssql-client` as a private dependency and exposes only
plain, serializable data types (`QueryResult`, `ColumnMeta`, `CellValue`). Tauri commands and Svelte
never see a `SqlValue` or a driver `Row`. This (a) insulates against the fast-moving driver, and (b)
means cells serialize to JSON cleanly for the grid.

Everything hangs off **one schema-introspection layer** (`schema.rs`). It is simultaneously: the
object tree's data source, the type source for typed parameters, and the lookup behind
right-click-run-scoped. Build it well and the tree, the runner, and the param editor all share it
instead of each re-querying `sys.*`.

---

## 4. Database is _execution context_, not an in-SQL parameter (important)

The DEV box has ~27 near-identical tenant databases (`ESP_Nomad_SE_DEV`, `ESP_Suntory_DEV`,
`ESP_Arnotts_Group_DEV`, …). The real workflow is running the _same_ query against _different_
tenant DBs. So which database you're in is a first-class **input to execution**, not a value spliced
into SQL and not something baked permanently into the connection.

You cannot bind a database name and cannot `USE @db`. So model it as context the executor applies:

```rust
struct ExecutionContext {
    connection_id: ConnectionId,
    database: Option<String>,   // executor issues `USE [database];` before the batch
}

// executor.rs
async fn run(ctx: &ExecutionContext, sql: &str, params: &[BoundParam]) -> Result<Vec<QueryResult>>;
//                                                                            ^ Vec: a batch can
//                                                                              return multiple results
```

Consequences to bake in now (cheap now, painful to retrofit):

- A saved query stores a **target database** (or "current"), separate from its connection.
- The tree's "current database" and the runner's target are the same `ExecutionContext` concept, set
  in one place.
- **Future fan-out is then just a loop over contexts** —
  `for db in matching_dbs { run(ctx.with_db(db), …) }` — not string surgery. That's why this is
  deferred, not designed-out.

---

## 5. Parameterization (the headline feature)

Steal Paw / RapidAPI's model: a parameter is a **typed, reusable, remembered** value, not
find-and-replace text. Two substitution mechanisms, discriminated by **whether the param has a
declared SQL type**:

- **Bind param** (has a SQL type) → real `sp_executesql` parameter. Safe, correctly typed. SQL
  Server _requires_ a type declaration (`@cust int`) to bind, so "typed at the UI level" isn't a
  preference — the protocol demands it. Covers the 80% case ("change the customer id").
- **Raw-text fragment** (no SQL type) → string-spliced before send. Covers table names, dynamic
  `ORDER BY`, `TOP @n`, whole clauses. Injectable by nature — **render it visually loud/unsafe** in
  the UI. Fine here: the only person I can hurt is me, on DEV.

```rust
struct Param {
    name: String,               // @cust
    sql_type: Option<SqlType>,  // Some -> bind (typed widget); None -> raw text (unsafe)
    last_value: Option<String>, // remember-last-value: the whole point of "run it again"
    scope: ParamScope,          // Global | Session | Local(query)
}
```

Design rules:

- **Remember-last-value is the feature.** Second run should be _one click_: if every param already
  has a value, just run; only prompt for genuinely-unset ones.
- **Session scope** = "I'm on customer 12345 all afternoon." Set `@cust` once at session level;
  every saved query referencing it just works, no per-query prompting. Three tiers: Global defaults
  (`@today`) < Session values < per-query Local.
- **Auto-type from the catalog.** When building a scoped query off a table, pre-fill param types
  from `schema.rs` (`sys.columns`+`sys.types`) — "typed at the UI level" becomes "typed
  automatically from the schema, editable to override." The tree already knows every type.
- Keep the UI type set **small**: int, bigint, nvarchar, bit, date, datetime2, decimal,
  uniqueidentifier, money — the things you filter by, not all 80 SQL types.

**Saved queries vs tabs — keep them separate concepts:**

- **Tabs = scratch.** Ephemeral, autosaved so nothing is ever lost (this is "persist across
  sessions" from the original ask).
- **Saved queries = intentional.** A named, searchable library you _promote_ things into. Its own
  home in the UI, not reconstructed by scrolling old tabs.

---

## 6. SQL runner semantics

- **What runs:** if there's a selection → run the selection. Otherwise → run the **current batch**
  (text between `GO` separators; whole document if there are none). _Not_ "the statement under the
  cursor" — T-SQL statement terminators are optional, so cursor-precision needs real lexing.
  Deferred to v2 (`sqlparser-rs` MsSql dialect, incomplete — evaluate later).
- `GO` is a client batch separator, not T-SQL — the runner splits on it; it is never sent.
- A batch can return **multiple result sets** and/or `PRINT`/messages → `Vec<QueryResult>` plus a
  messages channel. Grid shows result tabs; a messages pane shows PRINT/errors.
- No guardrails on DML/DDL. It runs what I give it. Correct for a personal DEV tool.

---

## 7. Object tree

- **v1 nodes only:** Server → Databases → Tables → Columns, plus Views. Omit every other SSMS node
  type (don't render placeholders).
- **Columns show:** name, type, nullable, and a PK/FK marker. One extra join, high value.
- **Lazy-load children on expand.** server × dbs × tables × columns is far too much to walk eagerly.
  Cache in `schema.rs`; provide an explicit **Refresh** (I do DDL on DEV and want to see new objects
  immediately).
- **Type source ≠ the runner's type source (the subtlety the spike exposed):**
  - The **runner** gets types from result-set metadata, where `type_name` is a **TDS wire token** —
    `IntN`, `Int4`, `MoneyN`, `Guid`, `BigVarBinary`, `NVarChar`. Not user-facing. `core::types`
    maps these → friendly names (`int`, `money`, `uniqueidentifier`, `varbinary`, `nvarchar`). ~a
    dozen entries. Note the token also encodes nullability (`IntN` vs `Int4`) — ignore that and use
    the separate `nullable` bool.
  - The **tree** gets types from `sys.columns` JOIN `sys.types` → canonical `int` / `decimal(19,4)`
    / `nvarchar(50)` with real precision/scale/length. Use this, not wire tokens. Quirk: `nvarchar`
    `max_length` is in **bytes** (char length = /2); `-1` means `MAX`.
- **Grey out non-`ONLINE` databases** using `state_desc` from `sys.databases` — enumerating tables
  in a `RESTORING`/`OFFLINE` db errors on expand.
- **Right-click a table → run a saved query scoped to it** (`@table` pre-filled from the node). This
  is the payoff of the tree and the query library sharing one data model — it's what makes this
  better than DataGrip _for me_.

---

## 8. Core data models (sketch)

Illustrative, not final. Real driver API is `mssql-client` 0.20 (`row.columns()`,
`row.get_raw(i) -> Option<SqlValue>`;
`Column { name, type_name, nullable, precision, scale,
max_length }`).

```rust
// result.rs — driver-agnostic. crosses the Tauri/JSON boundary. serde-serializable.
struct QueryResult {
    columns: Vec<ColumnMeta>,
    rows: Vec<Vec<CellValue>>,
    rows_affected: Option<u64>,
}
struct ColumnMeta {
    name: String,
    sql_type: String,       // FRIENDLY, mapped from wire token via core::types
    nullable: bool,
    precision: Option<u8>,
    scale: Option<u8>,
}
// Owned by core (mirrors SqlValue but is OURS). serde tag lets Svelte right-align numbers,
// distinguish NULL, hex-render binary, etc.
enum CellValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Decimal(String),        // string to avoid precision loss over JSON
    Text(String),
    Uuid(String),
    Date(String), Time(String), DateTime(String), DateTimeOffset(String),
    Binary(String),         // "0xdeadbeef"
    Xml(String),
}
// map: mssql_client::SqlValue -> CellValue lives in executor.rs. This is `render_cell`
// from the spike, promoted into core.
```

---

## 9. Build order (riskiest-first; driver risk already retired)

- **Phase 0 — core spine, headless.** Workspace + `core`. Connection config + Keychain.
  `executor::run` returning `QueryResult`. Port the spike's `render_cell` → `SqlValue→CellValue`.
  Port the wire-token→friendly map. Unit-test against the DEV box with no UI. _Exit:_ can run
  arbitrary SQL from a test and get clean `QueryResult`s.
- **Phase 1 — the MVP that replaces ADS.** Tauri shell. Connection manager UI (save/edit, Keychain).
  SQL editor (CodeMirror: comment-toggle, selection). Run selection-or-batch (§6). Results grid
  (TanStack, virtualized) reading `CellValue`. Tab autosave. _Exit:_ I stop opening ADS for basic
  querying.
- **Phase 2 — object tree.** `schema.rs` cache. Databases→Tables→Columns + Views, lazy-load,
  Refresh, `state_desc` greying. Double-click table → `SELECT TOP 1000` into a new tab.
- **Phase 3 — saved queries + parameterization (§5).** Saved-query library UI. Bind + raw-text
  params, remembered values, session scope, auto-type-from-catalog. Database-as-target on saved
  queries. Right-click-run-scoped from the tree.
- **Later (not now):** cross-tenant fan-out (loop `ExecutionContext` over matching DBs);
  relationship diagrams; additional tree node types; export to CSV; query history.

---

## Appendix — footguns found during the spike

- datetimeoffset literals: `'...T...+05:30'` with a space throws server error 241. Use
  `SYSDATETIMEOFFSET()` or a space-free ISO string. (This was a _literal_ bug, not the driver.)
- `sql_variant` → the driver strictly refuses a wrong target type
  (`type mismatch: expected
  String, got INT`) rather than silently coercing. This is _good_ — a
  real type system. Rare in practice; handle via `get_raw`/`SqlValue` if ever needed.
- `SqlValue` is `#[non_exhaustive]` → every `match` needs a wildcard arm.
- `Decimal` values cross JSON as **strings** to avoid f64 precision loss.
- MONEY and DECIMAL both surface as `SqlValue::Decimal`; only the column wire-token (`MoneyN` vs
  `DecimalN`) distinguishes them. Header from metadata, cell from value.

## Appendix — carry-over assets

The two spike binaries (`main.rs` typed probes, `bin/dynamic.rs` untyped dump) are the seed of
`core`: the connection-string builder, `render_cell`, and the column-introspection loop all graduate
directly into `executor.rs` / `result.rs`. Keep them as `core`'s first integration tests against the
DEV box.
