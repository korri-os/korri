// Development only: `bun run caliper` shows Pico's parts at true device size.
// The portal builds Pico from source with its own Vite config, not this one.
import { caliper } from "@simonwjackson/caliper"
import { defineConfig } from "vite"

export default defineConfig({
  // Open Pico in the Caliper app; the app owns the take agent and its settings.
  plugins: [caliper()],
  // Vite answers 403 for host names it does not know. These are this machine's tailnet names.
  server: { allowedHosts: ["zao", "zao.hummingbird-lake.ts.net"] },
})
