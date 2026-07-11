<script lang="ts">
  import type { ViewInfo } from "../api";

  // v1: a view is a plain leaf. The AC requires table→columns, not view→columns.
  // (`list_columns` already works on views since it queries sys.columns/sys.objects
  // — leave that as a seam, don't build view expansion here.)
  let { view }: { view: ViewInfo } = $props();
</script>

<li class="view">{view.schema}.{view.name}</li>

<style>
  .view {
    padding: 0.1rem 0 0.1rem 1.4rem;
    font-size: 0.85rem;
    color: #333;
    white-space: nowrap;
  }
</style>
