<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let name = $state("billz");

  // Round-trip through the Rust `app_name` command to prove the JS -> Rust bridge.
  // Falls back to the default when run outside a Tauri window (plain `vite` in a browser).
  $effect(() => {
    invoke<string>("app_name")
      .then((n) => { name = n; })
      .catch(() => { /* not in a Tauri webview; keep the default */ });
  });
</script>

<main>
  <h1>{name}</h1>
  <p>Tauri + Svelte 5 shell is alive.</p>
</main>

<style>
  main { font-family: system-ui, sans-serif; padding: 2rem; text-align: center; }
  h1 { font-size: 3rem; margin: 0; }
</style>
