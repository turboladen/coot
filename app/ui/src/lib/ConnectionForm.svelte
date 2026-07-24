<script lang="ts">
  import { untrack } from "svelte";
  import type { ConnectionConfig, DatabaseInfo } from "./api";
  import { listDatabases, testConnection } from "./api";
  import { save } from "./connections.svelte";
  import { formatServer, parseServer } from "./connectionFormLogic";
  import { Check, Eye, EyeOff, X } from "./icons";

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
  // The stored `server` is "host,port" (or just "host"). Split it into separate
  // Host/Port fields for editing (billz-a5y.7); recombined on save via
  // formatServer in buildConfig.
  const seeded = parseServer(seed?.server ?? "");
  let host = $state(seeded.host);
  let port = $state(seeded.port);
  let username = $state(seed?.username ?? "");
  let defaultDatabase = $state(seed?.defaultDatabase ?? "");
  let encrypt = $state(seed?.encrypt ?? false);
  let trustServerCertificate = $state(seed?.trustServerCertificate ?? true);

  let password = $state("");
  // Eye-toggle: reveal the typed password (billz-a5y.7) by flipping the input's
  // `type`; see the value+oninput note on the markup below.
  let showPassword = $state(false);
  // Seeded from the saved config so editing shows the correct state (85b); default
  // true for a new connection (store in Keychain).
  let rememberPassword = $state(seed?.rememberPassword ?? true);

  // Databases loaded by a successful Test (list_databases). Empty until then —
  // the Default-database dropdown shows helper text meanwhile (billz-a5y.7).
  let databases = $state<DatabaseInfo[]>([]);

  let status = $state<{ kind: "ok" | "error"; text: string } | null>(null);
  let busy = $state(false);

  function buildConfig(): ConnectionConfig {
    return {
      id,
      name,
      server: formatServer(host, port),
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
      // SELECT 1 passed. Test does double duty (billz-a5y.7): also load the
      // database list to populate the Default-database dropdown. A list failure
      // must NOT mask the successful SELECT 1 — hence the inner try/catch.
      try {
        databases = await listDatabases(cfg.id);
        status = {
          kind: "ok",
          text: `Connection OK (SELECT 1 succeeded). Loaded ${databases.length} databases.`,
        };
      } catch (e) {
        status = {
          kind: "ok",
          text: `Connection OK (SELECT 1 succeeded). Could not load databases: ${e}`,
        };
      }
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

  <label>Name<input class="field" bind:value={name} /></label>
  <!-- Host + Port share one row; recombined into the stored "host,port" on save.
       Identifier fields: no spell-check squiggle, no WebKit autocorrect/autocapitalize
       mangling hostnames or logins like `sa` (billz-pj7). -->
  <div class="row">
    <label class="grow">Host<input class="field" bind:value={host} placeholder="myhost" spellcheck="false" autocorrect="off" autocapitalize="off" /></label>
    <label class="port">Port<input class="field" bind:value={port} placeholder="1433" spellcheck="false" autocorrect="off" autocapitalize="off" /></label>
  </div>
  <label>Username<input class="field" bind:value={username} spellcheck="false" autocorrect="off" autocapitalize="off" /></label>
  <label>
    Password
    <!-- Eye-toggle reveals the typed password. `type` is dynamic, which Svelte
         forbids together with `bind:value`, so we bind manually via value+oninput
         (this also keeps a single element, preserving caret/focus on toggle). -->
    <span class="pw-wrap">
      <input
        class="field"
        type={showPassword ? "text" : "password"}
        value={password}
        oninput={(e) => (password = e.currentTarget.value)}
        placeholder={isNew ? "" : "(unchanged)"}
      />
      <button
        type="button"
        class="eye"
        aria-label={showPassword ? "Hide password" : "Show password"}
        onclick={() => (showPassword = !showPassword)}
      >
        {#if showPassword}<EyeOff size={15} />{:else}<Eye size={15} />{/if}
      </button>
    </span>
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={rememberPassword} />
    Remember password in Keychain (else prompt each session)
  </label>
  <label>
    Default database (optional)
    <!-- Dropdown populated by Test (list_databases). Empty until then. A saved
         value not yet in the loaded list gets its own option so it still shows —
         guarded so it never DUPLICATES a value already in `databases`. -->
    <select class="field" bind:value={defaultDatabase}>
      <option value="">(none)</option>
      {#if defaultDatabase !== "" && !databases.some((d) => d.name === defaultDatabase)}
        <option value={defaultDatabase}>{defaultDatabase}</option>
      {/if}
      {#each databases as d (d.name)}
        <option value={d.name}>{d.name}</option>
      {/each}
    </select>
    {#if databases.length === 0}
      <span class="helper">Test the connection to load databases.</span>
    {/if}
  </label>
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
    <p class="status {status.kind}"><span class="status-icon" aria-hidden="true">{#if status.kind === "error"}<X size={13} />{:else}<Check size={13} />{/if}</span>{status.text}</p>
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
  /* Host + Port on one row. */
  .row { display: flex; gap: var(--sp-2); }
  .row .grow { flex: 1 1 auto; min-width: 0; }
  .row .port { flex: 0 0 6rem; }
  /* Class-based so styling survives the password input's dynamic type flip
     (type="text" would otherwise match neither an attribute selector). */
  input.field,
  select.field {
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
    padding: var(--sp-1) var(--sp-2);
    font: inherit;
    width: 100%;
    box-sizing: border-box;
  }
  .pw-wrap { position: relative; display: flex; }
  .pw-wrap .field { padding-right: 2rem; }
  .eye {
    position: absolute;
    top: 0;
    right: 0;
    height: 100%;
    display: flex;
    align-items: center;
    padding: 0 var(--sp-2);
    border: none;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }
  .eye:hover { color: var(--text); }
  .helper { color: var(--muted); font-size: var(--fs-sm); font-style: italic; }
  /* Tie checkboxes into the palette (billz-a5y.8) — form-scoped so it doesn't
     reach other checkboxes app-wide. */
  input[type="checkbox"] { accent-color: var(--accent); }
  .actions { display: flex; gap: var(--sp-1); margin-top: var(--sp-2); }
  /* Save/Test/Cancel use the global app.css button + .primary system (billz-a5y.8:
     dropped the local duplicate so they gain the shared hover states + stay in sync).
     `.eye`'s own border:none/background:none rules still win (class > element). */
  .status { font-size: var(--fs-sm); }
  .status.ok { color: var(--ok); }
  .status.error { color: var(--danger); white-space: pre-wrap; }
  /* Icon-paired feedback (app.css status principle): X for error, Check for ok. */
  .status-icon { margin-right: var(--sp-1); vertical-align: text-bottom; }
</style>
