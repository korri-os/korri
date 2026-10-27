// Development only: `bun run caliper` shows Pico's parts at true device size.
// The portal builds Pico from source with its own Vite config, not this one.
import { caliper } from "@simonwjackson/caliper"
import { defineConfig } from "vite"

export default defineConfig({
  // Open Pico in the Caliper app; the app owns the take agent and its settings.
  // Pico targets the handhelds, not Caliper's standard phones and screens.
  plugins: [caliper({
    devices: ["rg353m", "odin2portal", {
      id: "rpminiv2",
      name: "RP MINI V2",
      widthMm: 75,
      heightMm: 65,
      cssWidth: 1240,
      cssHeight: 1080,
      viewportNote: "From nix/devices/rpminiv2/portal.nix: 1080 x 1240 DSI-1 at transform 90, scale 1. Millimetres inferred from a 3.92 in diagonal.",
    }, {
      id: "r36tmax",
      name: "R36T MAX",
      widthMm: 72,
      heightMm: 72,
      cssWidth: 720,
      cssHeight: 720,
      viewportNote: "From nix/devices/r36tmax/portal.nix: 720 x 720 DSI-1, scale 1. Millimetres assume a 4 in diagonal, which HARDWARE.md does not record.",
    }],
  })],
  // Vite answers 403 for host names it does not know. These are this machine's tailnet names.
  server: { allowedHosts: ["zao", "zao.hummingbird-lake.ts.net"] },
})
