<script lang="ts">
  // cwt.8 — the editor tab bar. Reads the tabsState runes module and calls its
  // ops; holds no state of its own. One tab per open scratch query (derived title
  // + × close), a + button to open a new one. Click a tab to select it.
  import { closeTab, newTab, selectTab, tabsState } from "./tabs.svelte";

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
      <button class="close" title="Close tab" onclick={(e) => onClose(e, tab.id)}>×</button>
    </div>
  {/each}
  <button class="new" title="New tab" onclick={newTab}>+</button>
</div>

<style>
  .tab-bar {
    display: flex;
    align-items: stretch;
    gap: 0.25rem;
    padding: 0.25rem 0.4rem 0;
    border-bottom: 1px solid #ccc;
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.5rem;
    border: 1px solid #ccc;
    border-bottom: none;
    border-radius: 4px 4px 0 0;
    background: #f5f5f5;
    cursor: pointer;
    font-size: 0.8rem;
    white-space: nowrap;
  }
  .tab.active {
    background: #fff;
    border-color: #3b82f6;
  }
  .title { max-width: 14rem; overflow: hidden; text-overflow: ellipsis; }
  .close {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 1rem;
    line-height: 1;
    padding: 0 0.1rem;
    color: #888;
  }
  .close:hover { color: #000; }
  .new {
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #f5f5f5;
    cursor: pointer;
    font-size: 0.9rem;
    padding: 0 0.5rem;
    align-self: center;
  }
</style>
