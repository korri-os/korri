import { afterEach, expect, mock, test } from "bun:test"
import * as React from "react"
import * as ReactJsxRuntime from "react/jsx-runtime"
import { createRoot, type Root } from "react-dom/client"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient } from "../korrid/client"
import type { PortalSurface } from "./surface-registry"

// Same single-React arrangement as SurfaceRoot.test.tsx.
const shiftPackageRoot = new URL("../", import.meta.resolve("@korri/shift"))
mock.module("react", () => React)
mock.module("react/jsx-runtime", () => ReactJsxRuntime)
mock.module(new URL("node_modules/react/index.js", shiftPackageRoot).pathname, () => React)
mock.module(new URL("node_modules/react/jsx-runtime.js", shiftPackageRoot).pathname, () => ReactJsxRuntime)
// A render loop never lets act() drain, so this test runs React's real
// scheduler and only watches the render count.
;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = false

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms))
let root: Root | undefined

afterEach(() => {
  root?.unmount()
  root = undefined
  document.body.innerHTML = ""
})

const identityKorrid = () => ({
  ...createInMemoryKorridClient(),
  // A device with an owner identity: the host publishes identity management.
  async identityStatus() {
    return {
      _tag: "Ok" as const,
      payload: { localBackupAvailable: true, retiredPublicKeys: [] },
    }
  },
})

function mount(surface: PortalSurface, SurfaceRoot: typeof import("./SurfaceRoot").SurfaceRoot) {
  const container = document.createElement("div")
  document.body.append(container)
  root = createRoot(container)
  root.render(<SurfaceRoot bus={createInputBus()} korrid={identityKorrid()} surface={surface} />)
}

test("dismissing an already idle identity status publishes no new model", async () => {
  const { SurfaceRoot } = await import("./SurfaceRoot")
  let renders = 0
  // A surface that dismisses on every render, as a careless one may.
  function Dismisser({ dismiss }: { readonly dismiss: () => void }) {
    React.useEffect(() => dismiss())
    return null
  }
  mount({
    id: "dismisser",
    title: "Dismisser",
    presentations: ["catalog"],
    render: ({ host }) => {
      renders += 1
      return <Dismisser dismiss={() => host.dismissIdentityStatus()} />
    },
  }, SurfaceRoot)

  await sleep(500)
  const settled = renders
  await sleep(500)

  expect(renders - settled).toBeLessThan(3)
})

test("the idle catalog stops rendering once identity status has loaded", async () => {
  const { SurfaceRoot } = await import("./SurfaceRoot")
  const { ShiftSurface } = await import("@korri/shift")
  let renders = 0
  const surface: PortalSurface = {
    id: "shift",
    title: "Shift",
    presentations: ["catalog"],
    render: ({ model, host }) => {
      renders += 1
      return <ShiftSurface host={host} model={model} />
    },
  }
  mount(surface, SurfaceRoot)

  await sleep(500)
  const settled = renders
  await sleep(500)

  expect(renders - settled).toBeLessThan(3)
})
