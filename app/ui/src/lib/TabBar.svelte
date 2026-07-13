<script lang="ts">
  // cwt.8 — the editor tab bar. Reads the tabsState runes module and calls its
  // ops; holds no state of its own. One tab per open scratch query (derived title
  // + × close), a + button to open a new one. Click a tab to select it.
  import { closeTab, newTab, selectTab, tabsState } from "./tabs.svelte";
  import { Plus, X } from "./icons";

  function onClose(e: MouseEvent, id: string) {
    e.stopPropagation(); // don't also select the tab we're closing
    closeTab(id);
  }
</script>

<div class="tab-bar">
  {#each tabsState.tabs as tab (tab.id)}
    <div
      class="tab"
      class:active={tabsState.activeId === tab.id}
      role="tab"
      tabindex="0"
      aria-selected={tabsState.activeId === tab.id}
      onclick={() => selectTab(tab.id)}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && selectTab(tab.id)}
    >
      <span class="title">{tab.title}</span>
      <button class="close" title="Close tab" onclick={(e) => onClose(e, tab.id)}><X size={13} /></button>
    </div>
  {/each}
  <button class="new" title="New tab" onclick={newTab}><Plus size={13} /></button>
</div>

<style>
  .tab-bar {
    display: flex;
    align-items: stretch;
    gap: 0.25rem;
    padding: 0.25rem 0.4rem 0;
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.5rem;
    border: 1px solid var(--border);
    border-bottom: none;
    border-radius: var(--r-sm) var(--r-sm) 0 0;
    background: var(--panel);
    cursor: pointer;
    font-size: 0.8rem;
    white-space: nowrap;
  }
  .tab.active {
    background: var(--raised);
    box-shadow: var(--shadow-sm);
    border-color: var(--border-strong);
  }
  .title { max-width: 14rem; overflow: hidden; text-overflow: ellipsis; }
  .close {
    display: inline-flex;
    align-items: center;
    border: none;
    background: none;
    cursor: pointer;
    line-height: 1;
    padding: 0 0.1rem;
    color: var(--muted);
  }
  .close:hover { color: var(--text); }
  .new {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--panel);
    cursor: pointer;
    padding: 0 0.5rem;
    align-self: center;
    color: var(--muted);
  }
</style>
