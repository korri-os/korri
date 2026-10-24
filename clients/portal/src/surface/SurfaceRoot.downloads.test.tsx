/**
 * Downloads asked for from a game rather than from a launch (ADR 0013), end
 * to end: the in-memory brain declares a game whose release comes from an
 * artifact, Pico renders what the host publishes, and the host carries the
 * person's Download, Cancel download and Remove download back as RPC. The
 * download runs in the background, one at a time, and the launch gate never
 * cancels one it did not start.
 */
import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceAction, SurfaceHost, SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import type { Game } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type InMemoryKorridClientConfig } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
  document.body.innerHTML = ""
})
const invoke = async (fn: () => void) => { await act(async () => fn()) }
const waitFor = async (fn: () => boolean, tries = 1500) => {
  for (let i = 0; i < tries; i++) {
    if (fn()) return
    await act(async () => { await new Promise(done => setTimeout(done, 10)) })
  }
  throw new Error("condition did not become true")
}
const local = { label: "This device", isLocal: true }
const megaman: Game = { id: "mm", title: "Mega Man Rock N Roll", supportsRunnerSelection: false, source: local }
const hollow: Game = { id: "hk", title: "Hollow Knight", supportsRunnerSelection: false, source: local }
/** The game's own files, and the runtime its runner needs. */
const content = { id: "@megaman:rnr/zip", title: "Mega Man Rock N Roll", bytesTotal: 400_000_000 }
const godot = { id: "@korri:godot/runtime", title: "Godot runtime", bytesTotal: 100_000_000 }
const rootfs = { id: "@korri:fex/rootfs", title: "FEX rootfs", bytesTotal: 1_160_000_000 }

async function mount(config: InMemoryKorridClientConfig = {}, opened = "game:mm") {
  const korrid = createInMemoryKorridClient({
    games: [megaman, hollow],
    artifacts: [content, godot, rootfs],
    releaseArtifacts: { mm: [content.id] },
    gameArtifacts: { mm: [godot.id], hk: [rootfs.id] },
    acquisitionBytesPerStep: 100_000_000, acquisitionUnpackSteps: 0,
    sessionFocusDelayMs: 0,
    ...config,
  })
  await korrid.finishOnboarding()
  let model!: SurfaceModel
  let host!: SurfaceHost
  const bus = createInputBus()
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.push(root)
  await act(async () => root.render(<SurfaceRoot bus={bus} korrid={korrid} surface={{
    id: "acceptance", title: "Acceptance", presentations: ["catalog"],
    render: props => {
      model = props.model
      host = props.host
      return <PicoSurface {...props} initialView={{ _tag: "Detail", gameId: opened }} />
    },
  }} />))
  await waitFor(() => model.catalog._tag === "Ready")
  const game = (id: string) => model.catalog._tag === "Ready" ? model.catalog.games.find(candidate => candidate.id === id) : undefined
  // Launch settings is offered on every local game; these tests are about downloads.
  const actions = (id: string) => host.gameActions(id).map(action => action.id).filter(action => action !== "launch-settings")
  const action = (label: string) => {
    const status: SurfaceStatus = model.status
    if (status._tag !== "Confirm" && status._tag !== "Busy") throw new Error(`no actions on ${status._tag}`)
    const found = (status.actions ?? []).find(candidate => candidate.label === label)
    if (!found) throw new Error(`no ${label}`)
    return found.id
  }
  const button = (name: string) => [...container.querySelectorAll("button")].find(candidate => candidate.textContent === name)
  const state = async (id: string) => {
    const all = await korrid.acquisitionStatus()
    return all._tag === "Ok" ? all.payload.artifacts.find(artifact => artifact.id === id)?.state._tag : undefined
  }
  return {
    korrid, container, bus, model: () => model, host: () => host, game, actions, action, button, state,
    async back() { await invoke(() => bus.emit({ type: "back" })) },
  }
}

test("a game whose own files are not here is in the library before any download, with its size and Download", async () => {
  const view = await mount()
  await waitFor(() => view.game("game:mm")?.download !== undefined)
  // Its own files and its runner's runtime, before anything was asked.
  expect(view.game("game:mm")?.download).toEqual({ bytes: 500_000_000 })
  await waitFor(() => view.actions("game:mm").length > 0)
  expect(view.host().gameActions("game:mm")).toEqual([
    { id: "download", label: "Download", enabled: true },
    { id: "launch-settings", label: "Launch settings", enabled: true },
  ])
  expect(view.container.textContent).toContain("500 MB DOWNLOAD")
  expect(view.button("Download")).toBeDefined()
  // Nothing was fetched to show it.
  expect(await view.state(content.id)).toBe("Missing")
}, 30_000)

test("Download runs in the background: the game shows its bar and Cancel download, leaving does not stop it, and it ends Ready", async () => {
  const view = await mount()
  await waitFor(() => view.button("Download") !== undefined)
  await invoke(() => { view.button("Download")!.focus(); view.button("Download")!.click() })
  // korrid takes the whole default route, one artifact at a time.
  await waitFor(() => view.game("game:mm")?.download?.progress !== undefined)
  expect(view.model().status).toEqual({ _tag: "Browsing" })
  expect(view.actions("game:mm")).toEqual(["cancel-download"])
  const bar = view.container.querySelector('[role="progressbar"]')
  expect(bar?.getAttribute("data-meter")).toBe("progress")
  expect(bar?.getAttribute("aria-label")).toMatch(/^DOWNLOADING \d+%$/)
  expect(view.container.textContent).toMatch(/DOWNLOADING \d+%/)
  // The cursor went with the pressed row to the one that replaced it.
  expect(document.activeElement?.textContent).toBe("Cancel download")

  // Leave the game: the download goes on, and the shelf says so.
  await view.back()
  await waitFor(() => view.container.querySelector('[role="progressbar"]') === null)
  const left = view.game("game:mm")?.download?.progress?.done ?? 0
  await waitFor(() => (view.game("game:mm")?.download?.progress?.done ?? 0) > left)

  // It finishes on its own: nothing left to download, and its own files can be removed.
  await waitFor(() => view.game("game:mm")?.download === undefined && view.game("game:mm") !== undefined)
  expect(await view.state(content.id)).toBe("Ready")
  expect(await view.state(godot.id)).toBe("Ready")
  expect(view.actions("game:mm")).toEqual(["remove-download"])
}, 30_000)

test("queued behind another download, then Cancel download takes it off the queue and deletes nothing else", async () => {
  const view = await mount({ acquisitionBytesPerStep: 20_000_000 }, "game:hk")
  // Hollow Knight's runtime downloads first.
  await waitFor(() => view.button("Download") !== undefined)
  await invoke(() => view.button("Download")!.click())
  await waitFor(() => view.game("game:hk")?.download?.progress !== undefined)
  // Mega Man is asked for second, and waits its turn.
  await waitFor(() => view.actions("game:mm").includes("download"))
  await invoke(() => view.host().runGameAction("game:mm", "download"))
  await waitFor(() => view.game("game:mm")?.download?.queued === true)
  expect(view.game("game:mm")?.download).toEqual({ bytes: 500_000_000, queued: true })
  expect(view.actions("game:mm")).toEqual(["cancel-download"])

  await invoke(() => view.host().runGameAction("game:mm", "cancel-download"))
  await waitFor(() => view.actions("game:mm").includes("download"))
  expect(view.game("game:mm")?.download).toEqual({ bytes: 500_000_000 })
  expect(await view.state(content.id)).toBe("Missing")
  expect(await view.state(godot.id)).toBe("Missing")
  // The other game's download was not touched.
  expect(["Downloading", "Unpacking", "Ready"]).toContain((await view.state(rootfs.id))!)
}, 30_000)

test("Cancel download while it runs: korrid stops it and deletes the partial file", async () => {
  const view = await mount({ acquisitionBytesPerStep: 20_000_000 })
  await waitFor(() => view.button("Download") !== undefined)
  await invoke(() => view.button("Download")!.click())
  await waitFor(() => (view.game("game:mm")?.download?.progress?.done ?? 0) > 0)
  await waitFor(() => view.button("Cancel download") !== undefined)
  await invoke(() => view.button("Cancel download")!.click())
  await waitFor(() => view.button("Download") !== undefined)
  expect(view.game("game:mm")?.download).toEqual({ bytes: 500_000_000 })
  expect(await view.state(content.id)).toBe("Missing")
  const again = await view.korrid.acquisitionStart([content.id])
  expect(again._tag === "Ok" ? again.payload.artifacts[0]?.state : undefined).toMatchObject({ _tag: "Downloading", bytesDone: 0 })
}, 30_000)

test("Remove download asks first on Pico, then korrid deletes the game's own files and keeps the runtime", async () => {
  const view = await mount({ artifacts: [{ ...content, ready: true }, { ...godot, ready: true }, rootfs] })
  await waitFor(() => view.button("Remove download") !== undefined)
  const remove = view.host().gameActions("game:mm").find(candidate => candidate.id === "remove-download")
  expect(remove).toMatchObject({ label: "Remove download", destructive: true, enabled: true })
  await invoke(() => { view.button("Remove download")!.focus(); view.button("Remove download")!.click() })
  // Nothing is removed until the person says yes.
  expect(await view.state(content.id)).toBe("Ready")
  const dialog = view.container.querySelector('[role="dialog"]')
  expect(dialog?.textContent).toContain("REMOVE DOWNLOAD?")
  expect(dialog?.textContent).toContain("Deletes its downloaded files from this device. Your saves stay.")
  await invoke(() => view.button("REMOVE DOWNLOAD")!.click())
  await waitFor(() => view.game("game:mm")?.download !== undefined)
  expect(view.game("game:mm")?.download).toEqual({ bytes: 400_000_000 })
  expect(await view.state(content.id)).toBe("Missing")
  expect(await view.state(godot.id)).toBe("Ready")
  await waitFor(() => view.button("Download") !== undefined)
  expect(document.activeElement?.textContent).toBe("Download")
}, 30_000)

test("Remove download is not offered while the game runs, and a refusal korrid makes reads as plain words", async () => {
  const view = await mount({ artifacts: [{ ...content, ready: true }, { ...godot, ready: true }, rootfs] })
  await waitFor(() => view.actions("game:mm").includes("remove-download"))
  // Launched from Pico: everything is Ready, so the gate steps aside.
  await invoke(() => view.host().launchGame("game:mm"))
  await waitFor(() => { const catalog = view.model().catalog; return catalog._tag === "Ready" && catalog.games.some(game => game.id.startsWith("now-playing:")) })
  expect(view.actions("game:mm")).not.toContain("remove-download")

  // The session ends, and Remove download is back. Then korrid refuses it
  // anyway, as it would if the game started again elsewhere meanwhile.
  const live = await view.korrid.sessionStatus()
  const launchId = live._tag === "Ok" ? live.payload.active?.launchId : undefined
  expect(launchId).toBeDefined()
  expect(await view.korrid.sessionEnd(launchId)).toMatchObject({ _tag: "Ok" })
  await waitFor(() => view.actions("game:mm").includes("remove-download"))
  view.korrid.acquisitionRemove = async () => ({ _tag: "Err", payload: { code: "ActiveSessionConflict", message: "session 7f3a is active" } })
  await invoke(() => view.host().runGameAction("game:mm", "remove-download"))
  await waitFor(() => view.model().status._tag === "Problem")
  expect(view.model().status).toEqual({
    _tag: "Problem", kicker: "Couldn't remove the download",
    reason: "Mega Man Rock N Roll is running. Stop it first, then remove the download.",
    canRetry: false, gameId: "game:mm", gameTitle: "Mega Man Rock N Roll",
  })
  expect(view.container.textContent).toContain("Mega Man Rock N Roll is running. Stop it first, then remove the download.")
  expect(view.container.textContent).not.toContain("ActiveSessionConflict")
  expect(view.container.querySelector('.pico-card[data-tone="warn"]')).not.toBeNull()
  // Back acknowledges it; the files are still there.
  await view.back()
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(await view.state(content.id)).toBe("Ready")
}, 30_000)

test("Play asks once with everything not ready, the game's own files and its runtime together", async () => {
  const view = await mount({ acquisitionBytesPerStep: 300_000_000 })
  await waitFor(() => view.game("game:mm")?.download !== undefined)
  await invoke(() => view.host().launchGame("game:mm"))
  await waitFor(() => view.model().status._tag === "Confirm")
  expect(view.model().status).toMatchObject({
    _tag: "Confirm", kicker: "Needs a download", detail: "Mega Man Rock N Roll and Godot runtime, 500 MB", gameId: "game:mm",
  })
  await invoke(() => view.host().runAction(view.action("Download")))
  await waitFor(() => { const s = view.model().status; return s._tag === "Busy" && s.kicker === "Downloading Mega Man Rock N Roll" })
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(await view.korrid.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { gameId: "mm" } } })
}, 30_000)

test("Play while a background download runs joins it; leaving the launch never cancels it", async () => {
  const view = await mount({ acquisitionBytesPerStep: 10_000_000 })
  await waitFor(() => view.button("Download") !== undefined)
  await invoke(() => view.button("Download")!.click())
  await waitFor(() => view.game("game:mm")?.download?.progress !== undefined)
  const cancels: string[] = []
  const cancel = view.korrid.acquisitionCancel.bind(view.korrid)
  view.korrid.acquisitionCancel = id => {
    cancels.push(id)
    return cancel(id)
  }

  // Play: the person already said yes, so no question; the gate watches.
  await invoke(() => view.host().launchGame("game:mm"))
  await waitFor(() => { const s = view.model().status; return s._tag === "Busy" && s.kicker === "Downloading Mega Man Rock N Roll" })
  const busy = view.model().status
  const leave = busy._tag === "Busy" ? busy.actions?.[0] : undefined
  expect(leave).toMatchObject({ label: "Play later", enabled: true } satisfies Partial<SurfaceAction>)
  expect(view.button("Play later")).toBeDefined()
  await invoke(() => view.button("Play later")!.click())
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(cancels).toEqual([])
  expect(["Downloading", "Queued"]).toContain((await view.state(content.id))!)
  expect(await view.korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })

  // Back from the launch is the same: the download goes on.
  await invoke(() => view.host().launchGame("game:mm"))
  await waitFor(() => view.model().status._tag === "Busy")
  await view.back()
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(cancels).toEqual([])
  expect(["Downloading", "Queued"]).toContain((await view.state(content.id))!)
  expect(view.actions("game:mm")).toEqual(["cancel-download"])
}, 30_000)
