/**
 * The page test/pico-attract-browser.test.ts drives in Chromium: Pico's
 * Ready library through its treaty entry, on the RG DS displays or (with
 * `?displays=none`) on one. Input goes through the portal's own input path:
 * its bus, and its spatial focus, which turns a direction into DOM focus and
 * confirm into a click. Back and Menu reach Pico from that same bus.
 */
import type { SurfaceHost, SurfaceInputAction } from "@contracts/surface/korri-surface"
import { createInputBus } from "../../../../clients/portal/src/input/bus"
import { createSpatialFocusController } from "../../../../clients/portal/src/input/spatial-focus"
import type { Direction } from "../../../../clients/portal/src/input/types"
import "../../src/pico-tokens.css"
import "../../src/pico-fonts.css"
import "../../src/pico.css"
import { rgdsDisplays } from "../../src/fixtures/displays"
import { createFixtureHost, fixtureModel } from "../../src/fixtures/fixture-host"
import { picoSurface } from "../../src/mount"

const root = document.getElementById("root")
if (root === null) throw new Error("no #root")
const bus = createInputBus()
createSpatialFocusController(bus)
const host: SurfaceHost = {
  ...createFixtureHost(),
  input: { on: (action, handler) => bus.onAction(action, () => handler()) },
}
const single = new URLSearchParams(location.search).get("displays") === "none"
picoSurface.mount(root, single ? fixtureModel : { ...fixtureModel, displays: rgdsDisplays }, host)

declare global {
  interface Window {
    /** A host button, through the portal's bus. */
    picoPress: (action: SurfaceInputAction) => void
    /** A d-pad press, through the portal's bus and spatial focus. */
    picoDirection: (direction: Direction) => void
    /** A confirm, through the portal's bus: a click on the focused control. */
    picoConfirm: () => void
  }
}

window.picoPress = action => bus.emit({ type: action } as Parameters<typeof bus.emit>[0])
window.picoDirection = direction => bus.emit({ type: "direction", direction })
window.picoConfirm = () => bus.emit({ type: "confirm" })
