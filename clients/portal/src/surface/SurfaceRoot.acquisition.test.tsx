/**
 * The download before a first launch, end to end: the in-memory brain
 * declares artifacts a game needs, Pico renders what the host publishes, and
 * the host carries the user's answers back as RPC and then into the shared
 * reserve/start lifecycle (ADR 0012).
 */
import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import type { Game } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type InMemoryKorridClientConfig } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"
import { runnerRoutes } from "./fixtures/runner-routes"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
  document.body.innerHTML = ""
})
const invoke = async (fn: () => void) => { await act(async () => fn()) }
const waitFor = async (fn: () => boolean, tries = 1000) => {
  for (let i = 0; i < tries; i++) {
    if (fn()) return
    await act(async () => { await new Promise(done => setTimeout(done, 10)) })
  }
  throw new Error("condition did not become true")
}
const hollow: Game = { id: "hk", title: "Hollow Knight", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }
const celeste: Game = { id: "celeste", title: "Celeste", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }
const rootfs = { id: "@korri:fex/rootfs", title: "FEX rootfs", bytesTotal: 1_160_000_000 }
const proton = { id: "@korri:proton/cachyos", title: "Proton-CachyOS", bytesTotal: 340_000_000 }

async function mount(config: InMemoryKorridClientConfig = {}) {
  const korrid = createInMemoryKorridClient({
    games: [hollow, celeste],
    artifacts: [rootfs, proton],
    gameArtifacts: { hk: [rootfs.id, proton.id] },
    // One step per host poll: ~4 reads to fetch the rootfs, one to unpack.
    acquisitionBytesPerStep: 300_000_000, acquisitionUnpackSteps: 1,
    // Keep the shared lifecycle's own Busy on screen long enough to see it.
    sessionFocusDelayMs: 1500,
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
      return <PicoSurface {...props} initialView={{ _tag: "Detail", gameId: "game:hk" }} />
    },
  }} />))
  await waitFor(() => model.catalog._tag === "Ready")
  const action = (label: string) => {
    const status: SurfaceStatus = model.status
    if (status._tag !== "Confirm" && status._tag !== "Busy") throw new Error(`no actions on ${status._tag}`)
    const found = (status.actions ?? []).find(candidate => candidate.label === label)
    if (!found) throw new Error(`no ${label}`)
    return found.id
  }
  return {
    korrid, container, bus, model: () => model, host: () => host, action,
    game: (id: string) => model.catalog._tag === "Ready" ? model.catalog.games.find(game => game.id === id) : undefined,
    async back() { await invoke(() => bus.emit({ type: "back" })) },
  }
}

test("a game whose runner needs downloads wears the size; one that does not, nothing", async () => {
  const view = await mount()
  await waitFor(() => view.game("game:hk")?.download !== undefined)
  expect(view.game("game:hk")?.download).toEqual({ bytes: 1_500_000_000 })
  expect(view.game("game:celeste")?.download).toBeUndefined()
  expect(view.container.textContent).toContain("1.5 GB DOWNLOAD")
}, 30_000)

test("Play -> Confirm on Pico -> Download -> real progress -> Unpacking -> the shared launch, then the badge is gone", async () => {
  const view = await mount()
  await waitFor(() => view.game("game:hk")?.download !== undefined)
  await invoke(() => view.host().launchGame("game:hk"))
  await waitFor(() => view.model().status._tag === "Confirm")
  expect(view.model().status).toMatchObject({
    _tag: "Confirm", kicker: "Needs a download", detail: "FEX rootfs and Proton-CachyOS, 1.5 GB", gameId: "game:hk",
    actions: [{ label: "Download", enabled: true }, { label: "Cancel", enabled: true }],
  })
  // Pico draws the question as the yellow card with the host's two rows.
  expect(view.container.querySelector('.pico-card[data-tone="ask"]')).not.toBeNull()
  expect([...view.container.querySelectorAll("button")].map(button => button.textContent)).toEqual(["Download", "Cancel"])
  // Nothing was reserved: the question comes before the launch.
  expect(await view.korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })

  await invoke(() => view.host().runAction(view.action("Download")))
  await waitFor(() => { const s = view.model().status; return s._tag === "Busy" && s.progress !== undefined })
  const busy = view.model().status
  expect(busy).toMatchObject({ _tag: "Busy", kicker: "Downloading FEX rootfs", actions: [{ label: "Cancel" }], gameId: "game:hk" })
  expect(busy._tag === "Busy" && busy.progress?.total).toBe(1_160_000_000)
  expect(view.container.querySelector('[role="progressbar"][data-meter="progress"]')).not.toBeNull()
  await waitFor(() => { const s = view.model().status; return s._tag === "Busy" && s.kicker === "Unpacking FEX rootfs" })
  expect(view.model().status).toEqual({ _tag: "Busy", kicker: "Unpacking FEX rootfs", progress: { done: 0 }, gameId: "game:hk" })
  expect(view.container.querySelector('[role="progressbar"][data-meter="indeterminate"]')).not.toBeNull()
  // Every artifact Ready: the shared lifecycle takes over with its own Busy.
  await waitFor(() => { const s = view.model().status; return s._tag === "Busy" && s.kicker === "Starting Hollow Knight…" && s.detail === "Opening your session" })
  await waitFor(() => view.model().status._tag === "Browsing")
  const status = await view.korrid.sessionStatus()
  expect(status).toMatchObject({ _tag: "Ok", payload: { active: { gameId: "hk" } } })
  await waitFor(() => view.game("game:hk")?.download === undefined && view.game("game:hk") !== undefined)
}, 30_000)

test("Back on the question is Cancel: nothing starts, nothing is reserved", async () => {
  const view = await mount()
  await invoke(() => view.host().launchGame("game:hk"))
  await waitFor(() => view.model().status._tag === "Confirm")
  await view.back()
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(await view.korrid.acquisitionStatus("hk")).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ state: { _tag: "Missing" } }, { state: { _tag: "Missing" } }] } })
  expect(await view.korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
}, 30_000)

test("Cancel while downloading stops korrid's download and keeps the bytes; the game is not started", async () => {
  const view = await mount({ acquisitionBytesPerStep: 100_000_000 })
  await invoke(() => view.host().launchGame("game:hk"))
  await waitFor(() => view.model().status._tag === "Confirm")
  await invoke(() => view.host().runAction(view.action("Download")))
  await waitFor(() => { const s = view.model().status; return s._tag === "Busy" && (s.progress?.done ?? 0) > 0 })
  await invoke(() => view.host().runAction(view.action("Cancel")))
  await waitFor(() => view.model().status._tag === "Browsing")
  const after = await view.korrid.acquisitionStatus("hk")
  expect(after).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ id: rootfs.id, state: { _tag: "Missing" } }, { id: proton.id, state: { _tag: "Missing" } }] } })
  expect(await view.korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  // The next start resumes from the kept bytes.
  const resumed = await view.korrid.acquisitionStart([rootfs.id])
  const state = resumed._tag === "Ok" ? resumed.payload.artifacts[0]?.state : undefined
  expect(state?._tag === "Downloading" && state.bytesDone).toBeGreaterThan(0)
  await view.korrid.acquisitionCancel(rootfs.id)
}, 30_000)

test("a failed download is a Problem in korrid's words; Retry resumes it and the launch follows", async () => {
  const view = await mount({
    artifacts: [{ ...rootfs, bytesTotal: 600_000_000, failure: { message: "The connection stalled. Check the network and try again.", atFraction: 0.5 } }, { ...proton, ready: true }],
    acquisitionBytesPerStep: 300_000_000,
  })
  await invoke(() => view.host().launchGame("game:hk"))
  await waitFor(() => view.model().status._tag === "Confirm")
  expect(view.model().status).toMatchObject({ _tag: "Confirm", detail: "FEX rootfs, 600 MB" })
  await invoke(() => view.host().runAction(view.action("Download")))
  await waitFor(() => view.model().status._tag === "Problem")
  expect(view.model().status).toEqual({
    _tag: "Problem", kicker: "Couldn't download FEX rootfs",
    reason: "The connection stalled. Check the network and try again.",
    canRetry: true, gameId: "game:hk", gameTitle: "Hollow Knight",
  })
  expect(view.container.querySelector('.pico-card[data-tone="warn"]')).not.toBeNull()
  // The in-memory brain fails once per start; the retried start carries on.
  await invoke(() => view.host().retry())
  await waitFor(() => view.model().status._tag === "Busy")
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(await view.korrid.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { gameId: "hk" } } })
}, 30_000)

test("a game that needs nothing launches at once, as before", async () => {
  const view = await mount({ gameArtifacts: { hk: [] } })
  await invoke(() => view.host().launchGame("game:hk"))
  await waitFor(() => { const s = view.model().status; return s._tag === "Busy" && s.detail === "Opening your session" })
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(await view.korrid.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { gameId: "hk" } } })
}, 30_000)

test("a game with several runners and no saved choice: the chooser first, then the gate with the chosen runner", async () => {
  const chooser: Game = { ...hollow, supportsRunnerSelection: true }
  const view = await mount({
    games: [chooser, celeste],
    gameRoutes: [{ ...runnerRoutes, gameId: "hk", gameRunner: undefined, systemRunners: {} }],
    // Only one runner's build needs the artifacts.
    gameArtifacts: { "hk@retroarch/mgba": [rootfs.id], "hk@retroarch/mgba-nightly": [] },
  })
  const reads: (string | undefined)[][] = []
  const status = view.korrid.acquisitionStatus.bind(view.korrid)
  view.korrid.acquisitionStatus = (gameId, runnerId) => {
    if (gameId !== undefined) reads.push([gameId, runnerId])
    return status(gameId, runnerId)
  }
  await invoke(() => view.host().launchGame("game:hk"))
  await waitFor(() => view.model().runnerChoice?._tag === "Ready")
  // No gate before the choice: korrid cannot resolve artifacts without a runner.
  expect(view.model().status).toEqual({ _tag: "Browsing" })
  expect(reads).toEqual([])
  const choice = view.model().runnerChoice
  if (!choice || choice._tag === "Closed") throw new Error("expected routes")
  const launch = choice.routes[0]?.actions[0]
  if (!launch) throw new Error("expected launch action")
  await invoke(() => view.host().runAction(launch.id))
  await waitFor(() => view.model().status._tag === "Confirm")
  expect(reads).toEqual([["hk", "retroarch/mgba"]])
  expect(view.model().runnerChoice?._tag).toBe("Closed")
  expect(view.model().status).toMatchObject({ _tag: "Confirm", detail: "FEX rootfs, 1.2 GB", gameId: "game:hk" })
  // Cancel here closes the gate and leaves nothing reserved and no chooser panel.
  await view.back()
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(view.model().runnerChoice?._tag).toBe("Closed")
  expect(await view.korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
}, 30_000)
