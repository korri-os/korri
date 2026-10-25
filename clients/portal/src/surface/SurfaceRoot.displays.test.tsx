import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
let root: Root | undefined
afterEach(async () => { await act(async () => root?.unmount()); document.body.innerHTML = "" })

const displays = [
  { id: "DSI-2", main: true, x: 0, y: 0, width: 640, height: 480, touch: true },
  { id: "DSI-1", main: false, x: 0, y: 480, width: 640, height: 480, touch: true },
]
for (const scope of [undefined, "main", "all"] as const) {
  test(`surface ${scope ?? "default"} receives only its opted-in display facts`, async () => {
    let model: SurfaceModel | undefined
    const requests: string[] = []
    const client = createInMemoryKorridClient({ displays })
    const original = client.surfaceDisplays
    client.surfaceDisplays = async scope => { requests.push(scope); return original(scope) }
    const container = document.createElement("div")
    document.body.append(container)
    root = createRoot(container)
    await act(async () => root!.render(<SurfaceRoot bus={createInputBus()} korrid={client} surface={{
      id: "observer", title: "Observer", displays: scope, presentations: ["catalog"],
      render(props) { model = props.model; return null },
    }} />))
    for (let i = 0; i < 100 && requests.length === 0; i++) await act(async () => { await Bun.sleep(5) })
    expect(requests).toEqual([scope ?? "main"])
    expect(model?.displays).toEqual(scope === "all" ? displays : undefined)
  })
}

test("the overlay host configures its exact launch and publishes only opted-in facts", async () => {
  const { OverlayRoot } = await import("../overlay/OverlayRoot")
  const { createInMemoryOverlayController } = await import("../overlay/in-memory-overlay-controller")
  const client = createInMemoryKorridClient({ displays })
  const requests: unknown[] = []
  const original = client.surfaceDisplays
  client.surfaceDisplays = async (scope, overlay, launchId) => {
    requests.push({ scope, overlay, launchId })
    return original(scope, overlay, launchId)
  }
  let model: SurfaceModel | undefined
  const container = document.createElement("div")
  document.body.append(container)
  root = createRoot(container)
  await act(async () => root!.render(<OverlayRoot bus={createInputBus()} korrid={client} launchId="exact-launch"
    controller={createInMemoryOverlayController()} surface={{
      id: "observer", title: "Observer", displays: "all", presentations: ["gameplay-overlay"],
      render(props) { model = props.model; return null },
    }} />))
  expect(requests).toEqual([{ scope: "all", overlay: true, launchId: "exact-launch" }])
  expect(model?.displays).toEqual(displays)
  // Resume leaves the frozen portal mounted. The next Home creates a new
  // controller for the same launch; it must wake and place the overlay again.
  await act(async () => root!.render(<OverlayRoot bus={createInputBus()} korrid={client} launchId="exact-launch"
    controller={createInMemoryOverlayController()} surface={{
      id: "observer", title: "Observer", displays: "all", presentations: ["gameplay-overlay"],
      render(props) { model = props.model; return null },
    }} />))
  expect(requests).toHaveLength(2)
})
