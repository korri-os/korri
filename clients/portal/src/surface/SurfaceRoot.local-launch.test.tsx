import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import type { Game, LocalGame } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"
import { runnerRoutes } from "./fixtures/runner-routes"

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
