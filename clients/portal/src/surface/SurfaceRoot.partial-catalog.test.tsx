import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { within } from "@testing-library/react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { PeerListState, type Game } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
  document.body.innerHTML = ""
})
const waitFor = async (predicate: () => boolean) => {
  for (let attempt = 0; attempt < 250; attempt += 1) {
    if (predicate()) return
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 10)) })
  }
  throw new Error("condition did not become true")
}

test("Pico renders a partial peer library without OK and adds a recovered peer's games", async () => {
  const game: Game = {
    id: "wl4", title: "Wario Land 4", host: "rpminiv2",
    supportsRunnerSelection: false, source: { label: "rpminiv2", isLocal: false },
  }
  const recoveredGame: Game = {
    ...game, id: "recovered", title: "Recovered peer's game", host: "rg35xxsp",
    source: { label: "rg35xxsp", isLocal: false },
  }
  let recovered = false
  const client: KorridClient = {
    ...createInMemoryKorridClient({ games: [], onboardingNeeded: false }),
    async peerList() { return { _tag: "Ok", payload: { peers: [
      { devicePublicKey: "aa".repeat(32), label: "rg35xxsp", state: recovered ? PeerListState.Ready : PeerListState.Loading, updatedAt: 1 },
    ] } } },
    async catalogSnapshot() { return { _tag: "Ok", payload: {
      games: recovered ? [game, recoveredGame] : [game],
      failures: recovered ? [] : [{ host: "rg35xxsp", code: "UpstreamUnreachable", message: "upstream unreachable: peer connect failed" }],
    } } },
  }
  let model!: SurfaceModel
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.push(root)
  await act(async () => root.render(<SurfaceRoot bus={createInputBus()} korrid={client} surface={{
    id: "partial-catalog", title: "Partial catalog", presentations: ["catalog"],
    render: props => {
      model = props.model
      return <PicoSurface {...props} initialView={{ _tag: "Find" }} />
    },
  }} />))
  await waitFor(() => model.catalog._tag === "Ready")
  const ui = within(container)
  expect(model.status).toEqual({ _tag: "Browsing" })
  expect(ui.getByRole("button", { name: /Wario Land 4/ })).toBeTruthy()
  expect(ui.queryByRole("button", { name: "OK" })).toBeNull()
  expect(container.textContent).not.toContain("Catalog problem")

  recovered = true
  await waitFor(() => model.catalog._tag === "Ready" && model.catalog.games.length === 2)
  expect(model.status).toEqual({ _tag: "Browsing" })
  expect(ui.getByRole("button", { name: /Wario Land 4/ })).toBeTruthy()
  expect(ui.getByRole("button", { name: /Recovered peer's game/ })).toBeTruthy()
  expect(ui.queryByRole("button", { name: "OK" })).toBeNull()
})
