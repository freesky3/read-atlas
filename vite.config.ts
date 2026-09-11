import { configDefaults, defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          const normalized = id.replaceAll("\\", "/");
          if (normalized.includes("pdfjs-dist")) return "pdfjs";
          if (
            normalized.includes("react-markdown") ||
            normalized.includes("remark-") ||
            normalized.includes("rehype-") ||
            normalized.includes("katex")
          )
            return "artifact-markdown";
          if (
            normalized.includes("node_modules/@xyflow/react") ||
            normalized.includes("node_modules/@xyflow/system") ||
            normalized.includes("node_modules/@dagrejs/dagre") ||
            normalized.includes("node_modules/@dagrejs/graphlib") ||
            normalized.includes("/src/outline/OutlineCanvas")
          ) {
            return "outline-graph";
          }
          if (
            normalized.includes("node_modules/react/") ||
            normalized.includes("node_modules/react-dom/")
          )
            return "react";
          return undefined;
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/tmp/**"] },
  },
  test: {
    exclude: [...configDefaults.exclude, "tmp/**", "src-tauri/target/**"],
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    restoreMocks: true,
    clearMocks: true,
  },
});
