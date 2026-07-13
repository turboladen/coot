<script lang="ts">
  // Session-only password prompt (billz-85b). window.prompt is unreliable in the
  // Tauri v2 WKWebView, so this is an inline modal mirroring ConnectionForm's
  // field pattern. Parent supplies the connection name + submit/cancel.
  import { Lock } from "./icons";

  let {
    name,
    onsubmit,
    oncancel,
  }: {
    name: string;
    onsubmit: (password: string) => void;
    oncancel: () => void;
  } = $props();
  let password = $state("");
  let input = $state<HTMLInputElement>();
  // Focus the field once mounted.
  $effect(() => {
    input?.focus();
  });
</script>

<!-- Escape cancels (mirrors TableNode's menu pattern). -->
<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") oncancel();
  }}
/>

<!-- Button backdrop (not a static div) so svelte-check a11y stays clean — same
     pattern as tree/TableNode.svelte's .menu-backdrop. -->
<button class="backdrop" aria-label="Cancel" onclick={oncancel}></button>
<div class="modal" role="dialog" aria-modal="true" aria-label="Unlock connection">
  <h3><Lock size={16} /> Password for {name}</h3>
  <p class="hint">Session-only — held in memory until you quit, never saved.</p>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      if (password !== "") onsubmit(password);
    }}
  >
    <input type="password" bind:this={input} bind:value={password} placeholder="Password" />
    <div class="actions">
      <button type="submit" disabled={password === ""}>Unlock</button>
      <button type="button" onclick={oncancel}>Cancel</button>
    </div>
  </form>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgba(0, 0, 0, 0.25);
    border: none;
    padding: 0;
    cursor: default;
  }
  .modal {
    position: fixed;
    top: 30%;
    left: 50%;
    transform: translateX(-50%);
    z-index: 51;
    background: var(--raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-xl);
    padding: var(--sp-4) var(--sp-5);
    box-shadow: var(--shadow-md);
    min-width: 18rem;
    font-family: var(--font-ui);
    color: var(--text);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    margin: 0 0 0.3rem;
    font-size: var(--fs-md);
    color: var(--text);
  }
  .hint {
    margin: 0 0 0.6rem;
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  input {
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
    padding: var(--sp-1) var(--sp-2);
    margin-bottom: 0.6rem;
    font: inherit;
  }
  .actions {
    display: flex;
    gap: var(--sp-1);
    justify-content: flex-end;
  }
  button {
    font-size: var(--fs-sm);
    cursor: pointer;
    padding: var(--sp-1) var(--sp-3);
    border-radius: var(--r-sm);
    border: 1px solid var(--border-strong);
    background: var(--raised);
    color: var(--text);
    font-family: inherit;
    transition: background var(--dur-fast) var(--ease);
  }
  button[type="submit"] {
    background: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
