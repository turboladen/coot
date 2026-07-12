// Persisted (localStorage) Global param defaults, keyed by @name (e.g. "@today").
// Query inputs, not secrets. Mirrors tabs.svelte.ts persistence: load on init,
// merge + persist on write, degrade to {} on a corrupt/absent blob.
import { parseStringMap } from "./paramBarLogic";

const STORAGE_KEY = "billz.globalParams.v1";

function load(): Record<string, string> {
  try {
    return parseStringMap(localStorage.getItem(STORAGE_KEY));
  } catch (e) {
    console.warn("billz: failed to load global params from localStorage", e);
    return {};
  }
}

export const globalParams = $state<Record<string, string>>(load());

// Merge `writes` into the global store and persist (called on Run for
// global-scoped params).
export function setGlobalParams(writes: Record<string, string>): void {
  for (const [k, v] of Object.entries(writes)) globalParams[k] = v;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(globalParams));
  } catch (e) {
    console.warn("billz: failed to persist global params to localStorage", e);
  }
}

// Clear one param's Global value + persist the removal (d28.9). Mirrors
// setGlobalParams' persistence; delete is reactive on the $state proxy.
export function clearGlobalParam(name: string): void {
  delete globalParams[name];
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(globalParams));
  } catch (e) {
    console.warn("billz: failed to persist global params to localStorage", e);
  }
}
