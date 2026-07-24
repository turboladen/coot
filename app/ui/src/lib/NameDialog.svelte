<script lang="ts">
  // Single-field name prompt (billz-he0). Generalized from PasswordPrompt.svelte,
  // which established the pattern: window.prompt is unreliable in the Tauri v2
  // WKWebView, so name entry is an inline modal.
  //
  // Deliberately generic (title/label/submitLabel are props) rather than
  // save-specific — billz-1kn needs the same dialog to RENAME a saved query, and a
  // second bespoke name UI is how two name flows drift apart.
  import { untrack } from "svelte";
  import { Save } from "./icons";

  let {
    title,
    label = "Name",
    value = "",
    submitLabel = "Save",
    onsubmit,
    oncancel,
  }: {
    title: string;
    label?: string;
    value?: string;
    submitLabel?: string;
    onsubmit: (name: string) => void;
    oncancel: () => void;
  } = $props();

  // Seeded from `value` then owned locally — the parent passes a suggested name,
  // it doesn't drive the field. `untrack` makes that init-only capture explicit
  // (otherwise svelte-check flags it as a probable mistake); the dialog is mounted
  // inside an {#if}, so it remounts with a fresh suggestion each time it opens.
  let name = $state(untrack(() => value));
  let input = $state<HTMLInputElement>();

  // Per-instance id for the label↔input association. NOT a hardcoded literal: this
  // dialog is deliberately reusable (billz-1kn wants it for RENAME too), and two
  // mounted instances sharing one DOM id makes the document invalid and points both
  // labels at whichever input the browser resolves first.
  const inputId = `name-dialog-${crypto.randomUUID()}`;

  // Focus the field once mounted, and pre-select the suggestion so typing replaces
  // it (the suggested title is a starting point, not something to edit around).
  $effect(() => {
    input?.focus();
    input?.select();
  });

  // Trailing whitespace shouldn't count as a name; the store trims too, but the
  // submit button must agree with what actually gets saved.
  const valid = $derived(name.trim() !== "");
</script>

<!-- Escape cancels (mirrors PasswordPrompt / TableNode's menu pattern). -->
<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") oncancel();
  }}
/>

<!-- Button backdrop (not a static div) so svelte-check a11y stays clean — same
     pattern as PasswordPrompt.svelte / tree/TableNode.svelte. -->
<button class="backdrop" aria-label="Cancel" onclick={oncancel}></button>
<div class="modal" role="dialog" aria-modal="true" aria-label={title}>
  <h3><Save size={16} /> {title}</h3>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      if (valid) onsubmit(name);
    }}
  >
    <label for={inputId}>{label}</label>
    <input id={inputId} bind:this={input} bind:value={name} />
    <div class="actions">
      <button type="submit" disabled={!valid}>{submitLabel}</button>
      <button type="button" onclick={oncancel}>Cancel</button>
    </div>
  </form>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: var(--scrim);
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
    min-width: 20rem;
    font-family: var(--font-ui);
    color: var(--text);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    margin: 0 0 0.6rem;
    font-size: var(--fs-md);
    color: var(--text);
  }
  label {
    display: block;
    margin-bottom: 0.2rem;
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
