import { execSync } from "node:child_process"
import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// Self-identification: the portal renders this stamp so there is never a
// question of which UI (portal vs spike page vs native) is on screen.
const gitSha = (() => {
  try {
    return execSync("git rev-parse --short HEAD", { cwd: __dirname })
      .toString()
      .trim()
  } catch {
    return "unknown"
  }
})()

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: {
    __PORTAL_BUILD__: JSON.stringify(
      `portal ${gitSha} · ${new Date().toISOString().slice(0, 16)}Z`,
    ),
  },
  resolve: {
    alias: {
      "@contracts": resolve(__dirname, "../../contracts"),
      // Surfaces are compiled from source by their host. The alias is the only
      // thing that would change if Shift moved to its own package registry.
      "@korri/shift": resolve(__dirname, "../../surfaces/shift/src/index.ts"),
      "@korri/pico": resolve(__dirname, "../../surfaces/pico/src/index.ts"),
    },
  },
  server: {
    fs: {
      // Allow serving the contracts directory, which lives above this root.
      allow: [resolve(__dirname, "../..")],
    },
  },
  // The Android WebView loads from file:///android_asset/portal/, where
  // absolute /assets/... URLs would miss. Relative base keeps it working.
  base: "./",
})
