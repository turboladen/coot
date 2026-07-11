// A bump counter that forces the ObjectTree to remount (rqb.5 Refresh). App.svelte
// keys the tree on `${conns.activeId}:${treeRefresh.nonce}`, so bumping the nonce
// tears down every node's local memo → re-fetch (and the just-invalidated core
// cache re-queries sys.*). Mutate the field in place; never reassign the export.
export const treeRefresh = $state({ nonce: 0 });
export function bumpRefresh(): void {
  treeRefresh.nonce += 1;
}
