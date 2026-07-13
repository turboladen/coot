<script lang="ts">
  // cwt.6 — renders ONE QueryResult as a headed, vertically-virtualized grid.
  // table-core (framework-agnostic) drives columns/rows; @tanstack/svelte-virtual
  // virtualizes the body so a big result only mounts visible rows. Sorting/
  // filtering/resizing and multi-result-set tabs are later waves.
  import { get } from "svelte/store";
  import { createTable, getCoreRowModel, type ColumnDef, type TableState } from "@tanstack/table-core";
  import { createVirtualizer } from "@tanstack/svelte-virtual";
  import type { CellValue, QueryResult } from "./api";
  import { Database, Search } from "./icons";
  import { renderCell } from "./renderCell";

  let { result }: { result: QueryResult } = $props();

  type Row = CellValue[];
  const ROW_H = 28;

  // One column per result column; accessor returns the CellValue at that index.
  const columns = $derived<ColumnDef<Row>[]>(
    result.columns.map((c, i) => ({
      id: String(i),
      // header carries the display name; sqlType is read from result.columns for the tag
      header: c.name,
      accessorFn: (row) => row[i],
      size: 160,
    })),
  );

  // table-core is lower-level than the framework adapters: it snapshots options
  // and requires state/onStateChange/renderFallbackValue. We own state in a rune
  // and bridge reactivity via setOptions inside the $derived.by below. The table
  // is constructed ONCE with inert initials (empty data/columns/state) — the
  // $derived.by bridge feeds the real data/columns/state on first read and on
  // every change. (Reading the reactive props here would only capture their
  // initial values, which Svelte warns about; the bridge is the reactive path.)
  let tableState = $state<TableState>({} as TableState);
  const table = createTable<Row>({
    data: [],
    columns: [],
    state: {} as TableState,
    onStateChange: (u) => {
      tableState = typeof u === "function" ? u(tableState) : u;
    },
    renderFallbackValue: null,
    getCoreRowModel: getCoreRowModel(),
  });

  // THE reactivity bridge, done the genuinely-reactive way: $derived.by reads
  // result.rows / columns / tableState (all tracked), so it re-feeds the table
  // and recomputes the row model whenever any of them change. (A plain $derived
  // reading table.getRowModel() would compute once and freeze — table method
  // calls aren't tracked.) `{#key result}` in the parent also remounts on a new
  // result set, keeping the virtualizer count clean.
  // ONE bridge, read by both rows and header: feed the table once (reading
  // result.rows/columns/tableState — all tracked), THEN read both the row model
  // and the header groups from the now-current table. Splitting these into two
  // deriveds is a bug: the template reads the header first, so a separate
  // header-derived would call getHeaderGroups() BEFORE the rows-derived ran
  // setOptions, memoizing an empty header + uncolumned body (invisible headless).
  const model = $derived.by(() => {
    // state must carry EVERY feature's fields — table-core's getState() returns
    // this object verbatim (no merge), and getHeaderGroups()/getRowModel() read
    // getState().columnPinning.left et al. A bare {} → "undefined is not an
    // object" crash and a blank grid. Merge table.initialState (features fill it)
    // under our own tableState, exactly as TanStack's React/Svelte adapters do.
    table.setOptions((prev) => ({ ...prev, data: result.rows, columns, state: { ...table.initialState, ...tableState } }));
    return { rows: table.getRowModel().rows, headerGroup: table.getHeaderGroups()[0] };
  });
  const rows = $derived(model.rows);
  const headerGroup = $derived(model.headerGroup);

  // Vertical virtualization. createVirtualizer returns a Svelte store — the
  // template's $rowVirtualizer reads keep it subscribed (which runs _didMount /
  // attaches the resize observer for its mounted lifetime).
  let scrollEl = $state<HTMLDivElement>();
  const rowVirtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0, // real count fed by the re-bind $effect below (avoids a reactive read here)
    getScrollElement: () => scrollEl ?? null,
    estimateSize: () => ROW_H,
    overscan: 12,
  });

  // Re-bind the scroll element + row count once the div mounts and on data change.
  // In Svelte 5 the ordering of bind:this populating scrollEl vs the virtualizer's
  // first read isn't guaranteed, so without this the grid can render BLANK until a
  // resize/scroll (invisible in headless checks). We read the instance via get()
  // — NON-reactively — so this effect depends only on scrollEl + row count, never
  // on the store itself (setOptions calls store.set internally; a reactive read
  // here would self-trigger an infinite effect loop). setOptions takes a PARTIAL
  // options object (svelte-virtual merges it over the current options) — NOT a
  // function updater. The template's subscription keeps the observer alive across
  // these transient get() reads.
  $effect(() => {
    const count = result.rows.length;
    const el = scrollEl;
    get(rowVirtualizer).setOptions({
      count,
      getScrollElement: () => el ?? null,
      estimateSize: () => ROW_H,
      overscan: 12,
    });
  });

  const gridTemplate = $derived(headerGroup ? headerGroup.headers.map((h) => `${h.getSize()}px`).join(" ") : "");
</script>

{#if result.columns.length === 0}
  <div class="empty">
    {#if result.rowsAffected != null}
      <span>{result.rowsAffected} rows affected</span>
    {:else}
      <Database size={20} />
      <span>No result set.</span>
    {/if}
  </div>
{:else}
  <div class="grid">
    <!-- Sticky header — same column widths as the body rows. -->
    <div class="header-row" style:grid-template-columns={gridTemplate}>
      {#each headerGroup?.headers ?? [] as header, i (header.id)}
        <div class="th" style:width="{header.getSize()}px">
          {header.column.columnDef.header}<span class="htype">{result.columns[i].sqlType}</span>
        </div>
      {/each}
    </div>

    <!-- Scroll container: fills remaining height, owns the vertical scroll. -->
    <div class="body" bind:this={scrollEl}>
      {#if rows.length === 0}
        <div class="no-rows">
          <Search size={20} />
          <span>No rows.</span>
        </div>
      {:else}
        <!-- Spacer sized to the full virtual height; rows absolutely positioned. -->
        <div class="spacer" style:height="{$rowVirtualizer.getTotalSize()}px">
          {#each $rowVirtualizer.getVirtualItems() as vi (vi.key)}
            {@const row = rows[vi.index]}
            <div
              class="tr"
              class:stripe={vi.index % 2 === 1}
              style:grid-template-columns={gridTemplate}
              style:height="{ROW_H}px"
              style:transform="translateY({vi.start}px)"
            >
              {#each row.getVisibleCells() as cell (cell.id)}
                {@const r = renderCell(cell.getValue<CellValue>())}
                <div
                  class="td"
                  class:nullish={r.nullish}
                  class:mono={r.mono}
                  class:num={r.align === "right"}
                  style:width="{cell.column.getSize()}px"
                  style:text-align={r.align}
                >
                  {r.text}
                </div>
              {/each}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .grid {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    font-size: 13px;
  }
  .header-row {
    display: grid;
    border-bottom: 2px solid var(--border-strong);
    background: var(--panel);
    flex: none;
  }
  .th {
    padding: 4px 8px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-right: 1px solid var(--border);
    color: var(--muted);
    font-family: var(--font-ui);
    box-sizing: border-box;
  }
  .htype {
    margin-left: var(--sp-1);
    font-size: var(--fs-xs);
    color: var(--type-tag);
    font-weight: 400;
  }
  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    position: relative;
  }
  .spacer {
    position: relative;
    width: 100%;
  }
  .tr {
    display: grid;
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    border-bottom: 1px solid var(--border);
    box-sizing: border-box;
  }
  .tr.stripe {
    background: color-mix(in srgb, var(--brand) 3%, var(--raised));
  }
  .td {
    padding: 4px 8px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-right: 1px solid var(--border);
    box-sizing: border-box;
    line-height: 20px;
  }
  .td.mono {
    font-family: var(--font-mono);
  }
  .td.num {
    color: var(--num-cell);
  }
  .td.nullish {
    color: var(--null-cell);
    font-style: italic;
  }
  .empty,
  .no-rows {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--sp-2);
    padding: var(--sp-5);
    color: var(--muted);
    text-align: center;
  }
  .empty :global(svg),
  .no-rows :global(svg) {
    color: var(--faint);
  }
</style>
