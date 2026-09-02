import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  base: "./",
  plugins: [react()],
  build: {
    outDir: "dist-vscode",
    emptyOutDir: true,
    rollupOptions: {
      input: "src/vscode-main.tsx",
      output: {
        entryFileNames: "assets/vscode-main.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
});
