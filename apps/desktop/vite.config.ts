import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  // Keep one build usable by Tauri and an Android packaged-asset host.
  base: "./",
  plugins: [vue()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: ["es2022", "chrome105", "safari13"],
    minify: "oxc",
    sourcemap: false,
  },
});
