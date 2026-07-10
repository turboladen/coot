<script lang="ts">
  import type { ConnectionConfig } from "./api";
  import { conns, remove, select } from "./connections.svelte";

  // The parent owns the form; the list just signals New/Edit up.
  let { onnew, onedit }: { onnew: () => void; onedit: (cfg: ConnectionConfig) => void } = $props();

  async function onDelete(cfg: ConnectionConfig) {
    if (confirm(`Delete connection "${cfg.name}"?`)) {
      await remove(cfg.id);
    }
  }
</script>

<div class="list">
  <div class="header">
    <h2>Connections</h2>
    <button onclick={onnew}>New</button>
  </div>

  {#if conns.list.length === 0}
    <p class="empty">No saved connections yet.</p>
  {:else}
    <ul>
      {#each conns.list as cfg (cfg.id)}
        <li class:active={conns.activeId === cfg.id}>
          <div class="meta">
            <strong>{cfg.name}</strong>
            <span class="server">{cfg.server}</span>
          </div>
          <div class="actions">
            <button onclick={() => select(cfg.id)}>Select</button>
            <button onclick={() => onedit(cfg)}>Edit</button>
            <button onclick={() => onDelete(cfg)}>Delete</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .list { padding: 0.5rem; }
  .header { display: flex; align-items: center; justify-content: space-between; }
  h2 { font-size: 1rem; margin: 0.5rem 0; }
  .empty { color: #888; font-size: 0.9rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { padding: 0.5rem; border: 1px solid #ccc; border-radius: 4px; margin-bottom: 0.4rem; }
  li.active { border-color: #3b82f6; background: rgba(59, 130, 246, 0.08); }
  .meta { display: flex; flex-direction: column; }
  .server { color: #888; font-size: 0.8rem; }
  .actions { display: flex; gap: 0.3rem; margin-top: 0.3rem; }
  button { font-size: 0.8rem; cursor: pointer; }
</style>
