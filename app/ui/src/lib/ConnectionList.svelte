<script lang="ts">
  import type { ConnectionConfig } from "./api";
  import { conns } from "./connections.svelte";
  import ConnectionNode from "./ConnectionNode.svelte";

  // billz-a5y.3: the sidebar container. Each connection is now a collapsible ROOT
  // (ConnectionNode) with its own object tree beneath it — the flat list + the single
  // lower-pane ObjectTree merged into one multi-root structure ("Objects" heading gone).
  // `lockedIds` is App's session-lock state; `onunlock` bubbles a locked root's "Enter
  // password" up to App to prompt for that connection (no retarget).
  let {
    lockedIds = new Set<string>(),
    onnew,
    onedit,
    onunlock,
  }: {
    lockedIds?: Set<string>;
    onnew: () => void;
    onedit: (cfg: ConnectionConfig) => void;
    onunlock: (id: string) => void;
  } = $props();
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
        <ConnectionNode conn={cfg} {lockedIds} {onedit} {onunlock} />
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
</style>
