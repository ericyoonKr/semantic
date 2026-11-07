import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

const gateway = "http://127.0.0.1:8090";

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    assetsInlineLimit: 0,
    rolldownOptions: {
      output: {
        minify: { compress: { dropConsole: true, dropDebugger: true } },
      },
    },
  },
  server: {
    host: "127.0.0.1",
    proxy: {
      "/api": gateway,
      "/session": gateway,
      "/prefs": gateway,
    },
  },
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
  },
});
