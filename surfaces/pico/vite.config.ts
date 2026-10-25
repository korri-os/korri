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
    }, {
      id: "rgds",
      name: "RG DS",
      widthMm: 81,
      heightMm: 122,
      cssWidth: 640,
      cssHeight: 960,
      viewportNote: "Both RG DS displays as one window: two 640 x 480 touch panels, the main one above. Millimetres inferred from two 4 in 4:3 panels with no hinge gap.",
    }, {
      id: "rgds-side",
      name: "RG DS SIDE BY SIDE",
      widthMm: 162,
      heightMm: 61,
      cssWidth: 1280,
      cssHeight: 480,
      viewportNote: "Not a real device: the RG DS displays side by side, to check that the split follows the rectangles.",
    }],
  })],
  // Vite answers 403 for host names it does not know. These are this machine's tailnet names.
  server: { allowedHosts: ["zao", "zao.hummingbird-lake.ts.net"] },
})
