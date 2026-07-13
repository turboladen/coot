<script lang="ts">
  import type { ViewInfo } from "../api";
  import { Eye } from "../icons";

  // v1: a view is a plain leaf. The AC requires table→columns, not view→columns.
  // (`list_columns` already works on views since it queries sys.columns/sys.objects
  // — leave that as a seam, don't build view expansion here.)
  let { view }: { view: ViewInfo } = $props();
</script>

<li class="view"><Eye size={13} /> {view.schema}.{view.name}</li>

<style>
  .view {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.1rem 0.3rem 0.1rem 1.4rem;
    font-size: 0.85rem;
    color: var(--text);
    white-space: nowrap;
  }
  .view :global(svg) { color: var(--muted); flex: none; }
</style>
