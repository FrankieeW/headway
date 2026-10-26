import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

/**
 * Vendor chunking for the production renderer.
 *
 * The app ships as static local assets inside the Tauri bundle, so there is no network
 * caching win; the point is to keep the entry chunk under Vite's 500 kB warning without
 * suppressing the warning, and to isolate the stable React runtime/UI kit from app code.
 * The groups are dependency-acyclic: react ← radix/lucide/sonner/react-i18next,
 * react ← react-dom ← scheduler, app → all.
 */
function vendorChunk(id: string): string | undefined {
  if (!id.includes("node_modules")) return undefined;
  if (/node_modules\/(react|react-dom|scheduler)\//.test(id)) return "react-vendor";
  if (id.includes("node_modules/@radix-ui/")) return "ui-vendor";
  if (
    id.includes("node_modules/@tanstack/") ||
    id.includes("node_modules/i18next") ||
    id.includes("node_modules/react-i18next")
  ) {
    return "data-vendor";
  }
  return undefined;
}

export default defineConfig({
  plugins: [react()],
  root: "src",
  base: "./",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [{ name: vendorChunk, debugName: "renderer-vendors" }],
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_"],
});
