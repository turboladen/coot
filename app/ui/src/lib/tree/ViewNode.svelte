<script lang="ts">
  import type { ViewInfo } from "../api";
  import { Eye } from "../icons";
  import { selection, selectNode } from "./selection.svelte";
  import { childKey } from "./treeKey";

  // v1: a view is a leaf. The AC requires table->columns, not view->columns.
  // (`list_columns` already works on views since it queries sys.columns/sys.objects
  // — leave that as a seam, don't build view expansion here.)
  let { view, parentKey }: { view: ViewInfo; parentKey: string } = $props();

  const key = $derived(childKey(parentKey, "view", view.schema + "." + view.name));
</script>

<li>
  <button class="view" class:selected={selection.key === key} onclick={() => selectNode(key)}>
    <Eye size={13} />
    <span class="label">{view.schema}.{view.name}</span>
  </button>
</li>

<style>
  li { list-style: none; }
  .view {
    display: flex;
    align-items: center;
    /* Reset the global button base's justify-content:center (app.css). */
    justify-content: flex-start;
    gap: var(--sp-1);
    width: 100%;
    /* Views are siblings of Tables — same depth-2 indent (1.2rem), not 1.4
       (billz-a5y.8: they were mis-indented one step deeper than tables). */
    padding: 0.2rem 0.3rem 0.2rem 1.2rem;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    font: inherit;
    font-size: 0.85rem;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    color: var(--text);
    transition: background var(--dur-fast) var(--ease);
  }
  .view :global(svg) { color: var(--muted); flex: none; }
  .view:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
  .view.selected,
  .view.selected:hover { background: var(--tree-selected-bg); }
  .view.selected .label { color: var(--tree-selected-fg); }
  .label { color: var(--text); }
</style>
