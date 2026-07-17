<script lang="ts">
  import { untrack } from "svelte";
  import type { ConnectionConfig } from "./api";
  import { testConnection } from "./api";
  import { save } from "./connections.svelte";
  import { X } from "./icons";

  // `editing` = the config to edit, or null for a new connection. The parent
  // wraps this in {#key} so a new target remounts the form (fields re-init).
  // `onSessionUnlock` (85b): called with the connection id after a session-only
  // save (remember off + a typed password), so App marks it unlocked and doesn't
  // re-prompt. Real no-op default so it's always safe to call.
  let {
    editing,
    onclose,
    onSessionUnlock = () => {},
  }: {
    editing: ConnectionConfig | null;
    onclose: () => void;
    onSessionUnlock?: (id: string) => void;
  } = $props();

  // One-time snapshot of the prop to seed the fields below. `untrack` makes the
  // "capture initial value only" intent explicit (the parent's {#key} remounts
  // us for a new target, so we never need to react to `editing` changing).
  const seed = untrack(() => editing);
  const isNew = seed === null;
  // Stable id for this form instance: an edit keeps the existing id; a new
  // connection mints one ONCE. Minting inside buildConfig() instead would hand
  // Save and Test (each call it) different ids, persisting duplicate rows and
  // orphaning Keychain entries on "New → Test → Save".
  const id = seed?.id ?? crypto.randomUUID();

  // Local form state, seeded from `seed` (or blank for new). Fields are mutated
  // directly — this is a fresh instance per edit target.
  let name = $state(seed?.name ?? "");
  let server = $state(seed?.server ?? "");
  let username = $state(seed?.username ?? "");
  let defaultDatabase = $state(seed?.defaultDatabase ?? "");
  let encrypt = $state(seed?.encrypt ?? false);
  let trustServerCertificate = $state(seed?.trustServerCertificate ?? true);

  let password = $state("");
  // Seeded from the saved config so editing shows the correct state (85b); default
  // true for a new connection (store in Keychain).
  let rememberPassword = $state(seed?.rememberPassword ?? true);

  let status = $state<{ kind: "ok" | "error"; text: string } | null>(null);
  let busy = $state(false);

  function buildConfig(): ConnectionConfig {
    return {
      id,
      name,
      server,
      username,
      defaultDatabase: defaultDatabase.trim() === "" ? null : defaultDatabase.trim(),
      encrypt,
      trustServerCertificate,
      rememberPassword,
    };
  }

  async function onSave() {
    busy = true;
    status = null;
    try {
      // Send the password whenever one was typed; the backend routes it by the
      // rememberPassword flag (Keychain vs session-only). null = no change (e.g.
      // editing without a new password) — leaves the stored secret untouched.
      const pw = password !== "" ? password : null;
      const cfg = buildConfig();
      await save(cfg, pw);
      // 85b: a session-only save with a typed password is already stashed in the
      // backend session map — mark it unlocked so App doesn't re-prompt.
      if (pw !== null && !rememberPassword) onSessionUnlock(cfg.id);
      onclose();
    } catch (e) {
      status = { kind: "error", text: String(e) };
    } finally {
      busy = false;
    }
  }

  async function onTest() {
    // Test needs a saved connection (the password lives in the Keychain or the
    // session map, keyed by id). Save first if this is a brand-new connection.
    busy = true;
    status = null;
    try {
      const cfg = buildConfig();
      const pw = password !== "" ? password : null;
      await save(cfg, pw);
      if (pw !== null && !rememberPassword) onSessionUnlock(cfg.id);
      await testConnection(cfg.id);
      status = { kind: "ok", text: "Connection OK (SELECT 1 succeeded)." };
    } catch (e) {
      // Surfaces e.g. Config("no stored password…") legibly when Remember was off
      // and no password has been entered this session.
      status = { kind: "error", text: String(e) };
    } finally {
      busy = false;
    }
  }
</script>

<div class="form">
  <h2>{isNew ? "New connection" : `Edit: ${seed?.name}`}</h2>

  <label>Name<input bind:value={name} /></label>
  <!-- Identifier fields: no spell-check squiggle, no WebKit autocorrect/autocapitalize
       mangling hostnames or logins like `sa` (billz-pj7). -->
  <label>Server (host,port)<input bind:value={server} placeholder="myhost,1433" spellcheck="false" autocorrect="off" autocapitalize="off" /></label>
  <label>Username<input bind:value={username} spellcheck="false" autocorrect="off" autocapitalize="off" /></label>
  <label>
    Password
    <input type="password" bind:value={password} placeholder={isNew ? "" : "(unchanged)"} />
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={rememberPassword} />
    Remember password in Keychain (else prompt each session)
  </label>
  <label>Default database (optional)<input bind:value={defaultDatabase} spellcheck="false" autocorrect="off" autocapitalize="off" /></label>
  <label class="check">
    <input type="checkbox" bind:checked={encrypt} /> Encrypt
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={trustServerCertificate} /> Trust server certificate
  </label>

  <div class="actions">
    <button class="primary" onclick={onSave} disabled={busy}>Save</button>
    <button onclick={onTest} disabled={busy}>Test</button>
    <button onclick={onclose} disabled={busy}>Cancel</button>
  </div>

  {#if status}
    <p class="status {status.kind}">{#if status.kind === "error"}<span class="status-icon" aria-hidden="true"><X size={13} /></span>{/if}{status.text}</p>
  {/if}
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    padding: var(--sp-2);
    max-width: 24rem;
    font-family: var(--font-ui);
  }
  h2 { font-size: var(--fs-md); margin: var(--sp-2) 0; color: var(--text); }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  label.check { flex-direction: row; align-items: center; gap: var(--sp-1); }
  input[type="password"],
  input:not([type]) {
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
    padding: var(--sp-1) var(--sp-2);
    font: inherit;
  }
  .actions { display: flex; gap: var(--sp-1); margin-top: var(--sp-2); }
  button {
    cursor: pointer;
    padding: var(--sp-1) var(--sp-3);
    border-radius: var(--r-sm);
    border: 1px solid var(--border-strong);
    background: var(--raised);
    color: var(--text);
    font: inherit;
    transition: background var(--dur-fast) var(--ease);
  }
  button.primary {
    background: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }
  button:disabled { opacity: 0.5; cursor: default; }
  .status { font-size: var(--fs-sm); }
  .status.ok { color: var(--ok); }
  .status.error { color: var(--danger); white-space: pre-wrap; }
  .status-icon { margin-right: var(--sp-1); }
</style>
