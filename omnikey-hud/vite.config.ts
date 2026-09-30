import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Config standard Tauri : port fixe, pas de clearScreen pour garder les logs cargo.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Ne pas surveiller le backend Rust : cargo verrouille ses .exe de build (EBUSY).
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "chrome110",
  },
});
