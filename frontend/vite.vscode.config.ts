import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  base: "./",
  // The webview loads the bundle as a classic script, where `import.meta` is a SyntaxError.
  define: {
    "import.meta.url": JSON.stringify(""),
    "import.meta.resolve": "undefined",
  },
  plugins: [react()],
  build: {
    outDir: "dist-vscode",
    emptyOutDir: true,
    rollupOptions: {
      input: "src/vscode-main.tsx",
      output: {
        // Classic <script> webview: no import.meta / dynamic-import chunks allowed.
        inlineDynamicImports: true,
        entryFileNames: "assets/vscode-main.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
});
