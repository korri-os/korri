// Development only: `bun run caliper` shows Zone's parts at true device size.
// The portal builds Zone from source with its own Vite config, not this one.
import { caliper } from "@simonwjackson/caliper"
import { defineConfig, type Plugin } from "vite"
import { isZoneLook, ZONE_DEFAULT_LOOK, ZONE_LOOKS } from "./src/plan/look"

/**
 * The look every Caliper frame shows: `ZONE_LOOK=modern bun run caliper`
 * (or blend, retro; Blend, Zone's default, without it). The frame names it
 * on `<html>` before any part runs, so a part drawn alone (an atom, a page)
 * is drawn in that look, as the product's root `data-look` draws it
 * (src/fixtures/fixture-look.ts). Development only, as this file is.
 */
function zoneFrameLook(): Plugin {
  const asked = process.env.ZONE_LOOK ?? ZONE_DEFAULT_LOOK
  if (!isZoneLook(asked)) throw new Error(`ZONE_LOOK must be one of ${ZONE_LOOKS.join(", ")}; got ${asked}`)
  const look = asked
  return {
    name: "zone-frame-look",
    apply: "serve",
    transformIndexHtml: () => [
      { tag: "script", injectTo: "head-prepend", children: `document.documentElement.dataset.look = ${JSON.stringify(look)}` },
    ],
  }
}

export default defineConfig({
  // Zone targets the handhelds and the two sizes the mockup was proven at:
  // 640x480 (the RG353M) and 1280x720 (a 720p screen).
  plugins: [zoneFrameLook(), caliper({
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
