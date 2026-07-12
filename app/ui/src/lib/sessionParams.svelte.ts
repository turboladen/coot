// In-memory Session param values, keyed by @name (e.g. "@cust"). App-session
// lifetime — NOT persisted (that's Global). Mutate via setSessionParams; read the
// exported $state directly (like connections.svelte.ts).
export const sessionParams = $state<Record<string, string>>({});

// Merge `writes` into the session store (called on Run for session-scoped params).
export function setSessionParams(writes: Record<string, string>): void {
  for (const [k, v] of Object.entries(writes)) sessionParams[k] = v;
}
