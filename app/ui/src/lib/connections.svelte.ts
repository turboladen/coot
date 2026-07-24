// UI state for the connection manager (Svelte 5 runes module). The single source
// of truth for the connection LIST. Since billz-a5y.1, `activeId` is a MIRROR of
// the active tab's own `connectionId` — the tab owns which connection is active;
// tabs.svelte.ts keeps `activeId` in sync (syncActiveConnection/setActiveConnection).
// Mutate the exported `$state` object's fields — never reassign the export.
import {
  type ConnectionConfig,
  deleteConnection,
  listConnections,
  saveConnection,
} from "./api";
import { dropDatabases } from "./databases.svelte";

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
  dropDatabases(id); // billz-a5y.2: drop the cached entry so a removed id can't linger as "loaded this session"
  await refresh();
}
