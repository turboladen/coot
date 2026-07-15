// Ephemeral (in-memory) object-tree selection: the path key of the last-clicked
// node (billz-a8a). NOT persisted — selection is transient focus, unlike column
// widths. A module-level $state store avoids prop-drilling a setter through the
// deeply nested tree (ObjectTree -> DatabaseNode -> TableNode -> ColumnLeaf),
// matching the store pattern in columnWidths.svelte.ts / globalParams.svelte.ts.
const state = $state<{ key: string | null }>({ key: null });

export const selection = state;

export function selectNode(key: string): void {
  state.key = key;
}
