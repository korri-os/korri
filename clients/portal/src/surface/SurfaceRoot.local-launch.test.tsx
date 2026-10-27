import { afterEach, expect, spyOn, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import type { Game, LocalGame } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"
import { runnerRoutes } from "./fixtures/runner-routes"
import { turtlesCatalogGames, turtlesLocalGame } from "./fixtures/turtles-in-time"

// A host korrid lists its own library both as local games and as catalog games
// with source.isLocal (observed on the RG DS, 2026-10-25). Folding shows one
// local-game entry; Play must still launch it.

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
  document.body.innerHTML = ""
})
const invoke = async (fn: () => void) => { await act(async () => fn()) }
const waitFor = async (fn: () => boolean) => {
  for (let i = 0; i < 200; i++) {
    if (fn()) return
    await act(async () => { await new Promise(done => setTimeout(done, 10)) })
  }
  throw new Error("condition did not become true")
}

const identity = { kind: "hash", value: "sha256:" + "4e".repeat(32) } as const
const catalogCopy: Game = {
  id: "tetris-ds-usa", title: "Tetris DS (USA)", host: "rgds", identity,
  supportsRunnerSelection: false,
  source: { label: "rgds", isLocal: true },
}
const localCopy: LocalGame = { id: "tetris-ds-usa", title: "Tetris DS (USA)", system: "Nintendo DS", identity }

test("Play launches a folded local-game entry through its catalog copy on this device", async () => {
  const korrid = createInMemoryKorridClient({
    games: [catalogCopy],
    localGames: [localCopy],
    gameRoutes: [{ ...runnerRoutes, gameId: catalogCopy.id, gameRunner: undefined, systemRunners: {}, routes: runnerRoutes.routes.slice(0, 1) }],
  })
  await korrid.finishOnboarding()
  let model!: SurfaceModel
  let host!: SurfaceHost
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.push(root)
  await act(async () => root.render(<SurfaceRoot bus={createInputBus()} korrid={korrid} surface={{
    id: "acceptance", title: "Acceptance", presentations: ["catalog"],
    render: props => {
      model = props.model
      host = props.host
      return <PicoSurface {...props} />
    },
  }} />))
  await waitFor(() => model.catalog._tag === "Ready")
  const games = model.catalog._tag === "Ready" ? model.catalog.games : []
  // One folded entry, shown as the local copy.
  expect(games.map(item => item.id)).toEqual(["local-game:tetris-ds-usa"])

  await invoke(() => host.launchGame("local-game:tetris-ds-usa"))
  // The in-memory korrid starts the session at once: the game becomes the
  // resumable live session instead of a refusal notice.
  await waitFor(() => model.catalog._tag === "Ready" &&
    model.catalog.games.some(item => item.resumable && item.id.startsWith("now-playing:")))
  expect(container.textContent).not.toContain("cannot start this entry")
})

async function mountTurtles(supportsRunnerSelection = true) {
  const korrid = createInMemoryKorridClient({
    games: turtlesCatalogGames.map(game => ({ ...game, supportsRunnerSelection })),
    localGames: [turtlesLocalGame],
    gameRoutes: [{ ...runnerRoutes, gameId: turtlesLocalGame.id, gameRunner: undefined, systemRunners: {}, routes: runnerRoutes.routes.slice(0, 1) }],
  })
  await korrid.finishOnboarding()
  const reserve = spyOn(korrid, "sessionReserve")
  const prepare = spyOn(korrid, "sessionPrepare")
  let model!: SurfaceModel
  let host!: SurfaceHost
  const id = `local-game:${turtlesLocalGame.id}`
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.push(root)
  await act(async () => root.render(<SurfaceRoot bus={createInputBus()} korrid={korrid} surface={{
    id: "acceptance", title: "Acceptance", presentations: ["catalog"],
    render: props => {
      model = props.model
      host = props.host
      return <PicoSurface {...props} initialView={{ _tag: "Detail", gameId: id }} />
    },
  }} />))
  await waitFor(() => model.catalog._tag === "Ready")
  const games = model.catalog._tag === "Ready" ? model.catalog.games : []
  expect(games.map(game => game.id)).toEqual([id])
  const locations = games[0]!.launchLocations!
  expect(locations.map(location => location.label)).toEqual(["This device", "odin2portal", "rg35xxsp"])
  return {
    korrid, reserve, prepare, container, id, locations,
    host: () => host,
    model: () => model,
    async expectLocalLaunch() {
      // Check the refusal directly so a regression reports the actual notice,
      // rather than only timing out waiting for the session.
      expect(container.textContent).not.toContain("Korri cannot start this entry on this device.")
      await waitFor(() => model.catalog._tag === "Ready" && model.catalog.games.some(game => game.resumable))
      expect(await korrid.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { gameId: turtlesLocalGame.id } } })
      expect(reserve).toHaveBeenCalledWith({ gameId: turtlesLocalGame.id })
      expect(prepare).not.toHaveBeenCalled()
    },
  }
}

for (const supportsRunnerSelection of [true, false]) {
  const path = supportsRunnerSelection ? "selected runner" : "configured command"
  test(`${path}: a multi-location local game launches only after an exact location choice`, async () => {
    const view = await mountTurtles(supportsRunnerSelection)
    await invoke(() => view.host().launchGame(view.id))
    await invoke(() => view.host().launchGame(view.id, "unknown-location"))
    expect(view.reserve).not.toHaveBeenCalled()
    expect(view.prepare).not.toHaveBeenCalled()
    expect(view.model().status._tag).toBe("Browsing")

    await invoke(() => view.host().launchGame(view.id, view.locations[0]!.id))
    await view.expectLocalLaunch()
  })

  test(`${path}: runGameAction resolves the folded local game's launchable catalog copy`, async () => {
    const view = await mountTurtles(supportsRunnerSelection)
    await invoke(() => view.host().runGameAction(view.id, "play"))
    await view.expectLocalLaunch()
  })
}

test("Pico's detail Play and This device choice start the folded Turtles game", async () => {
  const view = await mountTurtles()
  const play = [...view.container.querySelectorAll("button")].find(button => button.textContent?.includes("PLAY"))
  expect(play).toBeDefined()
  await invoke(() => play!.click())
  expect(view.container.textContent).toContain("PLAY WHERE?")
  expect(view.reserve).not.toHaveBeenCalled()
  const local = [...view.container.querySelectorAll("button")].find(button => button.textContent === "This device")
  expect(local).toBeDefined()
  await invoke(() => local!.click())
  await view.expectLocalLaunch()
})

for (const device of ["odin2portal", "rg35xxsp"]) {
  test(`a folded game's explicit ${device} choice still prepares that remote copy`, async () => {
    const view = await mountTurtles()
    const location = view.locations.find(location => location.label === device)!
    await invoke(() => view.host().launchGame(view.id, location.id))
    expect(view.prepare).toHaveBeenCalledWith(turtlesLocalGame.id, device)
    expect(view.reserve).not.toHaveBeenCalled()
    expect(view.container.textContent).not.toContain("cannot start this entry")
  })
}
