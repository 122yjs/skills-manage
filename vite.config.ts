import { defineConfig } from "vite";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import os from "node:os";
import path from "path";

import { loadDashboardSnapshot } from "./web-dashboard/snapshot";

const host = process.env.TAURI_DEV_HOST;

function webDashboardPlugin(): Plugin {
  const databasePath = path.join(os.homedir(), ".skillsmanage", "db.sqlite");

  return {
    name: "skills-manage-web-dashboard",
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const pathname = new URL(request.url ?? "/", "http://localhost").pathname;
        if (pathname !== "/api/dashboard") {
          next();
          return;
        }

        response.setHeader("Cache-Control", "no-store");
        response.setHeader("Content-Type", "application/json; charset=utf-8");
        if (request.method !== "GET") {
          response.statusCode = 405;
          response.end(JSON.stringify({ error: "Method not allowed" }));
          return;
        }

        try {
          response.statusCode = 200;
          response.end(JSON.stringify(loadDashboardSnapshot(databasePath)));
        } catch (error) {
          response.statusCode = 500;
          response.end(
            JSON.stringify({
              error: error instanceof Error ? error.message : String(error),
            }),
          );
        }
      });
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async ({ mode }) => ({
  plugins: [
    ...(mode === "web" ? [webDashboardPlugin()] : []),
    tailwindcss(),
    react(),
  ],

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 24200,
    strictPort: true,
    host: mode === "web" ? "127.0.0.1" : host || false,
    hmr: mode !== "web" && host
      ? {
          protocol: "ws",
          host,
          port: 24201,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },

  // Vitest configuration
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
}));
