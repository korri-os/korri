/**
 * The page test/pico-attract-browser.test.ts drives in Chromium: Pico's
 * Ready library through its treaty entry, on the RG DS displays or (with
 * `?displays=none`) on one, with the fixture host's buttons reachable as the
 * portal delivers them.
 */
import type { SurfaceInputAction } from "@contracts/surface/korri-surface"
import "../../src/pico-tokens.css"
import "../../src/pico-fonts.css"
import "../../src/pico.css"
import { rgdsDisplays } from "../../src/fixtures/displays"
import { createFixtureHost, fixtureModel } from "../../src/fixtures/fixture-host"
import { picoSurface } from "../../src/mount"

const root = document.getElementById("root")
if (root === null) throw new Error("no #root")
const host = createFixtureHost()
const single = new URLSearchParams(location.search).get("displays") === "none"
picoSurface.mount(root, single ? fixtureModel : { ...fixtureModel, displays: rgdsDisplays }, host)

declare global {
  interface Window {
    picoPress: (action: SurfaceInputAction) => void
  }
}

window.picoPress = action => host.press(action)
