// UI state for the connection manager (Svelte 5 runes module). The single source
// of truth for the connection list + which one is active. Mutate the exported
// `$state` object's fields — never reassign the export.
import {
  type ConnectionConfig,
  deleteConnection,
  listConnections,
  saveConnection,
} from "./api";

export const conns = $state<{ list: ConnectionConfig[]; activeId: string | null }>({
  list: [],
  activeId: null,
});

export async function refresh() {
  conns.list = await listConnections();
}

export async function save(cfg: ConnectionConfig, password: string | null) {
  await saveConnection(cfg, password);
  await refresh();
}

export async function remove(id: string) {
  await deleteConnection(id);
  if (conns.activeId === id) conns.activeId = null;
  await refresh();
}

export function select(id: string) {
  conns.activeId = id;
}
