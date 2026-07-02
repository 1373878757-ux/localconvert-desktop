import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true
  },
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        splash: "splash.html"
      }
    }
  },
  envPrefix: ["VITE_", "TAURI_"]
});
