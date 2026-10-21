import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { createInputBus } from "../input/bus"
import { createControllers } from "../input/controllers"
import { createInMemoryKorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

let root: Root | undefined
afterEach(async () => { await act(async () => root?.unmount()); document.body.innerHTML = "" })
async function settle(check: () => boolean) {
  for (let count = 0; count < 100; count++) { if (check()) return; await act(async () => { await Bun.sleep(5) }) }
  throw new Error("Surface model did not settle")
}
test("SurfaceRoot publishes controller evidence, Connecting and completion through the unchanged treaty", async () => {
  const controllers = createControllers()
  let model: SurfaceModel | undefined
  let host: SurfaceHost | undefined
  const seen: string[] = []
  const container = document.createElement("div")
  document.body.append(container)
  root = createRoot(container)
  await act(async () => root!.render(<SurfaceRoot bus={createInputBus()} korrid={createInMemoryKorridClient()} controllers={controllers} surface={{
    id: "observer", title: "Observer", presentations: ["catalog"],
    render(props) { model = props.model; host = props.host; seen.push(JSON.stringify(model)); return null },
  }} />))
  await settle(() => model?.network !== undefined && model?.onboarding !== undefined)
  await act(async () => controllers.added("pad", "Pad"))
  expect(model?.onboarding?.controllers).toEqual([{ id: "pad", name: "Pad", heard: false }])
  await act(async () => controllers.input("pad"))
  expect(model?.onboarding?.controllers[0]?.heard).toBe(true)
  await act(async () => host!.connectWifi("home-5g", "test-passphrase"))
  const wifi = model?.network?.wifi
  expect(wifi?._tag === "Available" && wifi.attempt._tag).toBe("Connecting")
  await settle(() => model?.network?.connection._tag === "Wifi")
  await act(async () => host!.finishOnboarding())
  await settle(() => model?.onboarding === undefined)
  expect(seen.join()).not.toContain("test-passphrase")

  // Setup finished: Settings offers to run it again. Setup in progress: it
  // does not. Running it brings `onboarding` back and the pad must be heard
  // again. Nothing else changes: the network connection stays.
  const restart = () => model?.settings.flatMap(group => group.items).find(item => item.id === "setup-restart")
  expect(restart()).toMatchObject({
    label: "Run setup again",
    interaction: { kind: "action", actionId: "setup-restart", confirmation: { confirmLabel: "Run setup" } },
  })
  await act(async () => host!.runAction("setup-restart"))
  await settle(() => model?.onboarding !== undefined)
  expect(model?.onboarding?.controllers).toEqual([{ id: "pad", name: "Pad", heard: false }])
  expect(model?.network?.connection._tag).toBe("Wifi")
  expect(model?.status._tag).not.toBe("Problem")
  expect(restart()).toBeUndefined()
  await act(async () => controllers.input("pad"))
  expect(model?.onboarding?.controllers[0]?.heard).toBe(true)
  await act(async () => host!.finishOnboarding())
  await settle(() => model?.onboarding === undefined)
  expect(restart()).toBeDefined()
})

test("SurfaceRoot offers the Starter pack and dispatches Install through runAction only during setup", async () => {
  const client = createInMemoryKorridClient()
  let model: SurfaceModel | undefined
  let host: SurfaceHost | undefined
  const seen: string[] = []
  const container = document.createElement("div")
  document.body.append(container)
  root = createRoot(container)
  await act(async () => root!.render(<SurfaceRoot bus={createInputBus()} korrid={client} surface={{
    id: "observer", title: "Observer", presentations: ["catalog"],
    render(props) { model = props.model; host = props.host; const tag = model.onboarding?.starterPack?._tag; if (tag) seen.push(tag); return null },
  }} />))
  await settle(() => model?.onboarding?.starterPack?._tag === "Offered")
  await act(async () => host!.runAction("starter-pack-install"))
  await settle(() => model?.onboarding?.starterPack?._tag === "Installed")
  expect(seen).toContain("Installing")
  await act(async () => host!.finishOnboarding())
  await settle(() => model?.onboarding === undefined)
  await act(async () => host!.runAction("setup-restart"))
  await settle(() => model?.onboarding !== undefined)
  await act(async () => { await Bun.sleep(5) })
  expect(model?.onboarding?.starterPack).toBeUndefined()
})

test("finishing setup mid-install still rescans and refreshes the library after discovery", async () => {
  const base = createInMemoryKorridClient({ discoveryScan: { delayMs: 10 } })
  let releaseEnable!: () => void
  const enabled = new Promise<void>(resolve => { releaseEnable = resolve })
  let rescans = 0
  let libraryReads = 0
  const client = { ...base,
    async setPluginEnabled(id: string, value: boolean) { await enabled; return base.setPluginEnabled(id, value) },
    async rescanDiscovery() { rescans++; return base.rescanDiscovery() },
    async localGames() { libraryReads++; return base.localGames() },
  }
  let model: SurfaceModel | undefined
  let host: SurfaceHost | undefined
  const container = document.createElement("div")
  document.body.append(container)
  root = createRoot(container)
  await act(async () => root!.render(<SurfaceRoot bus={createInputBus()} korrid={client} surface={{
    id: "observer", title: "Observer", presentations: ["catalog"],
    render(props) { model = props.model; host = props.host; return null },
  }} />))
  await settle(() => model?.onboarding?.starterPack?._tag === "Offered")
  await act(async () => host!.runAction("starter-pack-install"))
  expect(model?.onboarding?.starterPack?._tag).toBe("Installing")
  await act(async () => host!.finishOnboarding())
  await settle(() => model?.onboarding === undefined)
  const before = libraryReads
  await act(async () => releaseEnable())
  await settle(() => rescans === 1 && libraryReads > before)
  expect(model?.onboarding).toBeUndefined()
  const repositories = await base.pluginRepositories()
  expect(repositories._tag).toBe("Ok")
  if (repositories._tag === "Ok") expect(repositories.payload.otherInstalls.find(install => install.id === "@korri:starter-pack")?.enabled).toBe(true)
})

test("SurfaceRoot unmount cleans up an active Starter pack discovery wait", async () => {
  const base = createInMemoryKorridClient({ discoveryScan: { results: [{ _tag: "Enriching", payload: {} }] } })
  let reads = 0
  const client = { ...base,
    async discoverySnapshot() { reads++; return base.discoverySnapshot() },
  }
  let model: SurfaceModel | undefined
  let host: SurfaceHost | undefined
  const container = document.createElement("div")
  document.body.append(container)
  root = createRoot(container)
  await act(async () => root!.render(<SurfaceRoot bus={createInputBus()} korrid={client} surface={{
    id: "observer", title: "Observer", presentations: ["catalog"],
    render(props) { model = props.model; host = props.host; return null },
  }} />))
  await settle(() => model?.onboarding?.starterPack?._tag === "Offered")
  const before = reads
  await act(async () => host!.runAction("starter-pack-install"))
  await settle(() => reads > before)
  expect(model?.onboarding?.starterPack?._tag).toBe("Installing")
  await act(async () => root!.unmount())
  root = undefined
  const stopped = reads
  await Bun.sleep(300)
  expect(reads).toBe(stopped)
})
