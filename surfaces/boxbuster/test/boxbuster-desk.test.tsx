/**
 * The order desk: getting a game from a provider in Boxbuster. Both ways in
 * (the empty store's board and the door's way to the desk in a stocked
 * store), every page, and the tape the desk hands you for the TV. Driven the
 * way the portal drives a surface: focus and clicks, Back through the host.
 */
import { afterEach, beforeEach, describe, expect, test } from "bun:test"
import type {
  SurfaceGame,
  SurfaceModel,
  SurfaceStore,
  SurfaceStoreGet,
} from "@contracts/surface/korri-surface"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { BoxbusterSurface } from "../src/BoxbusterSurface"
import type { BoxbusterStoreDrawing, BoxbusterStoreProps } from "../src/BoxbusterStore"
import { DESK_START, DESK_TEXT_MAX, deskBack, deskType } from "../src/boxbuster-desk"
import { DOOR_SPOT, ORDER_DESK_SPOT, spotsFrom } from "../src/boxbuster-spots"
import { computeMap } from "../src/map"
import { placeTapes } from "../src/tape-placement"
import { createRecordingHost } from "./recording-host"

function recordingDrawing(available = true) {
  const drawn: { last?: BoxbusterStoreProps } = {}
  const drawing: BoxbusterStoreDrawing = {
    available: () => available,
    Draw: props => {
      drawn.last = props
      return null
    },
  }
  return { drawing, drawn }
}

const realRect = HTMLElement.prototype.getBoundingClientRect
beforeEach(() => {
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    return this.hasAttribute("data-boxbuster-surface") ? new DOMRect(0, 0, 640, 480) : realRect.call(this)
  }
})
afterEach(() => {
  HTMLElement.prototype.getBoundingClientRect = realRect
  cleanup()
})

const HUB = "@korri:homebrewhub"
const action = (id: string, label: string, enabled = true) => ({ id, label, enabled })
const entry2048 = { id: "e:2048", title: "2048", system: "Game Boy" }

function storeWith(overrides: Partial<SurfaceStore> = {}): SurfaceStore {
  return {
    providers: { _tag: "Ready", providers: [
      { id: "@korri:pico8", title: "PICO-8 BBS" },
      { id: HUB, title: "Homebrew Hub" },
      { id: "@korri:retrobrews", title: "RetroBrews" },
    ] },
    search: { _tag: "Idle" },
    details: { _tag: "Closed" },
    ...overrides,
  }
}
const ready = (get: SurfaceStoreGet): SurfaceStore["details"] =>
  ({ _tag: "Ready", entry: entry2048, description: "Slide the tiles and join equal numbers to reach 2048.", get })

function modelWith(games: readonly SurfaceGame[], store: SurfaceStore | undefined): SurfaceModel {
  return {
    presentation: { kind: "catalog" },
    catalog: games.length === 0 ? { _tag: "Empty" } : { _tag: "Ready", games },
    status: { _tag: "Browsing" },
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
    ...(store === undefined ? {} : { store }),
  }
}

const kirby: SurfaceGame = { id: "kirby", title: "Kirby" }
const spotOf = (container: HTMLElement) =>
  container.querySelector("[data-boxbuster-surface]")?.getAttribute("data-spot")
const go = (label: string | RegExp) => fireEvent.click(screen.getByRole("button", { name: label }))

describe("the ways to the desk", () => {
  test("the empty store's board points to the desk only where Korri publishes a store", () => {
    const host = createRecordingHost()
    const { drawing } = recordingDrawing()
    const { rerender } = render(<BoxbusterSurface host={host} model={modelWith([], undefined)} drawing={drawing} />)
    expect(screen.queryByRole("button", { name: "Go to the order desk" })).toBeNull()
    rerender(<BoxbusterSurface host={host} model={modelWith([], storeWith())} drawing={drawing} />)
    const desk = screen.getByRole("button", { name: "Go to the order desk" })
    expect(document.activeElement === desk).toBe(true)
    fireEvent.click(desk)
    expect(host.calls).toEqual(["openStore"])
    expect(screen.getByRole("region", { name: "Order desk" })).toBeTruthy()
    expect(document.activeElement?.textContent).toBe("PICO-8 BBS")
  })

  test("without 3D graphics the board still points to the desk", () => {
    const host = createRecordingHost()
    const { drawing } = recordingDrawing(false)
    render(<BoxbusterSurface host={host} model={modelWith([kirby], storeWith())} drawing={drawing} />)
    go("Go to the order desk")
    expect(screen.getByRole("region", { name: "Order desk" })).toBeTruthy()
  })

  test("in a stocked store the door has a worn patch to the desk, and Back returns to the door", () => {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    const { container, rerender } = render(<BoxbusterSurface host={host} model={modelWith([kirby], undefined)} drawing={drawing} />)
    expect(spotOf(container)).toBe(DOOR_SPOT)
    expect(screen.queryByRole("button", { name: "Order desk" })).toBeNull()
    rerender(<BoxbusterSurface host={host} model={modelWith([kirby], storeWith())} drawing={drawing} />)
    // The way on is drawn in the room, like every exit: a mark the scene lights.
    expect(drawn.last?.targets.some(target => target._tag === "Exit" && target.exit.to === ORDER_DESK_SPOT)).toBe(true)
    go("Order desk")
    expect(spotOf(container)).toBe(ORDER_DESK_SPOT)
    expect(host.calls).toEqual(["openStore"])
    act(() => host.press("back"))
    expect(spotOf(container)).toBe(DOOR_SPOT)
    expect(host.calls).toEqual(["openStore"])
  })

  test("the spots carry the desk only when asked", () => {
    const map = computeMap({ returns: [], newReleases: [], staffPicks: [], classics: [{ id: "k", title: "Kirby" }] })
    const placed = placeTapes(map)
    const plain = spotsFrom(map, placed).byId.get(DOOR_SPOT)
    const withDesk = spotsFrom(map, placed, { orderDesk: true }).byId.get(DOOR_SPOT)
    expect(plain?.exits.some(exit => exit.to === ORDER_DESK_SPOT)).toBe(false)
    expect(withDesk?.exits.filter(exit => exit.to === ORDER_DESK_SPOT).map(exit => exit.label)).toEqual(["Order desk"])
  })
})

describe("ordering", () => {
  function atDesk(store: SurfaceStore, games: readonly SurfaceGame[] = []) {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    const view = render(<BoxbusterSurface host={host} model={modelWith(games, store)} drawing={drawing} />)
    go("Go to the order desk")
    return { host, drawn, drawing, ...view }
  }

  test("a catalogue not searched opens the order form; the keys write it; Search asks that one", () => {
    const { host } = atDesk(storeWith())
    go("Homebrew Hub")
    expect(screen.getByRole("form", { name: "Order from Homebrew Hub" })).toBeTruthy()
    for (const key of ["2", "0", "4", "8"]) go(`Write ${key}`)
    go("Write a space")
    go("Rub out")
    go("Search")
    expect(host.calls.at(-1)).toBe(`searchStore:${HUB}:2048`)
  })

  test("the desk's pages: searching, no answer with Retry, nothing found, and results", () => {
    const host = createRecordingHost()
    const { drawing } = recordingDrawing()
    const searched = (search: SurfaceStore["search"]) => modelWith([], storeWith({ search }))
    const { rerender } = render(<BoxbusterSurface host={host} model={searched({ _tag: "Searching", providerId: HUB, query: "2048" })} drawing={drawing} />)
    go("Go to the order desk")
    go("Homebrew Hub")
    expect(screen.getByText("Asking Homebrew Hub for “2048”…")).toBeTruthy()
    rerender(<BoxbusterSurface host={host} model={searched({
      _tag: "Problem", providerId: HUB, query: "2048", message: "The provider did not answer. Check the connection and try again.",
      actions: [action("store:2:search-retry", "Retry")],
    })} drawing={drawing} />)
    go("Retry")
    expect(host.calls.at(-1)).toBe("action:store:2:search-retry")
    rerender(<BoxbusterSurface host={host} model={searched({ _tag: "Ready", providerId: HUB, query: "zelda", entries: [] })} drawing={drawing} />)
    expect(screen.getByText("Nothing in Homebrew Hub matches “zelda”.")).toBeTruthy()
    rerender(<BoxbusterSurface host={host} model={searched({ _tag: "Ready", providerId: HUB, query: "2048", entries: [entry2048] })} drawing={drawing} />)
    go(/^2048/)
    expect(host.calls.at(-1)).toBe("openStoreEntry:e:2048")
  })

  test.each([
    ["offer", { _tag: "Offer", actions: [action("store:1:get", "Get")] }, ["Get"], undefined],
    ["confirm", { _tag: "Confirm", kicker: "Download to this device", detail: "2048, size unknown", actions: [action("store:3:download", "Download"), action("store:3:cancel", "Cancel")] }, ["Download", "Cancel"], "2048, size unknown"],
    ["folders", { _tag: "Confirm", kicker: "Choose a folder", actions: [action("store:4:source.0", "Game Boy"), action("store:4:source.1", "SD card"), action("store:4:cancel", "Cancel")] }, ["Game Boy", "SD card", "Cancel"], "Choose a folder"],
    ["unavailable", { _tag: "Unavailable", reason: "No folder on this device holds Game Boy games." }, [], "No folder on this device holds Game Boy games."],
    ["staged", { _tag: "Staged", message: "Downloaded to this device, but your library did not recognise it as a game." }, [], "Downloaded to this device, but your library did not recognise it as a game."],
    ["stopped", { _tag: "Problem", kicker: "Download stopped", reason: "Korri restarted before the download finished.", actions: [action("store:8:retry", "Retry")] }, ["Retry"], "Download stopped"],
  ] as const)("the entry when %s shows Korri's words and runs exactly its actions", (_name, get, labels, words) => {
    const { host } = atDesk(storeWith({ search: { _tag: "Ready", providerId: HUB, query: "", entries: [entry2048] }, details: ready(get) }))
    go("Homebrew Hub")
    go(/^2048/)
    const page = document.querySelector(".boxbuster-desk-entry")
    expect([...(page?.querySelectorAll("button") ?? [])].map(button => button.textContent)).toEqual([...labels])
    if (words !== undefined) expect(screen.getByText(words)).toBeTruthy()
    const first = page?.querySelector("button")
    if (first) {
      fireEvent.click(first)
      expect(host.calls.at(-1)?.startsWith("action:store:")).toBe(true)
    }
  })

  test("a download is a counter from Korri's bytes with Cancel; Back leaves it running", () => {
    const working: SurfaceStoreGet = {
      _tag: "Working", kicker: "Downloading 2048", detail: "16 KB of 32.8 KB · 16 KB/s · 0:01 left",
      progress: { done: 16_000, total: 32_768 }, actions: [action("store:5:cancel", "Cancel")],
    }
    const { host } = atDesk(storeWith({ search: { _tag: "Ready", providerId: HUB, query: "", entries: [entry2048] }, details: ready(working) }))
    go("Homebrew Hub")
    go(/^2048/)
    const counter = screen.getByRole("progressbar", { name: "Downloading 2048" })
    expect(counter.getAttribute("aria-valuenow")).toBe("48")
    expect(counter.querySelectorAll('[data-lit="on"]')).toHaveLength(7)
    go("Cancel")
    expect(host.calls.at(-1)).toBe("action:store:5:cancel")
    const before = host.calls.length
    act(() => host.press("back"))
    expect(screen.getByRole("button", { name: /^2048/ })).toBeTruthy()
    expect(host.calls.length).toBe(before)
  })

  test("in the library: the desk hands you the tape at the TV, once it is on a shelf", () => {
    const store = storeWith({
      search: { _tag: "Ready", providerId: HUB, query: "", entries: [entry2048] },
      details: ready({ _tag: "InLibrary", detail: "In your library", gameId: "store-2048" }),
    })
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    const { container, rerender } = render(<BoxbusterSurface host={host} model={modelWith([], store)} drawing={drawing} />)
    go("Go to the order desk")
    go("Homebrew Hub")
    go(/^2048/)
    // The catalog has not shelved it yet.
    expect(screen.getByText("It is on its way to the shelves.")).toBeTruthy()
    rerender(<BoxbusterSurface host={host} model={modelWith([{ id: "store-2048", title: "2048" }], store)} drawing={drawing} />)
    go("Take it to the TV")
    expect(spotOf(container)).toBe("viewing")
    expect(drawn.last?.held?.tapeId).toBe("store-2048")
    // One confirm on the deck puts it in: the only launchGame call.
    go("Put 2048 in the deck")
    expect(host.calls.at(-1)).toBe("launch:store-2048:")
  })
})

describe("the desk's rules", () => {
  test("Back walks form, entry, results, providers, then leaves", () => {
    expect(deskBack({ view: "Form", providerId: HUB, text: "x" }, false)).toEqual({ view: "Providers", providerId: HUB, text: "x" })
    expect(deskBack({ view: "Form", providerId: HUB, text: "x" }, true)).toEqual({ view: "Results", providerId: HUB, text: "x" })
    expect(deskBack({ view: "Entry", providerId: HUB, text: "" }, true)).toMatchObject({ view: "Results" })
    expect(deskBack({ view: "Results", providerId: HUB, text: "" }, true)).toMatchObject({ view: "Providers" })
    expect(deskBack(DESK_START, false)).toBe("Leave")
  })

  test("the form stops at its limit", () => {
    let place = DESK_START
    for (let i = 0; i < DESK_TEXT_MAX + 5; i++) place = deskType(place, "a")
    expect(place.text).toHaveLength(DESK_TEXT_MAX)
  })
})
