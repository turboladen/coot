// d28.7 scoped-open orchestration: run a saved query "scoped" to a right-clicked
// table. Persists the query with @table's value + auto-typed column params, then
// opens it in a tab targeting the TABLE's database (so [schema].[table] resolves).
// App's param bar + run_params path take over — no new run mechanism. Kept out of
// TableNode to keep the tree node focused; .svelte.ts because `save` mutates the
// savedQueries $state store.
import { listColumns, type SavedQuery } from "./api";
import { autoTypeParams, deriveParams } from "./paramBarLogic";
import { save as saveQuery } from "./savedQueries.svelte";
import { newTabWithContent } from "./tabs.svelte";
import { quoteIdent } from "./tree/selectTopQuery";

export async function openScopedQuery(
  connId: string,
  db: string,
  schema: string,
  table: string,
  query: SavedQuery,
): Promise<void> {
  const columns = await listColumns(connId, db, schema, table);
  const tableRef = `${quoteIdent(schema)}.${quoteIdent(table)}`;
  const derived = deriveParams(query.sql, query.params);
  const typed = autoTypeParams(derived, columns);
  const params = typed.map((p) => (p.name === "@table" ? { ...p, lastValue: tableRef } : p));
  await saveQuery({ ...query, params });
  // Target the table's DB (not query.targetDatabase) so @table resolves.
  newTabWithContent(query.sql, db, query.id);
}
