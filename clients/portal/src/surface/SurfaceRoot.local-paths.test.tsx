import { afterEach, expect, spyOn, test } from "bun:test"
import { act, type ComponentProps } from "react"
import { within } from "@testing-library/react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { FocusOwnership, InitialHandoff, type ActiveSession } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type InMemoryKorridClientConfig, type KorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"
import { miniCatalog, miniLocalGames, miniSession } from "./fixtures/mini-capture-3"
import { runnerRoutes } from "./fixtures/runner-routes"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
  document.body.innerHTML = ""
})
const invoke = async (action: () => void) => { await act(async () => action()) }
const waitFor = async (predicate: () => boolean) => {
  for (let attempt = 0; attempt < 200; attempt++) {
    if (predicate()) return
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 10)) })
  }
  throw new Error("condition did not become true")
}
if (miniLocalGames._tag !== "Ok") throw new Error("expected local games capture")
const game = miniLocalGames.payload.games[0]!
const id = `local-game:${game.id}`

async function mount(initialView: ComponentProps<typeof PicoSurface>["initialView"], config: InMemoryKorridClientConfig = {}) {
  if (miniCatalog._tag !== "Ok") throw new Error("expected catalog capture")
  const base = createInMemoryKorridClient({
    games: miniCatalog.payload.games,
    localGames: [game],
    onboardingNeeded: false,
    gameRoutes: [{ ...runnerRoutes, gameId: game.id, gameRunner: undefined, systemRunners: {}, routes: runnerRoutes.routes.slice(0, 1) }],
    ...config,
  })
  const reserve = spyOn(base, "sessionReserve")
  const start = spyOn(base, "sessionStart")
  const prepare = spyOn(base, "sessionPrepare")
  const resume = spyOn(base, "sessionResume")
  const streamPlay = spyOn(base, "invitesStreamPlay")
  const korrid: KorridClient = {
    ...base,
    async catalogSnapshot() { return miniCatalog },
    async localGames() { return miniLocalGames },
    async sessionStatus() {
      const result = await base.sessionStatus()
      return result._tag === "Ok" && Object.keys(result.payload).length === 0
        ? miniSession : result
    },
  }
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
      return <PicoSurface {...props} initialView={initialView} />
    },
  }} />))
  await waitFor(() => model.catalog._tag === "Ready")
  expect(model.status).toMatchObject({ _tag: "Problem", reason: expect.stringContaining("rg35xxpro") })
  await invoke(() => host.dismiss())
  expect(model.status._tag).toBe("Browsing")
  const ui = within(container)
  const click = async (name: string | RegExp) => { await invoke(() => ui.getByRole("button", { name }).click()) }
  const play = async () => {
    await click(/PLAY/)
    expect(container.textContent).toContain("PLAY WHERE?")
    expect(reserve).not.toHaveBeenCalled()
    await click("This device")
  }
  return {
    base, reserve, start, prepare, resume, streamPlay, ui, click, play, container, bus,
    host: () => host, model: () => model,
    async expectLocalLaunch() {
      expect(container.textContent).not.toContain("Korri cannot start this entry on this device.")
      await waitFor(() => model.catalog._tag === "Ready" && model.catalog.games.some(game => game.resumable))
      expect(await base.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { gameId: game.id } } })
      expect(reserve).toHaveBeenCalledTimes(1)
      expect(prepare).not.toHaveBeenCalled()
    },
  }
}

for (const mode of ["shelf", "grid", "hero"] as const) {
  test(`captured catalog: ${mode} tile -> detail Play -> This device`, async () => {
    const view = await mount({ _tag: "Home", mode })
    const tile = mode === "hero"
      ? view.ui.getByRole("region", { name: game.title }).querySelector("button")!
      : view.ui.getByRole("button", { name: new RegExp(game.title.replace(/[()]/g, "\\$&")) })
    await invoke(() => tile.click())
    await view.play()
    await view.expectLocalLaunch()
  })
}

for (const order of ["korri", "recent"] as const) {
  test(`captured catalog: Find (${order}) search -> detail -> This device`, async () => {
    const view = await mount({ _tag: "Find", order })
    for (const letter of "TURTLES") await view.click(`Type ${letter}`)
    await view.click(/Teenage Mutant/)
    await view.play()
    await view.expectLocalLaunch()
  })
}

test("captured catalog: direct detail Play and both host launch calls preserve the exact local choice", async () => {
  const view = await mount({ _tag: "Detail", gameId: id })
  expect(view.model().invites).toEqual([])
  expect(view.container.textContent).not.toContain("PLAYING NOW")
  await invoke(() => view.host().launchGame(id))
  expect(view.reserve).not.toHaveBeenCalled()
  await view.play()
  await view.expectLocalLaunch()
})

for (const device of ["odin2portal", "rg35xxsp"]) {
  test(`captured catalog: an explicit ${device} choice never becomes a local launch`, async () => {
    const view = await mount({ _tag: "Detail", gameId: id })
    await invoke(() => view.host().launchGame(id, "unknown-location"))
    expect(view.reserve).not.toHaveBeenCalled()
    expect(view.prepare).not.toHaveBeenCalled()
    const catalog = view.model().catalog
    if (catalog._tag !== "Ready") throw new Error("expected catalog")
    const location = catalog.games.find(game => game.id === id)!.launchLocations!.find(location => location.label === device)!
    await invoke(() => view.host().launchGame(id, location.id))
    expect(view.prepare).toHaveBeenCalledWith(game.id, device)
    expect(view.reserve).not.toHaveBeenCalled()
    expect(view.container.textContent).not.toContain("cannot start this entry")
  })
}

test("captured catalog: runGameAction starts the same folded local game", async () => {
  const view = await mount({ _tag: "Detail", gameId: id })
  await invoke(() => view.host().runGameAction(id, "play"))
  await view.expectLocalLaunch()
})

// The idle capture has no Continue entry. Seed a real in-memory session to
// exercise the additional route without pretending it was in sess3.json.
for (const route of ["Continue", "runGameAction"] as const) {
  test(`captured catalog plus live session: ${route} resumes without a second launch`, async () => {
    const session = { launchId: "local-path-session", gameId: game.id, phase: "running", focusOwnership: FocusOwnership.Launch, initialHandoff: InitialHandoff.Observed }
    const view = await mount({ _tag: "Find", section: "Continue" }, { activeSession: session })
    if (route === "Continue") {
      // Linux's status omits title, so the banner uses the captured game id.
      await view.click(/teenage-mutant-ninja-turtles/)
      await view.click(/CONTINUE/)
    } else await invoke(() => view.host().runGameAction(`now-playing:${session.launchId}`, "resume"))
    expect(view.resume).toHaveBeenCalledWith(session.launchId)
    expect(view.reserve).not.toHaveBeenCalled()
    expect(view.prepare).not.toHaveBeenCalled()
  })
}

test("captured catalog after a local session ends: the shelf can launch it again", async () => {
  const view = await mount({ _tag: "Home" }, { activeSession: {
    launchId: "ended-path-session", gameId: game.id, phase: "running", focusOwnership: FocusOwnership.Launch, initialHandoff: InitialHandoff.Observed,
  } })
  await act(async () => { await view.base.sessionEnd("ended-path-session") })
  await invoke(() => view.host().reload())
  await waitFor(() => {
    const catalog = view.model().catalog
    return catalog._tag === "Ready" && !catalog.games.some(game => game.resumable)
  })
  await invoke(() => view.host().dismiss())
  await view.click(/Teenage Mutant/)
  await view.play()
  await view.expectLocalLaunch()
})

test("captured catalog plus invite: PLAYING NOW streams; Ended dismisses before a new local Play", async () => {
  let remote: ActiveSession | undefined = { launchId: "far-session", gameId: game.id, title: game.title, phase: "running" }
  const view = await mount({ _tag: "Home" }, {
    invites: [{ inviteId: "turtles-invite", devicePublicKey: "far", deviceLabel: "odin2portal", personName: "Simon", session: remote }],
    streamSession: () => remote,
  })
  await waitFor(() => view.model().invites?.length === 1)
  const playingNow = view.ui.getByRole("region", { name: "Playing now" })
  await invoke(() => within(playingNow).getByRole("button", { name: "Play on stream" }).click())
  expect(view.streamPlay).toHaveBeenCalledWith("turtles-invite")
  expect(view.reserve).not.toHaveBeenCalled()
  expect(view.prepare).not.toHaveBeenCalled()
  remote = undefined
  await waitFor(() => view.model().status._tag === "Ended")
  await invoke(() => view.host().dismiss())
  // The capture's separate catalog failures still need their own dismissal.
  if (view.model().status._tag === "Problem") await invoke(() => view.host().dismiss())
  await waitFor(() => {
    const catalog = view.model().catalog
    return catalog._tag === "Ready" && !catalog.games.some(game => game.resumable)
  })
  await view.click(/Teenage Mutant/)
  await view.play()
  await view.expectLocalLaunch()
})

test("captured catalog: repeated Play during a held startup cannot replace its local launch", async () => {
  let release!: () => void
  const reservation = new Promise<void>(resolve => { release = resolve })
  const view = await mount({ _tag: "Detail", gameId: id }, { sessionReserveGate: reservation })
  await view.play()
  await waitFor(() => view.reserve.mock.calls.length === 1)
  expect(view.model().status._tag).toBe("Busy")
  await invoke(() => {
    view.host().launchGame(id, JSON.stringify(["local", null, game.id]))
    view.host().runGameAction(id, "play")
  })
  expect(view.reserve).toHaveBeenCalledTimes(1)
  await invoke(release)
  await view.expectLocalLaunch()
})

test("captured catalog: the download gate retains the local route through confirmation", async () => {
  const artifact = { id: "turtles-test-runtime", title: "SNES runtime", bytesTotal: 10 }
  const view = await mount({ _tag: "Detail", gameId: id }, {
    artifacts: [artifact], gameArtifacts: { [game.id]: [artifact.id] },
    acquisitionBytesPerStep: 10, acquisitionUnpackSteps: 0,
  })
  await view.play()
  await waitFor(() => view.model().status._tag === "Confirm")
  expect(view.reserve).not.toHaveBeenCalled()
  await view.click("Download")
  await view.expectLocalLaunch()
})
