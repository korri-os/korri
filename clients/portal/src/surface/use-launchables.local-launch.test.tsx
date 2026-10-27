import { afterEach, expect, spyOn, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { CatalogSnapshotOutcome } from "@contracts/generated/korrid"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import type { PortalEntry } from "../launchables/state"
import { miniCatalog, miniLocalGames, miniSession } from "./fixtures/mini-capture-3"
import { runnerRoutes } from "./fixtures/runner-routes"
import { surfaceModelFrom } from "./surface-model"
import { useLaunchables, type Launchables } from "./use-launchables"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
})
const invoke = async (action: () => void) => { await act(async () => action()) }
const waitFor = async (predicate: () => boolean) => {
  for (let attempt = 0; attempt < 150; attempt++) {
    if (predicate()) return
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 10)) })
  }
  throw new Error("condition did not become true")
}

async function mount() {
  if (miniCatalog._tag !== "Ok" || miniLocalGames._tag !== "Ok") throw new Error("expected captured catalogs")
  const localGame = miniLocalGames.payload.games[0]!
  let catalog: CatalogSnapshotOutcome = miniCatalog
  let statusUnavailable = false
  const base = createInMemoryKorridClient({
    games: miniCatalog.payload.games,
    localGames: miniLocalGames.payload.games,
    onboardingNeeded: false,
    gameRoutes: [{ ...runnerRoutes, gameId: localGame.id, gameRunner: undefined, systemRunners: {}, routes: runnerRoutes.routes.slice(0, 1) }],
  })
  const reserve = spyOn(base, "sessionReserve")
  const start = spyOn(base, "sessionStart")
  const prepare = spyOn(base, "sessionPrepare")
  const resume = spyOn(base, "sessionResume")
  const korrid: KorridClient = {
    ...base,
    async catalogSnapshot() { return catalog },
    async localGames() { return miniLocalGames },
    async sessionStatus() {
      if (statusUnavailable) return { _tag: "Err", payload: { code: "BrainUnreachable", message: "status unavailable" } }
      const result = await base.sessionStatus()
      return result._tag === "Ok" && Object.keys(result.payload).length === 0
        ? miniSession : result
    },
  }
  let launchables!: Launchables
  function Probe() { launchables = useLaunchables(korrid); return null }
  const root = createRoot(document.createElement("div"))
  roots.push(root)
  await act(async () => root.render(<Probe />))
  await waitFor(() => launchables.state._tag === "Ready")
  const entry = launchables.state._tag === "Ready" ? launchables.state.entries[0]! : undefined
  if (entry?.kind !== "local-game") throw new Error("expected folded local-game")
  expect(entry.alternatives).toHaveLength(3)
  expect(launchables.state).toMatchObject({ notice: { _tag: "Catalog", message: expect.stringContaining("rg35xxpro") } })
  return {
    base, reserve, start, prepare, resume, entry, localGame,
    bare: { kind: "local-game", game: localGame } satisfies PortalEntry,
    current: () => launchables,
    setCatalog(next: CatalogSnapshotOutcome) { catalog = next },
    setUnavailable() { statusUnavailable = true },
    async expectStarted() {
      expect(surfaceModelFrom(launchables.state).status).not.toMatchObject({ reason: "Korri cannot start this entry on this device." })
      await waitFor(() => launchables.state._tag === "Ready" && launchables.state.entries.some(entry => entry.kind === "now-playing"))
      expect(reserve).toHaveBeenCalledTimes(1)
      expect(reserve).toHaveBeenCalledWith({ gameId: localGame.id })
      expect(start).toHaveBeenCalledTimes(1)
      expect(prepare).not.toHaveBeenCalled()
    },
  }
}

for (const selection of ["folded", "bare"] as const) {
  test(`confirmEntry launches a ${selection} local-game using the current captured catalog`, async () => {
    const view = await mount()
    const selected = selection === "folded" ? view.entry : view.bare
    await invoke(() => {
      view.current().confirmEntry(selected)
      view.current().confirmEntry(selected)
    })
    await view.expectStarted()
  })
}

test("confirmEntry resumes the local session instead of starting its folded game twice", async () => {
  const view = await mount()
  await invoke(() => view.current().confirmEntry(view.bare))
  await view.expectStarted()
  await invoke(() => view.current().confirmEntry(view.bare))
  expect(view.resume).toHaveBeenCalledTimes(1)
  expect(view.reserve).toHaveBeenCalledTimes(1)
})

test("an ended local session can be launched again from a retained inventory selection", async () => {
  const view = await mount()
  await invoke(() => view.current().confirmEntry(view.bare))
  await view.expectStarted()
  const status = await view.base.sessionStatus()
  if (status._tag !== "Ok" || status.payload.active === undefined) throw new Error("expected active session")
  await invoke(() => { void view.base.sessionEnd(status.payload.active!.launchId) })
  await act(async () => { await view.current().refresh() })
  await invoke(() => view.current().confirmEntry(view.bare))
  await waitFor(() => view.start.mock.calls.length === 2)
  expect(view.reserve).toHaveBeenCalledTimes(2)
  expect(view.prepare).not.toHaveBeenCalled()
})

test("stale alternatives cannot restore a local route removed from the current catalog", async () => {
  const view = await mount()
  if (miniCatalog._tag !== "Ok") throw new Error("expected catalog")
  view.setCatalog({ _tag: "Ok", payload: { ...miniCatalog.payload, games: miniCatalog.payload.games.filter(game => !game.source.isLocal) } })
  await act(async () => { await view.current().refresh() })
  await invoke(() => view.current().confirmEntry(view.entry))
  expect(view.reserve).not.toHaveBeenCalled()
  expect(view.prepare).not.toHaveBeenCalled()
  expect(surfaceModelFrom(view.current().state).status).toMatchObject({ reason: "Korri cannot start this entry on this device." })
})

test("unavailable session observation blocks a folded local launch", async () => {
  const view = await mount()
  view.setUnavailable()
  await act(async () => { await view.current().refresh() })
  await invoke(() => view.current().confirmEntry(view.bare))
  expect(view.reserve).not.toHaveBeenCalled()
  expect(view.prepare).not.toHaveBeenCalled()
})
