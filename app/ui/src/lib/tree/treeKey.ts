// Pure hierarchical path-key builder for object-tree node selection (billz-a8a).
// No Svelte / DOM — bun-testable. Each node composes its key from its parent's key
// plus its own segment, so uniqueness follows the tree hierarchy. The root parent
// is the connection id, so a selection made under one connection never matches a
// node rendered under another (no cross-connection false highlight).
export function childKey(
  parentKey: string,
  kind: "db" | "table" | "view" | "col",
  name: string,
): string {
  return `${parentKey}/${kind}:${name}`;
}
