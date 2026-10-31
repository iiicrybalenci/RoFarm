import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "chrome105",
    minify: "esbuild",
    cssCodeSplit: true,
    sourcemap: false,
    chunkSizeWarningLimit: 320,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules/solid-js")) return "solid";
          if (id.includes("@tauri-apps")) return "tauri";
          return undefined;
        },
      },
    },
  },
});
