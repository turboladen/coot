# billz-85b — Session-only / prompt-at-connect password — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a connection be saved "don't remember password" — the password is prompted at connect, held in memory for the app session, and never written to the Keychain.

**Architecture:** A persisted `remember_password` flag on `ConnectionConfig`; a `SessionOverlaySecretStore` layering an ephemeral in-memory map over the existing `CachingSecretStore<KeychainSecretStore>` (get prefers the session value; `set_session_password` never touches the durable store); a `set_session_password` Tauri command; and a Svelte password-prompt modal that unlocks a session-only connection on activation.

**Tech Stack:** Rust (edition 2024, `core` + `app`/Tauri), Svelte 5 runes, TypeScript.

## Global Constraints

- **`CLAUDE.md`: secrets never touch disk in plaintext.** A session-only password lives ONLY in an in-memory `Mutex<HashMap>` for the process lifetime. `ConnectionConfig` gains no password field.
- The driver stays behind `core`; no `mssql_client::` type in `app`/`core` public API.
- Core ops take `&dyn SecretStore` (verified: `executor.rs:35`, `schema.rs:310+`), so `SessionOverlaySecretStore` slots in with **no core-signature changes**.
- `cargo fmt` + `cargo clippy` clean (warnings are errors); `just verify` green.
- `SqlValue` is `#[non_exhaustive]` (not touched here, but any new match needs a wildcard).
- Adding a `ConnectionConfig` field breaks all struct literals — Task 1 updates all 8 sites.

---

### Task 1: `ConnectionConfig.remember_password` field (core)

**Files:**
- Modify: `core/src/connection.rs` — the struct + `sample_config` test fixture + a new deserialize test + the `config_serde_round_trips_and_holds_no_password` guard (line 306).
- Modify: `core/src/connection_store.rs` — the `config` literal (line 107) + the `persisted_json_contains_no_password` guard (line 197).
- Modify (struct-literal fixups): `core/src/session.rs:146,166`, `core/tests/dev_box.rs:40`, `core/src/schema.rs:836`, `core/src/executor.rs:539`, `app/src/lib.rs:330`.

**⚠ Two existing disk-invariant guards WILL break** the moment the field is added: both assert `!s.to_lowercase().contains("password")`, and the new key lowercases to `rememberpassword`, which contains the substring `password`. Step 3 fixes both to a **quote-delimited** check `!…contains("\"password\"")` — the real password field (never present) would serialize as the JSON key `"password"`, whereas `"rememberpassword"` never contains quote-`password`-quote. This keeps the invariant tight while allowing the new metadata key.

**Interfaces:**
- Produces: `ConnectionConfig.remember_password: bool` (serde `rememberPassword`, default `true`).

- [ ] **Step 1: Write the failing test**

In `core/src/connection.rs` tests module, add:

```rust
#[test]
fn remember_password_defaults_true_when_absent() {
    // A config written before 85b (no rememberPassword key) must still load.
    let json = r#"{"id":"c1","name":"n","server":"h,1433","username":"u",
        "defaultDatabase":null,"encrypt":false,"trustServerCertificate":true}"#;
    let cfg: ConnectionConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.remember_password);
}

#[test]
fn remember_password_round_trips_false() {
    let mut cfg = sample_config();
    cfg.remember_password = false;
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(json.contains("\"rememberPassword\":false"));
    let back: ConnectionConfig = serde_json::from_str(&json).unwrap();
    assert!(!back.remember_password);
}
```

- [ ] **Step 2: Run — verify it fails**

Run: `cargo test -p billz-core remember_password`
Expected: FAIL — no field `remember_password` (compile error).

- [ ] **Step 3: Add the field + fix all struct literals**

In `core/src/connection.rs`, add to `ConnectionConfig` (after `trust_server_certificate`):

```rust
    /// `false` ⇒ session-only password (prompted at connect, held in memory,
    /// never written to the Keychain). Default `true` for back-compat with
    /// configs written before billz-85b. Metadata only — not a secret.
    #[serde(default = "default_true")]
    pub remember_password: bool,
```

Add `remember_password: true,` to every `ConnectionConfig { … }` literal:
`core/src/connection.rs` `sample_config` (~line 223), `core/src/session.rs:146` & `:166`, `core/tests/dev_box.rs:40`, `core/src/schema.rs:836`, `core/src/executor.rs:539`, `core/src/connection_store.rs:107` (the `config` helper), `app/src/lib.rs:330` (the `env_connection` test helper).

Then fix the two disk-invariant guards so the new metadata key doesn't trip them:
- `core/src/connection.rs:306` — `assert!(!s.to_lowercase().contains("\"password\""), "serialized: {s}");`
- `core/src/connection_store.rs:197` — `!raw.to_lowercase().contains("\"password\""),`

- [ ] **Step 4: Run — verify it passes**

Run: `cargo test -p billz-core remember_password && cargo test -p billz-core && cargo test -p billz-app`
Expected: PASS (new tests + no literal left unfixed).

- [ ] **Step 5: Commit**

```bash
git add core/src/connection.rs core/src/session.rs core/tests/dev_box.rs core/src/schema.rs core/src/executor.rs core/src/connection_store.rs app/src/lib.rs
git commit -m "85b: ConnectionConfig.remember_password flag (serde default true)

(includes tightening two disk-invariant guards to quote-delimited \"password\")

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `SessionOverlaySecretStore` (core)

**Files:**
- Modify: `core/src/connection.rs` — the decorator + unit tests.
- Modify: `core/src/lib.rs` — re-export `SessionOverlaySecretStore`.

**Interfaces:**
- Consumes: `SecretStore`, `ConnectionId`, `InMemorySecretStore` (tests).
- Produces: `SessionOverlaySecretStore<S: SecretStore>` with `new(inner: S)` and `set_session_password(&self, id: &ConnectionId, password: &str)`, plus a `SecretStore` impl.

- [ ] **Step 1: Write the failing tests**

In `core/src/connection.rs` tests module:

```rust
#[test]
fn session_overlay_prefers_session_over_inner() {
    let inner = InMemorySecretStore::default();
    let id = ConnectionId("c1".into());
    inner.set_password(&id, "durable").unwrap();
    let overlay = SessionOverlaySecretStore::new(inner);
    overlay.set_session_password(&id, "ephemeral");
    assert_eq!(overlay.get_password(&id).unwrap().as_deref(), Some("ephemeral"));
}

#[test]
fn session_overlay_set_session_password_never_reaches_inner() {
    let overlay = SessionOverlaySecretStore::new(InMemorySecretStore::default());
    let id = ConnectionId("c1".into());
    overlay.set_session_password(&id, "ephemeral");
    // Prove nothing was written to the durable inner store — read it directly.
    assert!(overlay.inner.get_password(&id).unwrap().is_none());
}

#[test]
fn session_overlay_set_password_writes_through_to_inner() {
    let overlay = SessionOverlaySecretStore::new(InMemorySecretStore::default());
    let id = ConnectionId("c1".into());
    overlay.set_password(&id, "durable").unwrap();
    assert_eq!(overlay.inner.get_password(&id).unwrap().as_deref(), Some("durable"));
}

#[test]
fn session_overlay_delete_clears_both_layers() {
    let inner = InMemorySecretStore::default();
    let id = ConnectionId("c1".into());
    inner.set_password(&id, "durable").unwrap();
    let overlay = SessionOverlaySecretStore::new(inner);
    overlay.set_session_password(&id, "ephemeral");
    overlay.delete_password(&id).unwrap();
    assert!(overlay.get_password(&id).unwrap().is_none());
    assert!(overlay.inner.get_password(&id).unwrap().is_none());
}

#[test]
fn session_overlay_falls_through_to_inner_when_no_session() {
    let inner = InMemorySecretStore::default();
    let id = ConnectionId("c1".into());
    inner.set_password(&id, "durable").unwrap();
    let overlay = SessionOverlaySecretStore::new(inner);
    assert_eq!(overlay.get_password(&id).unwrap().as_deref(), Some("durable"));
}
```

(The tests read `overlay.inner`, so keep the field `pub(crate)` or add a
`#[cfg(test)]` accessor — use `pub(crate) inner` to keep the tests direct.)

- [ ] **Step 2: Run — verify it fails**

Run: `cargo test -p billz-core session_overlay`
Expected: FAIL — `SessionOverlaySecretStore` undefined.

- [ ] **Step 3: Implement**

In `core/src/connection.rs`, after `CachingSecretStore`:

```rust
/// A [`SecretStore`] decorator that layers an ephemeral, in-memory **session**
/// password map over any inner store. `get_password` prefers a session password
/// (set via [`set_session_password`]); otherwise it falls through to the inner
/// (durable) store. This backs the "don't remember password" path (billz-85b): a
/// session-only password lives ONLY in this map for the process lifetime and is
/// NEVER written to the inner store / Keychain (`CLAUDE.md` disk invariant).
///
/// `set_password` still writes through to the inner store (the remember-on path);
/// `delete_password` clears both layers.
///
/// [`set_session_password`]: Self::set_session_password
pub struct SessionOverlaySecretStore<S: SecretStore> {
    session: Mutex<HashMap<String, String>>,
    pub(crate) inner: S,
}

impl<S: SecretStore> SessionOverlaySecretStore<S> {
    pub fn new(inner: S) -> Self {
        Self { session: Mutex::new(HashMap::new()), inner }
    }

    /// Store a password in the SESSION map only — never the durable inner store.
    pub fn set_session_password(&self, id: &ConnectionId, password: &str) {
        self.session.lock().unwrap().insert(id.0.clone(), password.to_string());
    }
}

impl<S: SecretStore> SecretStore for SessionOverlaySecretStore<S> {
    fn set_password(&self, id: &ConnectionId, password: &str) -> Result<()> {
        self.inner.set_password(id, password)
    }

    fn get_password(&self, id: &ConnectionId) -> Result<Option<String>> {
        if let Some(pw) = self.session.lock().unwrap().get(&id.0) {
            return Ok(Some(pw.clone())); // ephemeral session password wins
        }
        self.inner.get_password(id)
    }

    fn delete_password(&self, id: &ConnectionId) -> Result<()> {
        self.session.lock().unwrap().remove(&id.0);
        self.inner.delete_password(id)
    }
}
```

In `core/src/lib.rs`, add `SessionOverlaySecretStore` to the `pub use
connection::{…}` re-export list.

- [ ] **Step 4: Run — verify it passes**

Run: `cargo test -p billz-core session_overlay && just lint`
Expected: PASS; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add core/src/connection.rs core/src/lib.rs
git commit -m "85b: SessionOverlaySecretStore — ephemeral session password overlay

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: App wiring + `set_session_password` command

**Files:**
- Modify: `app/src/lib.rs` — `AppState.secrets` type; `setup` builder; `save_connection` branch; `set_session_password` command + `generate_handler!` registration; import.

**Interfaces:**
- Consumes: `SessionOverlaySecretStore`, `CachingSecretStore`, `KeychainSecretStore` (core).
- Produces: command `set_session_password(id: ConnectionId, password: String)`.

- [ ] **Step 1: Change the store type + builder**

In `app/src/lib.rs`: add `SessionOverlaySecretStore` to the `use billz_core::{…}`
import. Change the field type:

```rust
    secrets: SessionOverlaySecretStore<CachingSecretStore<KeychainSecretStore>>,
```

In `setup`, build it:

```rust
                secrets: SessionOverlaySecretStore::new(CachingSecretStore::new(KeychainSecretStore)),
```

- [ ] **Step 2: Branch `save_connection` on `remember_password` (incl. on→off cleanup)**

Two changes. First, extend `connect_changed` so flipping the remember flag drops
the warm client (the creds *source* changed even if the connection string didn't).
Capture `old` once:

```rust
    let old = state.connections.get(&cfg.id)?;
    let connect_changed = password.is_some()
        || old.as_ref().is_some_and(|o| {
            build_connection_string(o, "") != build_connection_string(&cfg, "")
                || o.remember_password != cfg.remember_password // 85b: creds source changed
        });
```

Second, replace the `if let Some(pw) = password { … }` secret-write block with a
branch that **clears any stale durable secret when remember is off** (the fix for
the on→off gap: otherwise the old Keychain entry survives and `get_password` falls
through to it, silently defeating "don't remember"):

```rust
    if cfg.remember_password {
        if let Some(pw) = password {
            state.secrets.set_password(&cfg.id, &pw)?; // durable → Keychain
        }
    } else {
        // Session-only: drop any prior Keychain entry (idempotent; also clears the
        // session map) BEFORE stashing the new session password.
        state.secrets.delete_password(&cfg.id)?;
        if let Some(pw) = password {
            state.secrets.set_session_password(&cfg.id, &pw); // memory only
        }
    }
```

(Order matters: `delete_password` clears both layers, so `set_session_password`
must run after it.)

- [ ] **Step 3: Add the command + register it**

After `delete_connection`:

```rust
/// Store a session-only password in memory for `id` (billz-85b). Never persisted.
/// Called by the UI's unlock prompt for a `rememberPassword=false` connection.
#[tauri::command]
async fn set_session_password(
    id: ConnectionId,
    password: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    state.secrets.set_session_password(&id, &password);
    Ok(())
}
```

Add `set_session_password,` to the `tauri::generate_handler![…]` list.

- [ ] **Step 4: Add an app-level test**

In the `app/src/lib.rs` tests module, add (mirrors the existing store-backed
tests; `SessionOverlaySecretStore` wraps `InMemorySecretStore` here):

```rust
#[test]
fn session_overlay_command_path_stores_retrievably() {
    let overlay = SessionOverlaySecretStore::new(InMemorySecretStore::default());
    let id = ConnectionId("c1".into());
    overlay.set_session_password(&id, "pw"); // what the command does
    assert_eq!(overlay.get_password(&id).unwrap().as_deref(), Some("pw"));
}
```

(Add `SessionOverlaySecretStore` to the test module's `use` if needed. Do NOT
assert on `overlay.inner` here — `inner` is `pub(crate)` and this test is in the
`billz_app` crate, so it isn't visible cross-crate (E0616); the non-durability
guarantee is already covered by Task 2's core tests.)

- [ ] **Step 5: Run gates**

Run: `just test` (or `cargo test`) — Rust suites pass; `just lint` clean; `cargo build -p billz-app` compiles (command wiring).
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib.rs
git commit -m "85b: overlay store in AppState + set_session_password command

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: UI plumbing — `api.ts` + `ConnectionForm`

**Files:**
- Modify: `app/ui/src/lib/api.ts` — `ConnectionConfig.rememberPassword`; `setSessionPassword` binding.
- Modify: `app/ui/src/lib/ConnectionForm.svelte` — init/persist `rememberPassword`; send the password whenever typed; remove the `TODO(phase1)`.

**Interfaces:**
- Produces: `ConnectionConfig.rememberPassword: boolean`; `setSessionPassword(id, password)`.

- [ ] **Step 1: `api.ts`**

Add the field to `ConnectionConfig`:

```ts
export type ConnectionConfig = {
  id: string;
  name: string;
  server: string;
  username: string;
  defaultDatabase: string | null;
  encrypt: boolean;
  trustServerCertificate: boolean;
  rememberPassword: boolean;
};
```

Add the binding (after `saveConnection`):

```ts
export const setSessionPassword = (id: string, password: string) =>
  invoke<void>("set_session_password", { id, password });
```

- [ ] **Step 2: `ConnectionForm` — init from seed, persist, always-send-when-typed**

- Seed the checkbox from the config: change
  `let rememberPassword = $state(true);` to
  `let rememberPassword = $state(seed?.rememberPassword ?? true);`
  and delete the two `// Wave B …` / `// TODO(phase1) …` comment lines.
- In `buildConfig()`, add `rememberPassword,` to the returned object.
- In both `onSave` and `onTest`, change the password guard so a typed password is
  always sent (the backend decides Keychain vs session by `cfg.rememberPassword`):
  replace `const pw = password !== "" && rememberPassword ? password : null;`
  with `const pw = password !== "" ? password : null;`
- Update the checkbox label copy to reflect both paths:
  `Remember password (store in Keychain)` →
  `Remember password in Keychain (else prompt each session)`.

- [ ] **Step 3: Verify gates**

Run: `just ui-check`
Expected: 0 errors/0 warnings (the new required `rememberPassword` field is
supplied by `buildConfig`; all `ConnectionConfig` producers compile).

- [ ] **Step 4: Commit**

```bash
git add app/ui/src/lib/api.ts app/ui/src/lib/ConnectionForm.svelte
git commit -m "85b: UI plumbing — rememberPassword field + setSessionPassword; form persists flag

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: UI unlock UX — prompt modal + locked gate (`App.svelte`)

**Files:**
- Create: `app/ui/src/lib/PasswordPrompt.svelte` — the unlock modal.
- Modify: `app/ui/src/App.svelte` — `unlocked`/`dismissed` sets, `lockedConn`/`showPrompt` derived, gate the databases load, wire the prompt + 🔒 banner, pass `onSessionUnlock` to `ConnectionForm`.
- Modify: `app/ui/src/lib/ConnectionForm.svelte` — add the `onSessionUnlock?` prop + call it after a session-only save (Step 5).

**Interfaces:**
- Consumes: `setSessionPassword` (`api.ts`); `conns` (`connections.svelte`); `SvelteSet` (`svelte/reactivity`).

- [ ] **Step 1: Create `PasswordPrompt.svelte`**

```svelte
<script lang="ts">
  // Session-only password prompt (billz-85b). window.prompt is unreliable in the
  // Tauri v2 WKWebView, so this is an inline modal mirroring ConnectionForm's
  // field pattern. Parent supplies the connection name + submit/cancel.
  let { name, onsubmit, oncancel }: {
    name: string;
    onsubmit: (password: string) => void;
    oncancel: () => void;
  } = $props();
  let password = $state("");
  let input = $state<HTMLInputElement>();
  // Focus the field on mount (svelte 5: bind:this is set before this runs after
  // the microtask; use an effect so it fires once the node exists).
  $effect(() => { input?.focus(); });
</script>

<!-- Escape cancels (mirrors TableNode's menu pattern). -->
<svelte:window onkeydown={(e) => { if (e.key === "Escape") oncancel(); }} />

<!-- Button backdrop (not a static div) so svelte-check a11y stays clean — same
     pattern as tree/TableNode.svelte's .menu-backdrop. -->
<button class="backdrop" aria-label="Cancel" onclick={oncancel}></button>
<div class="modal" role="dialog" aria-modal="true" aria-label="Unlock connection">
  <h3>Password for {name}</h3>
  <p class="hint">Session-only — held in memory until you quit, never saved.</p>
  <form onsubmit={(e) => { e.preventDefault(); if (password !== "") onsubmit(password); }}>
    <input type="password" bind:this={input} bind:value={password} placeholder="Password" />
    <div class="actions">
      <button type="submit" disabled={password === ""}>Unlock</button>
      <button type="button" onclick={oncancel}>Cancel</button>
    </div>
  </form>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 50;
    background: rgba(0, 0, 0, 0.25); border: none; padding: 0; cursor: default;
  }
  .modal {
    position: fixed; top: 30%; left: 50%; transform: translateX(-50%); z-index: 51;
    background: #fff; border: 1px solid #ccc; border-radius: 8px; padding: 1rem 1.2rem;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2); min-width: 18rem;
  }
  h3 { margin: 0 0 0.3rem; font-size: 0.95rem; }
  .hint { margin: 0 0 0.6rem; font-size: 0.75rem; color: #888; }
  input { width: 100%; box-sizing: border-box; padding: 0.3rem; margin-bottom: 0.6rem; }
  .actions { display: flex; gap: 0.4rem; justify-content: flex-end; }
  button { font-size: 0.85rem; cursor: pointer; }
</style>
```

- [ ] **Step 2: `App.svelte` — state + locked derived**

Add imports:

```ts
  import { SvelteSet } from "svelte/reactivity";
  import { setSessionPassword } from "./lib/api";
  import PasswordPrompt from "./lib/PasswordPrompt.svelte";
```

Add state near the other connection state:

```ts
  // billz-85b: connection ids unlocked this app-session (a session-only password
  // is set). Same lifetime as the backend session map (both empty on restart).
  const unlocked = new SvelteSet<string>();
  // Ids whose prompt the user dismissed (Cancel) — the modal hides but the
  // connection stays locked (load still gated); a 🔒 button re-shows it.
  const dismissed = new SvelteSet<string>();
  // The active connection, if it's session-only and not yet unlocked. Drives the
  // load gate AND the locked/🔒 UI. Independent of `dismissed` (dismissing hides
  // the modal, not the locked state).
  const lockedConn = $derived.by(() => {
    const c = conns.list.find((c) => c.id === conns.activeId);
    return c && !c.rememberPassword && !unlocked.has(c.id) ? c : null;
  });
  // Show the modal only while locked AND not dismissed.
  const showPrompt = $derived(!!lockedConn && !dismissed.has(lockedConn.id));
```

- [ ] **Step 3: Gate the databases load on `lockedConn`**

Change the cwt.10 load effect so a locked connection doesn't attempt a load
(which would fail with no password):

```ts
  $effect(() => {
    treeRefresh.nonce; // track: a Refresh re-issues the load
    if (lockedConn) return; // billz-85b: wait for unlock before hitting the DB
    loadDatabases(conns.activeId);
  });
```

- [ ] **Step 4: Render the prompt + a 🔒 reopen affordance**

Where the workspace renders (top of the editor/results section is fine), add the
modal (only when not dismissed) and a locked banner with a 🔒 reopen button (shown
when locked but the prompt is dismissed), so the user is never trapped:

```svelte
{#if showPrompt && lockedConn}
  <PasswordPrompt
    name={lockedConn.name}
    onsubmit={(pw) => unlock(lockedConn.id, pw)}
    oncancel={() => dismissed.add(lockedConn.id)}
  />
{:else if lockedConn}
  <div class="locked-note">
    🔒 {lockedConn.name} is locked (session-only password not entered).
    <button type="button" onclick={() => dismissed.delete(lockedConn.id)}>Enter password</button>
  </div>
{/if}
```

Add a minimal `.locked-note` style (small, muted banner) in App's `<style>`.

Add the handler:

```ts
  async function unlock(id: string, password: string) {
    await setSessionPassword(id, password); // await BEFORE marking unlocked
    dismissed.delete(id);
    unlocked.add(id); // re-derives lockedConn → null → load effect fires
  }
```

**Cancel** → `dismissed.add(id)`: the modal hides but `lockedConn` stays truthy, so
`loadDatabases` remains gated (no tree) and the 🔒 banner appears. Other ops the
user might trigger (Run, tree expansion) will surface the existing `Config("no
stored password…")` error — acceptable; the 🔒 banner is the recovery path. This is
a **conscious** choice (per spec §4): a session-only connection you can't unlock
doesn't silently half-work, and you can still switch to another connection.

- [ ] **Step 5: Seed `unlocked` on save-with-session-password**

So saving a session-only connection with a typed password doesn't immediately
prompt, mark it unlocked after the save resolves — with correct ordering (add to
`unlocked` only AFTER the backend `set_session_password` has run, i.e. after
`await save(...)`, so `lockedConn` never goes null before the backend holds the
password).

Concretely:
1. In `ConnectionForm.svelte` (extends Task 4), add an
   `onSessionUnlock?: (id: string) => void` prop (default no-op, like `onclose`).
   In `onSave` and `onTest`, AFTER `await save(cfg, pw)` succeeds, call it when the
   password was session-only:
   ```ts
   if (pw !== null && !rememberPassword) onSessionUnlock(cfg.id);
   ```
2. In `App.svelte`, where `<ConnectionForm … />` is rendered, pass
   `onSessionUnlock={(id) => unlocked.add(id)}`.

Fallback if this is skipped: harmless — the user simply gets one prompt via the
modal (the backend already has the password only if it was saved; if not, the
prompt is correct). But the ordering above is the intended, no-extra-prompt path.

- [ ] **Step 6: Verify gates**

Run: `just ui-check && just ui-test && just ui-build`
Expected: svelte-check 0 errors/0 warnings; bun tests pass; build clean.

- [ ] **Step 7: Manual smoke (live app)**

Run: `just dev`.
1. New connection, Remember **off**, enter server/user/password, Save → tree loads (session password used). Verify **no** Keychain entry: `security find-generic-password -s billz -a <id>` returns nothing.
2. Quit + relaunch → select that connection → 🔒 prompt appears → enter password → tree loads.
3. Cancel the prompt → the modal hides, a 🔒 banner appears, no DB load; clicking "Enter password" (or pressing Escape then reselecting) re-shows the prompt. You can still switch to another connection while dismissed.
4. A Remember-**on** connection still auto-connects (no prompt).
5. Delete the session-only connection → its session password is cleared (no leftover).

- [ ] **Step 8: Commit**

```bash
git add app/ui/src/lib/PasswordPrompt.svelte app/ui/src/App.svelte app/ui/src/lib/ConnectionForm.svelte
git commit -m "85b: session-password unlock prompt + locked-connection gate

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- Persisted `rememberPassword` flag → Task 1. ✓
- `SessionOverlaySecretStore` (get prefers session; `set_session_password` never durable; delete clears both) → Task 2. ✓
- App wiring + `save_connection` branch + `set_session_password` command → Task 3. ✓
- UI flag persist/init + always-send-typed-password → Task 4. ✓
- Locked-on-activation gate + prompt modal + unlock + 🔒/seed-on-save → Task 5. ✓
- Secrets never on disk → session map is memory-only; `set_session_password` never calls inner (Task 2 test asserts it). ✓
- Remember-on unchanged; delete clears both layers → Tasks 2/3. ✓

**Placeholder scan:** No TBD; every code step shows complete code; commands have expected output. Task 5 Step 5 offers a concrete primary approach + a stated fallback (not a placeholder — both are complete). ✓

**Type consistency:** `SessionOverlaySecretStore<S>::new(inner: S)` + `set_session_password(&ConnectionId, &str)` consistent across Tasks 2/3. `ConnectionConfig.remember_password` (Rust) ↔ `rememberPassword` (serde/TS) consistent across Tasks 1/4. Command `set_session_password(id: ConnectionId, password: String)` ↔ `setSessionPassword(id, password)` binding ↔ `invoke("set_session_password", {id, password})` (arg names match Rust) — Tasks 3/4. `lockedConn`/`unlocked`/`unlock` consistent in Task 5. ✓
