import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  // Tauri expects a fixed port and its own control of the screen:
  clearScreen: false, // don't wipe Rust compiler errors
  server: {
    port: 1420, // must match tauri.conf.json devUrl
    strictPort: true, // fail rather than silently pick another port
  },
});
