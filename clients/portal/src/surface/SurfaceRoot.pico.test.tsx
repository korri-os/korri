/**
 * Pico on the real host, driven with direction, confirm and back alone.
 *
 * Many handhelds deliver no `system`, `options` or `menu` button, so Pico's
 * Settings has to be reachable from the d-pad. This mounts Pico through
 * SurfaceRoot with the portal's own input bus and focus mover, opens Settings
 * from MENU, comes back, and launches a game.
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

// The portal compiles Pico from source, but Bun resolves Pico's peer React
// from the surface package. Keep this test on one React instance, as the Shift
// integration test does.
const picoPackageRoot = new URL("../", import.meta.resolve("@korri/pico"))
mock.module("react", () => React)
mock.module("react/jsx-runtime", () => ReactJsxRuntime)
mock.module(new URL("node_modules/react/index.js", picoPackageRoot).pathname, () => React)
mock.module(new URL("node_modules/react/jsx-runtime.js", picoPackageRoot).pathname, () => ReactJsxRuntime)

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
 * happy-dom has no layout. Give the controls the geometry a 640×480 home has:
 * the shelf in one row, MENU on the floor row below it, and MENU's list
 * stacked above MENU. Focus and click stay native.
 */
function layout() {
  for (const button of document.querySelectorAll<HTMLButtonElement>("button")) {
    button.getBoundingClientRect = () => new DOMRect(0, 0, 100, 40)
  }
  document.querySelectorAll<HTMLButtonElement>('[aria-label="Shelf"] button').forEach((cart, index) => {
    cart.getBoundingClientRect = () => new DOMRect(16 + index * 40, 300, 36, 80)
  })
  const menu = document.querySelector<HTMLButtonElement>("button[aria-haspopup]")
  if (menu !== null) menu.getBoundingClientRect = () => new DOMRect(16, 440, 48, 16)
  document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button').forEach((row, index) => {
    row.getBoundingClientRect = () => new DOMRect(30, 320 + index * 30, 180, 24)
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
  return (active.getAttribute("aria-label") ?? active.textContent ?? "").split(",")[0]?.trim()
}

test("Settings is reachable from Pico's home with the d-pad, A and B, and a game still launches after", async () => {
  const korrid = createInMemoryKorridClient({
    games: [game("wl4", "Wario Land 4"), game("tetris", "Tetris")],
  })
  const prepared: string[] = []
  const { PicoSurface } = await import("@korri/pico")
  const { SurfaceRoot } = await import("./SurfaceRoot")
  const surface: PortalSurface = {
    id: "pico",
    title: "Pico",
    presentations: ["catalog"],
    render: ({ model, host }) => <PicoSurface host={host} model={model} />,
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
          async sessionPrepare(id, host) {
            prepared.push(id)
            return korrid.sessionPrepare(id, host)
          },
        }}
        surface={surface}
      />,
    )
  })
  await waitFor(() => document.querySelectorAll('[aria-label="Shelf"] button').length === 2, "the shelf")

  // The first press lands on the shelf; Right chooses Tetris.
  await send(bus, { type: "direction", direction: "down" })
  await send(bus, { type: "direction", direction: "right" })
  expect(focusedName()).toBe("Tetris")

  // Down reaches MENU; A opens the list on FIND; Down, A opens Settings.
  await send(bus, { type: "direction", direction: "down" })
  expect(focusedName()).toBe("MENU")
  await send(bus, { type: "confirm" })
  expect(focusedName()).toBe("FIND")
  await send(bus, { type: "direction", direction: "down" })
  expect(focusedName()).toBe("SETTINGS")
  await send(bus, { type: "confirm" })
  expect(document.querySelector(".pico-panel-screen")).not.toBeNull()

  // B comes home with the cursor on MENU; Up finds Tetris still chosen.
  await send(bus, { type: "back" })
  expect(document.querySelector(".pico-panel-screen")).toBeNull()
  expect(focusedName()).toBe("MENU")
  await send(bus, { type: "direction", direction: "up" })
  expect(focusedName()).toBe("Tetris")

  // A opens Tetris; A again presses its first control, PLAY.
  await send(bus, { type: "confirm" })
  await waitFor(() => document.querySelector(".pico-game-detail") !== null, "Tetris's own screen")
  await send(bus, { type: "confirm" })
  await waitFor(() => prepared.length > 0, "the launch")
  expect(prepared).toEqual(["tetris"])
})
