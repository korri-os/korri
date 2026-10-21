/**
 * Getting a game from a provider, end to end: the in-memory brain offers the
 * PICO-8 BBS, the one provider that searches, Pico renders what the host publishes, and the
 * host carries each answer back as RPC, then into the catalog and the shared
 * launch. Also: a korrid restart under a download, and the store with an
 * empty library.
 */
import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import type { Game } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient } from "../korrid/client"
import { createInMemoryStoreAcquisition, SAMPLE_STORE } from "../korrid/store-acquisition-client"
import { SurfaceRoot } from "./SurfaceRoot"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
  document.body.innerHTML = ""
})
const waitFor = async (what: string, fn: () => boolean, tries = 1500) => {
  for (let i = 0; i < tries; i++) {
    if (fn()) return
    await act(async () => { await new Promise(done => setTimeout(done, 10)) })
  }
  throw new Error(`never saw ${what}`)
}
const click = async (element: Element | null | undefined) => {
  if (!(element instanceof HTMLElement)) throw new Error("nothing to click")
  await act(async () => element.click())
}
const button = (container: HTMLElement, name: string | RegExp) =>
  [...container.querySelectorAll("button")].find(candidate => {
    const label = candidate.getAttribute("aria-label") ?? candidate.textContent ?? ""
    return typeof name === "string" ? label.trim() === name : name.test(label)
  })

const launched: string[] = []

async function mount(games: readonly Game[] = []) {
  launched.length = 0
  // Small steps, so a download is still running when the test restarts korrid.
  const store = createInMemoryStoreAcquisition({ ...SAMPLE_STORE, bytesPerStep: 4_000 })
  const korrid = createInMemoryKorridClient({ games, store })
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
      // The host as Pico sees it, with each launch it asks for recorded.
      const traced: SurfaceHost = { ...props.host, launchGame: (id, at) => { launched.push(id); props.host.launchGame(id, at) } }
      return <PicoSurface model={props.model} host={traced} />
    },
  }} />))
  await waitFor("the library read", () => model !== undefined && model.catalog._tag !== "Loading")
  if (games.length === 0 && model.catalog._tag !== "Empty") throw new Error(`catalog is ${JSON.stringify(model.catalog)} status ${JSON.stringify(model.status)}`)
  const get = () => {
    const details = model.store?.details
    if (details?._tag !== "Ready") throw new Error(`details are ${details?._tag}`)
    return details.get
  }
  return { store, korrid, container, model: () => model, host: () => host, get }
}

test("from an empty library: GET GAMES, a provider, a search, an entry, Get, Download, then Play", async () => {
  const view = await mount()
  await click(button(view.container, /MENU/))
  await click(button(view.container, "GET GAMES"))
  await waitFor("providers", () => view.model().store?.providers._tag === "Ready")
  await click(button(view.container, /PICO-8 BBS/))
  // Not searched yet: the keyboard opens.
  for (const key of ["c", "e", "l"]) await click(button(view.container, `Type ${key}`))
  await click(button(view.container, "SEARCH"))
  // The BBS does not answer the first time; Retry asks again.
  await waitFor("no answer", () => view.model().store?.search._tag === "Problem")
  await click(button(view.container, "Retry"))
  await waitFor("results", () => view.model().store?.search._tag === "Ready")
  await click(button(view.container, "Celeste Classic 2, pico8"))
  await waitFor("details", () => view.model().store?.details._tag === "Ready")
  await click(button(view.container, "GET"))
  // The BBS states no size: Confirm says so.
  await waitFor("confirm", () => view.get()._tag === "Confirm")
  expect(view.get()).toMatchObject({ detail: "Celeste Classic 2, size unknown" })
  await click(button(view.container, "Download"))
  await waitFor("downloading", () => view.get()._tag === "Working" && "progress" in view.get())
  // The download takes the launch stage, with the size korrid now reports.
  await waitFor("a real bar", () => view.container.querySelector('[role="progressbar"][aria-valuenow]') !== null)
  await waitFor("in the library", () => view.get()._tag === "InLibrary" && (view.get() as { gameId?: string }).gameId !== undefined, 3000)
  expect(view.model().catalog).toMatchObject({ _tag: "Ready", games: [{ title: "Celeste Classic 2" }] })
  const gameId = (view.get() as { gameId: string }).gameId
  const catalog = view.model().catalog
  expect(catalog._tag === "Ready" && catalog.games.some(game => game.id === gameId)).toBe(true)
  await click(button(view.container, "▶ PLAY"))
  // PLAY goes through the catalog's own launch: korrid starts that game.
  expect(launched).toEqual([gameId])
  await waitFor("the session", () => view.model().catalog._tag === "Ready" &&
    (view.model().catalog as { games: readonly { id: string }[] }).games.some((game: { id: string }) => game.id.startsWith("now-playing:")))
}, 30_000)

test("korrid restarting under a download: Download stopped with Retry, and Retry starts again", async () => {
  const view = await mount()
  view.host().openStore()
  await waitFor("providers", () => view.model().store?.providers._tag === "Ready")
  view.host().searchStore("@korri:pico8", "celeste")
  await waitFor("no answer", () => view.model().store?.search._tag === "Problem")
  const problem = view.model().store?.search
  if (problem?._tag !== "Problem") throw new Error("expected no answer")
  view.host().runAction(problem.actions[0]!.id)
  await waitFor("results", () => view.model().store?.search._tag === "Ready")
  const entry = view.model().store?.search
  if (entry?._tag !== "Ready") throw new Error("no results")
  view.host().openStoreEntry(entry.entries[0]!.id)
  await waitFor("details", () => view.model().store?.details._tag === "Ready")
  const run = (label: string) => {
    const get = view.get()
    if (!("actions" in get)) throw new Error(`no actions on ${get._tag}`)
    view.host().runAction(get.actions.find(candidate => candidate.label === label)!.id)
  }
  run("Get")
  await waitFor("confirm", () => view.get()._tag === "Confirm")
  run("Download")
  await waitFor("downloading", () => view.get()._tag === "Working" && (view.get() as { progress?: { done: number } }).progress !== undefined)
  view.store.restart()
  await waitFor("stopped", () => view.get()._tag === "Problem")
  expect(view.get()).toMatchObject({ kicker: "Download stopped" })
  run("Retry")
  await waitFor("asked again", () => view.get()._tag !== "Problem")
}, 30_000)

test("a cart the library already holds offers PLAY at once, with no Get and no download", async () => {
  // A fresh portal over a korrid whose library holds the cart: the game korrid
  // wrote carries the provider link as its identity (ADR 0016).
  const owned: Game = {
    id: "celeste-cart", title: "Celeste Classic 2", supportsRunnerSelection: false,
    source: { label: "This device", isLocal: true },
    identity: { kind: "provider", value: { provider: "@korri:pico8", ref: "celeste_classic_2-5" } },
  }
  const view = await mount([owned])
  view.host().openStore()
  await waitFor("providers", () => view.model().store?.providers._tag === "Ready")
  view.host().searchStore("@korri:pico8", "celeste")
  await waitFor("no answer", () => view.model().store?.search._tag === "Problem")
  const problem = view.model().store?.search
  if (problem?._tag !== "Problem") throw new Error("expected no answer")
  view.host().runAction(problem.actions[0]!.id)
  await waitFor("results", () => view.model().store?.search._tag === "Ready")
  const results = view.model().store?.search
  if (results?._tag !== "Ready") throw new Error("no results")
  view.host().openStoreEntry(results.entries[0]!.id)
  await waitFor("details", () => view.model().store?.details._tag === "Ready")
  expect(view.get()).toEqual({ _tag: "InLibrary", detail: "In your library", gameId: "game:celeste-cart" })
  expect(view.store.downloadedBytes()).toBe(0)
}, 30_000)
