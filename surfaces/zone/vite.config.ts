// Development only: `bun run caliper` shows Zone's parts at true device size.
// The portal builds Zone from source with its own Vite config, not this one.
import { caliper } from "@simonwjackson/caliper"
import { defineConfig } from "vite"

export default defineConfig({
  // Zone targets the handhelds and the two sizes the mockup was proven at:
  // 640x480 (the RG353M) and 1280x720 (a 720p screen).
  plugins: [caliper({
    devices: ["rg353m", {
      id: "screen-720p",
      name: "720p screen",
      widthMm: 155,
      heightMm: 87,
      cssWidth: 1280,
      cssHeight: 720,
      viewportNote: "The mockup's wide size, docs/design/surfaces/mania/shot-1280x720.png. Millimetres estimated for a 7 in diagonal.",
    }],
  })],
  server: { allowedHosts: ["zao", "zao.hummingbird-lake.ts.net"] },
})
