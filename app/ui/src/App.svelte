<script lang="ts">
  import { onMount } from "svelte";
  import type { ConnectionConfig } from "./lib/api";
  import ConnectionForm from "./lib/ConnectionForm.svelte";
  import ConnectionList from "./lib/ConnectionList.svelte";
  import { refresh } from "./lib/connections.svelte";

  // The form pane: `undefined` = closed, `null` = new, a config = editing it.
  // `{#key}` on the form remounts it when the target changes so fields re-init.
  let editing = $state<ConnectionConfig | null | undefined>(undefined);

  onMount(() => {
    // Falls back silently outside a Tauri webview (plain `vite` in a browser).
    refresh().catch(() => {});
  });

  function openNew() {
    editing = null;
  }
  function openEdit(cfg: ConnectionConfig) {
    editing = cfg;
  }
  function closeForm() {
    editing = undefined;
  }
</script>

<main>
  <aside>
    <ConnectionList onnew={openNew} onedit={openEdit} />
  </aside>
  <section>
    {#if editing !== undefined}
      {#key editing}
        <ConnectionForm editing={editing} onclose={closeForm} />
      {/key}
    {:else}
      <p class="hint">Select a connection, or click <strong>New</strong> to add one.</p>
    {/if}
  </section>
</main>

<style>
  main { display: grid; grid-template-columns: 20rem 1fr; height: 100vh; font-family: system-ui, sans-serif; }
  aside { border-right: 1px solid #ccc; overflow-y: auto; }
  section { padding: 0.5rem; }
  .hint { color: #888; padding: 1rem; }
</style>
