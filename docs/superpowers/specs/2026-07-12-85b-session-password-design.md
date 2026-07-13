# billz-85b — Session-only / prompt-at-connect password

**Bead:** billz-85b (P3, task) · **Date:** 2026-07-12 · **Status:** design approved

## Problem

The connection manager (cwt.3) implements remember-password = **true** only: the
password goes to the macOS Keychain via `KeychainSecretStore`. The `rememberPassword`
checkbox exists in the UI but is **UI-only** — not persisted, and unchecking it
just passes `null` to `save_connection` (nothing stored), so any later op fails
with `Config("no stored password…")`. There is no way to use a connection whose
password you don't want on disk. A `TODO(phase1)` marks this in the form.

## Goal

Support a **session-only password**: prompted at connect, held in memory for the
app session, **never written to the Keychain**. A connection can be marked
"don't remember"; using it prompts once per app-session.

**Invariant (`CLAUDE.md`):** secrets never touch disk in plaintext. A session-only
password lives only in an in-memory map for the process lifetime.

## Scope

**In:** a persisted `rememberPassword` flag on `ConnectionConfig`; a
`SessionOverlaySecretStore` layering session-memory over the existing
keychain-caching store; a `set_session_password` command; a UI password prompt
that unlocks a session-only connection on activation.

**Out:** changing the Keychain path for remember-on connections; encrypt-mode
work (`PLAN.md` §2); Test-before-save for a brand-new unsaved session-only
connection (Test still operates on a saved connection, as today); any secret
persisted to disk.

## Design

### 1. `ConnectionConfig` flag (core)

Add a metadata-only field (no password field is added — the disk invariant holds):

```rust
/// false ⇒ session-only password (prompted at connect, memory-only, never
/// Keychain). Default true for back-compat with configs written before 85b.
#[serde(default = "default_true")]
pub remember_password: bool,
```

Serializes as `rememberPassword`. Does **not** enter `build_connection_string`, so
the billz-lpb `connect_changed` introspection-retry comparison is unaffected.

### 2. `SessionOverlaySecretStore<S>` (core/src/connection.rs)

A `SecretStore` decorator layering an ephemeral session map over any inner store.
Clear separation: the overlay's job is "hold session-only passwords and prefer
them"; the inner `CachingSecretStore<KeychainSecretStore>` is unchanged (durable
source + its keychain read-cache).

```rust
pub struct SessionOverlaySecretStore<S: SecretStore> {
    session: Mutex<HashMap<String, String>>, // ephemeral, process-lifetime, never persisted
    inner: S,
}

impl<S: SecretStore> SessionOverlaySecretStore<S> {
    pub fn new(inner: S) -> Self { /* empty session map */ }
    /// Store a password in the session map ONLY (never the inner/durable store).
    pub fn set_session_password(&self, id: &ConnectionId, password: &str) { /* insert */ }
}

impl<S: SecretStore> SecretStore for SessionOverlaySecretStore<S> {
    // Prefer the ephemeral session password; else fall to the durable inner store.
    fn get_password(&self, id) -> Result<Option<String>> {
        if let Some(pw) = self.session.lock().unwrap().get(&id.0) { return Ok(Some(pw.clone())); }
        self.inner.get_password(id)
    }
    // remember-on path: write through to the durable inner store (Keychain).
    fn set_password(&self, id, pw) -> Result<()> { self.inner.set_password(id, pw) }
    // Clear both layers (idempotent, mirrors the inner stores).
    fn delete_password(&self, id) -> Result<()> {
        self.session.lock().unwrap().remove(&id.0);
        self.inner.delete_password(id)
    }
}
```

`Send + Sync` holds (a `Mutex`-guarded map + a `Send + Sync` inner), so
`&dyn SecretStore`/`&impl SecretStore` can cross the Tauri async `.await` as the
existing stores do.

### 3. App wiring

`AppState.secrets` becomes
`SessionOverlaySecretStore<CachingSecretStore<KeychainSecretStore>>` (built at
setup). Every core op already receives `&state.secrets`, so `get_password`
transparently prefers a session password — **no per-op change**.

`save_connection` (cfg now carries `remember_password`; no new param) — when a
password is supplied in the form:
- `remember_password == true` → `secrets.set_password` (write-through to Keychain, as today).
- `remember_password == false` → `secrets.set_session_password` (session map only).
- no password supplied (edit-without-change, or a session-only connection saved
  without typing one) → leave as-is; prompted at first use.

New command:
```rust
#[tauri::command]
fn set_session_password(id: ConnectionId, password: String, state: State<AppState>) -> AppResult<()> {
    state.secrets.set_session_password(&id, &password);
    Ok(())
}
```

### 4. UI — proactive unlock on activation (`App.svelte`)

`App` tracks an app-session `unlocked = new SvelteSet<string>()` (connection ids
with a session password this run — same lifetime as the backend session map, so
they stay in sync; both empty on restart).

A connection is **locked** when it is active, `cfg.rememberPassword === false`, and
`!unlocked.has(cfg.id)`. Selecting a connection already triggers `loadDatabases`
(which needs the password), so gate on locked:

- **Locked** → skip the load; show a **password modal** — a small component with a
  password `<input>` + Unlock / Cancel (reusing the connection form's field
  pattern; `window.prompt` is unreliable in the Tauri v2 WKWebView).
- **Unlock** → `set_session_password(id, pw)` → `unlocked.add(id)` → close modal →
  the load effect re-runs (tracked `unlocked` changed) → tree + ops proceed.
- **Cancel** → stays locked; ops surface the existing `Config("no stored
  password…")` message. A small **🔒 next to the active connection** reopens the
  prompt.

`unlocked` is also seeded when `save_connection` stashes a session password
(remember-off + a password typed → backend `set_session_password` + UI
`unlocked.add(id)`), so saving a session-only connection with a password doesn't
immediately re-prompt.

The edit form persists/loads `rememberPassword` correctly (fixing today's latent
reset-to-true-on-reload), and shows the password placeholder appropriately.

### 5. Deletion / edit coherence

`delete_connection` routes through the overlay → `delete_password` clears the
session map too. Editing remember-off → remember-on with a new password writes
through to the Keychain (`set_password`, unchanged).

## Testing

- **Core (Rust, `InMemorySecretStore` as inner):** `SessionOverlaySecretStore` —
  `get_password` prefers the session value over inner; `set_session_password` does
  NOT reach inner (inner still returns `None` for that id); `set_password` writes
  through to inner; `delete_password` clears both layers. `ConnectionConfig`
  deserializes with `rememberPassword` absent → defaults true (back-compat).
- **App:** `set_session_password` command stores retrievably (`get_password`
  round-trip through the overlay).
- **Gates + manual:** save a connection with Remember **off** + a password → tree
  loads (session pw used), no Keychain entry written; quit + relaunch → the
  connection is 🔒, expanding/running prompts → Unlock → works; a Remember-**on**
  connection still auto-connects; deleting a session-only connection clears its
  session password.

## Acceptance criteria

- A connection can be saved with `rememberPassword = false`; its password is never
  written to the Keychain (memory-only, process-lifetime).
- The choice persists (`ConnectionConfig.remember_password`); the edit form shows
  the correct checkbox on reload.
- Using a session-only connection prompts once per app-session (on activation);
  after unlock, all ops work; after restart, it re-prompts.
- A remember-on connection is unchanged (auto-connects from the Keychain).
- `cargo fmt`/`clippy` clean; `just verify` green; **no secret written to disk**;
  no `mssql_client` type in `app`/`core` public API.

## Files

- Modify: `core/src/connection.rs` — `remember_password` field;
  `SessionOverlaySecretStore` (+ unit tests).
- Modify: `core/src/lib.rs` — re-export `SessionOverlaySecretStore`.
- Modify: `app/src/lib.rs` — `AppState.secrets` type; `save_connection` branch;
  `set_session_password` command + registration.
- Modify: `app/ui/src/lib/api.ts` — `ConnectionConfig.rememberPassword`;
  `setSessionPassword` binding.
- Modify: `app/ui/src/App.svelte` — `unlocked` set, locked gate, prompt wiring, 🔒.
- Create: `app/ui/src/lib/PasswordPrompt.svelte` — the unlock modal.
- Modify: `app/ui/src/lib/ConnectionForm.svelte` — persist/load `rememberPassword`;
  remove the `TODO(phase1)`.
