// Pure, rune-free logic for the connection form (billz-a5y.7): splitting the
// stored `server` string into separate Host and Port fields and recombining
// them on save. The wire/storage shape stays "host,port" (ConnectionConfig has
// a single `server: string`, no port field) — SQL Server uses a COMMA before
// the port, so we key off the LAST comma. Kept here (not in the .svelte) so it's
// unit-testable with `bun test`.

export type HostPort = { host: string; port: string };

/**
 * Split a stored `server` value ("host" or "host,port") into host + port.
 * Splits on the LAST comma so an IPv6 literal (colons, no comma) or a named
 * instance (backslash, no comma) stays entirely in the host. No comma → empty
 * port. Both sides are trimmed.
 */
export function parseServer(server: string): HostPort {
  const i = server.lastIndexOf(",");
  if (i === -1) {
    return { host: server.trim(), port: "" };
  }
  return { host: server.slice(0, i).trim(), port: server.slice(i + 1).trim() };
}

/**
 * Recombine host + port into the stored `server` shape. An empty (or
 * whitespace-only) port yields host alone — no trailing comma. Both sides are
 * trimmed.
 */
export function formatServer(host: string, port: string): string {
  const h = host.trim();
  const p = port.trim();
  return p === "" ? h : `${h},${p}`;
}
