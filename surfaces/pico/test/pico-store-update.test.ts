/**
 * The store's navigation rules, driven through `update` with plain values:
 * what each press shows and what Pico asks Korri to do.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel, SurfaceStore } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../src/fixtures/fixture-host"
import { STORE_GETS, storeDetails, storeFixture } from "../src/fixtures/store"
import type { PicoMessage } from "../src/state/messages"
import { initialNavigation, type PicoNavigation, topLayer } from "../src/state/navigation"
import type { PicoRequest } from "../src/state/requests"
import { shownScreen } from "../src/state/shown"
import { update } from "../src/state/update"

const BBS = "@korri:pico8"
const withStore = (store: SurfaceStore | undefined, extra: Partial<SurfaceModel> = {}): SurfaceModel =>
  ({ ...fixtureModel, ...(store === undefined ? {} : { store }), ...extra })

function story(korri: SurfaceModel, ...messages: PicoMessage[]) {
  return storyFrom(initialNavigation(), korri, ...messages)
}
function storyFrom(start: PicoNavigation, korri: SurfaceModel, ...messages: PicoMessage[]) {
  let nav = start
  const requests: PicoRequest[] = []
  for (const message of messages) {
    const step = update(nav, message, korri)
    nav = step.model
    requests.push(...step.requests)
  }
  return { nav, requests, shown: shownScreen(nav, korri) }
}

const open = { _tag: "ChoseMenuStore" } as const
const back = { _tag: "PressedBack" } as const
const type = (character: string) => ({ _tag: "TypedText", character }) as const
const idleSearch = storeFixture({ search: { _tag: "Idle" } })

describe("opening the store", () => {
  test("GET GAMES opens it and asks Korri for its providers", () => {
    const { nav, requests, shown } = story(withStore(storeFixture()), open)
    expect(topLayer(nav)).toBe("Store")
    expect(shown._tag).toBe("Store")
    expect(requests).toEqual([{ _tag: "OpenStore" }])
  })

  test("a host without a store opens nothing", () => {
    const { nav, requests } = story(withStore(undefined), open)
    expect(nav.store).toBe(false)
    expect(requests).toEqual([])
  })

  test("Korri's launch status covers the store, as it covers every screen", () => {
    const korri = withStore(storeFixture(), { status: { _tag: "Busy", kicker: "Starting…", gameId: "tetris" } })
    expect(story(korri, open).shown._tag).toBe("Home")
  })
})

describe("searching", () => {
  test("a provider not searched yet opens on the keyboard; SEARCH sends the trimmed text", () => {
    const korri = withStore(idleSearch)
    const { nav, requests } = story(korri, open, { _tag: "ChoseStoreProvider", providerId: BBS },
      type(" "), type("c"), type("e"), type("l"), type(" "), { _tag: "SubmittedStoreSearch" })
    expect(requests).toEqual([{ _tag: "OpenStore" }, { _tag: "SearchStore", providerId: BBS, query: "cel" }])
    expect(nav.storePlace).toEqual({ view: "Results", providerId: BBS, typing: undefined })
  })

  test("a provider Korri does not list is not a page", () => {
    const { nav } = story(withStore(idleSearch), open, { _tag: "ChoseStoreProvider", providerId: "@korri:nobody" })
    expect(nav.storePlace.view).toBe("Providers")
  })

  test("a provider already searched shows its results, and SEARCH reopens the keys with that query", () => {
    const korri = withStore(storeFixture({ search: { _tag: "Ready", providerId: BBS, query: "celeste", entries: [] } }))
    const { nav } = story(korri, open, { _tag: "ChoseStoreProvider", providerId: BBS })
    expect(nav.storePlace.typing).toBeUndefined()
    const reopened = storyFrom(nav, korri, { _tag: "OpenedStoreSearch" })
    expect(reopened.nav.storePlace.typing?.text).toBe("celeste")
  })

  test("the keys stop at 64 characters", () => {
    const korri = withStore(idleSearch)
    const typed = story(korri, open, { _tag: "ChoseStoreProvider", providerId: BBS }, ...Array.from({ length: 70 }, () => type("a")))
    expect(typed.nav.storePlace.typing?.text).toHaveLength(64)
  })

  test("Back closes the keyboard before anything else, and sends nothing", () => {
    const korri = withStore(idleSearch)
    const { nav, requests } = story(korri, open, { _tag: "ChoseStoreProvider", providerId: BBS }, type("x"), back)
    expect(nav.storePlace).toEqual({ view: "Results", providerId: BBS, typing: undefined })
    expect(requests).toEqual([{ _tag: "OpenStore" }])
  })
})

describe("an entry", () => {
  const korri = withStore(storeFixture())
  const atEntry = () => story(korri, open, { _tag: "ChoseStoreProvider", providerId: BBS }, { _tag: "OpenedStoreEntry", entryId: "e:celeste" })

  test("opening one of Korri's results asks for it; one Korri did not list asks nothing", () => {
    expect(atEntry().requests).toEqual([{ _tag: "OpenStore" }, { _tag: "OpenStoreEntry", entryId: "e:celeste" }])
    const stray = storyFrom(atEntry().nav, korri, back, { _tag: "OpenedStoreEntry", entryId: "e:nowhere" })
    expect(stray.requests).toEqual([])
    expect(stray.nav.storePlace.view).toBe("Results")
  })

  test("only an action Korri publishes now is run; a stale one sends nothing", () => {
    const pressed = storyFrom(atEntry().nav, korri, { _tag: "PressedStoreAction", actionId: "store:1:get" })
    expect(pressed.requests).toEqual([{ _tag: "RunAction", actionId: "store:1:get" }])
    const moved = withStore(storeFixture({ details: storeDetails(STORE_GETS.confirm) }))
    const stale = storyFrom(atEntry().nav, moved, { _tag: "PressedStoreAction", actionId: "store:1:get" })
    expect(stale.requests).toEqual([])
  })

  test("Back walks entry, results, providers, then leaves, and never cancels a download", () => {
    const downloading = withStore(storeFixture({ details: storeDetails(STORE_GETS.downloading) }))
    const walked = storyFrom(atEntry().nav, downloading, back)
    expect(walked.nav.storePlace.view).toBe("Results")
    const out = storyFrom(walked.nav, downloading, back, back)
    expect(out.nav.store).toBe(false)
    expect(out.requests).toEqual([])
  })

  test("PLAY on an entry in the library launches the catalog game Korri named", () => {
    const inLibrary = withStore(storeFixture({ details: storeDetails({ _tag: "InLibrary", gameId: "hollow" }) }))
    const played = storyFrom(atEntry().nav, inLibrary, { _tag: "PressedStorePlay", gameId: "hollow" })
    expect(played.requests).toEqual([{ _tag: "LaunchGame", gameId: "hollow" }])
  })

  test("PLAY on a game with several places opens its own screen to ask", () => {
    const inLibrary = withStore(storeFixture({ details: storeDetails({ _tag: "InLibrary", gameId: "tetris" }) }))
    const played = storyFrom(atEntry().nav, inLibrary, { _tag: "PressedStorePlay", gameId: "tetris" })
    expect(played.requests).toEqual([])
    expect(played.nav.detail?.question._tag).toBe("ChoosingLocation")
    // Back from that question returns to the store, where the person was.
    const backed = storyFrom(played.nav, inLibrary, back, back)
    expect(topLayer(backed.nav)).toBe("Store")
  })

  test("PLAY for a game that is not the one in the library sends nothing", () => {
    const inLibrary = withStore(storeFixture({ details: storeDetails({ _tag: "InLibrary", gameId: "hollow" }) }))
    expect(storyFrom(atEntry().nav, inLibrary, { _tag: "PressedStorePlay", gameId: "celeste" }).requests).toEqual([])
  })

  test("the store keeps its place when it closes and opens again", () => {
    const left = storyFrom(atEntry().nav, korri, back, back, back)
    const reopened = storyFrom(left.nav, korri, open, { _tag: "ChoseStoreProvider", providerId: BBS })
    expect(reopened.nav.storePlace.providerId).toBe(BBS)
  })
})
