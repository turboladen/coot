<script lang="ts">
  // Session-only password prompt (billz-85b). window.prompt is unreliable in the
  // Tauri v2 WKWebView, so this is an inline modal mirroring ConnectionForm's
  // field pattern. Parent supplies the connection name + submit/cancel.
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
  <h3>Password for {name}</h3>
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
    background: #fff;
    border: 1px solid #ccc;
    border-radius: 8px;
    padding: 1rem 1.2rem;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
    min-width: 18rem;
  }
  h3 {
    margin: 0 0 0.3rem;
    font-size: 0.95rem;
  }
  .hint {
    margin: 0 0 0.6rem;
    font-size: 0.75rem;
    color: #888;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    padding: 0.3rem;
    margin-bottom: 0.6rem;
  }
  .actions {
    display: flex;
    gap: 0.4rem;
    justify-content: flex-end;
  }
  button {
    font-size: 0.85rem;
    cursor: pointer;
  }
</style>
