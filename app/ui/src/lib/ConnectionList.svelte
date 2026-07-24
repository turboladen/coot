<script lang="ts">
  import type { ConnectionConfig } from "./api";
  import { conns, remove } from "./connections.svelte";
  import { Check } from "./icons";

  // The parent owns the form; the list just signals New/Edit up. `lockedIds` is
  // App's session-lock state (xhv.2) — surfaced here as a per-row status dot;
  // default empty so the component still stands alone (e.g. in tests). `onselect`
  // (billz-a5y.1) points the ACTIVE tab at a connection — App wires it to
  // setActiveConnection (which stamps the tab + mirrors conns.activeId).
  let {
    lockedIds = new Set<string>(),
    onnew,
    onedit,
    onselect,
  }: {
    lockedIds?: Set<string>;
    onnew: () => void;
    onedit: (cfg: ConnectionConfig) => void;
    onselect: (id: string) => void;
  } = $props();

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
            <div class="name-row">
              {#if lockedIds.has(cfg.id)}
                <span class="dot off" title="Session password needed"></span>
              {:else}
                <span class="dot on" title="Ready"><Check size={8} /></span>
              {/if}
              <strong>{cfg.name}</strong>
            </div>
            <span class="server">{cfg.server}</span>
          </div>
          <div class="actions">
            <button onclick={() => onselect(cfg.id)}>Select</button>
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
  .empty { color: var(--muted); font-size: 0.9rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { padding: 0.5rem; border: 1px solid var(--border); border-radius: var(--r-sm); margin-bottom: 0.4rem; }
  li.active {
    border-color: var(--border-strong);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    font-weight: 600;
  }
  .meta { display: flex; flex-direction: column; }
  .name-row { display: flex; align-items: center; gap: var(--sp-1); }
  .dot {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 0.7rem;
    height: 0.7rem;
    border-radius: var(--r-pill);
    flex: none;
  }
  .dot.on {
    background: var(--ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 22%, transparent);
  }
  .dot.on :global(svg) { color: var(--accent-fg); width: 7px; height: 7px; }
  .dot.off { background: transparent; border: 1.5px solid var(--faint); }
  .server { color: var(--muted); font-size: 0.8rem; }
  .actions { display: flex; gap: 0.3rem; margin-top: 0.3rem; }
  button { font-size: 0.8rem; cursor: pointer; }
</style>
