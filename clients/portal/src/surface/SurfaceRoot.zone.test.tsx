/**
 * Zone on the real host, driven with direction, confirm and back alone.
 *
 * Many handhelds deliver no `system`, `options` or `menu` button, so Zone's
 * OPTIONS and SAVE DATA tabs have to be reachable from the d-pad. This mounts
 * Zone through SurfaceRoot with the portal's own input bus and focus mover,
 * goes up to the tabs, opens OPTIONS, comes back with B, and launches a game.
 */
import { afterEach, expect, mock, test } from "bun:test"
import * as React from "react"
import * as ReactJsxRuntime from "react/jsx-runtime"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { Game } from "@contracts/generated/korrid"
import { createInputBus, type InputBus } from "../input/bus"
import type { Direction } from "../input/types"
import { createSpatialFocusController } from "../input/spatial-focus"
import { createInMemoryKorridClient } from "../korrid/client"
import type { PortalSurface } from "./surface-registry"

// The portal compiles Zone from source, but Bun resolves Zone's peer React
// from the surface package. Keep this test on one React instance, as the Pico
// integration test does.
const zonePackageRoot = new URL("../", import.meta.resolve("@korri/zone"))
mock.module("react", () => React)
mock.module("react/jsx-runtime", () => ReactJsxRuntime)
mock.module(new URL("node_modules/react/index.js", zonePackageRoot).pathname, () => React)
mock.module(new URL("node_modules/react/jsx-runtime.js", zonePackageRoot).pathname, () => ReactJsxRuntime)

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const roots: Root[] = []
const disposers: Array<() => void> = []
afterEach(async () => {
  await act(async () => {
    for (const root of roots.splice(0)) root.unmount()
  })
  for (const dispose of disposers.splice(0)) dispose()
  document.body.innerHTML = ""
})

const game = (id: string, title: string): Game => ({
  id,
  title,
  host: "device-label",
  supportsRunnerSelection: false,
  source: { label: "device-label", isLocal: true },
})

const sleep = () => new Promise((resolve) => setTimeout(resolve, 0))

async function waitFor(ready: () => boolean, label: string) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (ready()) return
    await act(async () => {
      await sleep()
    })
  }
  throw new Error(`timed out: ${label}`)
}

/**
 * happy-dom has no layout. Give the controls the geometry a 640×480 zone
 * select has: the three tabs across the top, each over one card of the band,
 * and OPTIONS' rows stacked under the tabs. Every other control keeps a zero
 * box, which the focus mover skips. Focus and click stay native.
 */
function layout() {
  document.querySelectorAll<HTMLButtonElement>("[role='tablist'] button").forEach((tab, index) => {
    tab.getBoundingClientRect = () => new DOMRect(40 + index * 200, 10, 160, 40)
  })
  document.querySelectorAll<HTMLButtonElement>(".zone-card-band button").forEach((card, index) => {
    card.getBoundingClientRect = () => new DOMRect(40 + index * 200, 160, 160, 200)
  })
  document.querySelectorAll<HTMLButtonElement>(".zone-options-scroll button").forEach((row, index) => {
    row.getBoundingClientRect = () => new DOMRect(40, 100 + index * 40, 560, 36)
  })
}

async function send(bus: InputBus, action: { type: "confirm" } | { type: "back" } | { type: "direction"; direction: Direction }) {
  layout()
  await act(async () => {
    bus.emit(action)
    await sleep()
  })
}

const focusedName = () => {
  const active = document.activeElement
  if (!(active instanceof HTMLElement) || active === document.body) return "(nothing)"
  return (active.querySelector(".zone-card-title")?.textContent ?? active.textContent ?? "").trim()
}
const currentTab = () => document.querySelector("[role='tablist'] [aria-current='page']")?.textContent ?? ""

test("OPTIONS is reachable from Zone's home with the d-pad, A and B, and a game still launches after", async () => {
  const korrid = createInMemoryKorridClient({
    games: [game("wl4", "Wario Land 4"), game("tetris", "Tetris")],
  })
  // This navigation test starts in the library after first-boot setup.
  await korrid.finishOnboarding()
  const prepared: string[] = []
  const { ZoneSurface } = await import("@korri/zone")
  const { SurfaceRoot } = await import("./SurfaceRoot")
  const surface: PortalSurface = {
    id: "zone",
    title: "Zone",
    presentations: ["catalog", "gameplay-overlay"],
    render: ({ model, host }) => <ZoneSurface host={host} model={model} />,
  }
  const bus = createInputBus()
  disposers.push(createSpatialFocusController(bus), () => bus.dispose())
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.push(root)
  await act(async () => {
    root.render(
      <SurfaceRoot
        bus={bus}
        korrid={{
          ...korrid,
          async sessionStart(request) {
            prepared.push(request.gameId)
            return korrid.sessionStart(request)
          },
        }}
        surface={surface}
      />,
    )
  })
  await waitFor(() => document.querySelectorAll(".zone-card-band button").length === 2, "the band")

  // Zone opens on the first card; Right chooses Tetris.
  expect(focusedName()).toBe("Wario Land 4")
  await send(bus, { type: "direction", direction: "right" })
  expect(focusedName()).toBe("Tetris")

  // Up reaches the tab over it, OPTIONS; A opens it with the cursor kept there.
  await send(bus, { type: "direction", direction: "up" })
  expect(focusedName()).toContain("Options")
  await send(bus, { type: "confirm" })
  expect(document.querySelector(".zone-options")).not.toBeNull()
  expect(currentTab()).toContain("Options")
  expect(focusedName()).toContain("Options")

  // Down walks into OPTIONS' rows.
  await send(bus, { type: "direction", direction: "down" })
  expect(document.querySelector(".zone-options-scroll")?.contains(document.activeElement)).toBe(true)

  // B comes home with the cursor on Tetris, still chosen.
  await send(bus, { type: "back" })
  expect(document.querySelector(".zone-options")).toBeNull()
  expect(currentTab()).toContain("Games")
  expect(focusedName()).toBe("Tetris")

  // A on a card with one place launches it.
  await send(bus, { type: "confirm" })
  await waitFor(() => prepared.length > 0, "the launch")
  expect(prepared).toEqual(["tetris"])
})
