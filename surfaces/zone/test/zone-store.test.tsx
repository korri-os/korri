/**
 * GET GAMES' model (src/state/store.ts), driven with plain values: what
 * each fact does to where the person is, and what it asks Korri. The page
 * as a whole, from home, is in zone-navigation-acceptance ("get games").
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceStore } from "@contracts/surface/korri-surface"
import type { ZoneStoreView } from "../src/pages/ZoneStore"
import { updateStore, ZONE_STORE_START, type ZoneStoreMessage, type ZoneStoreModel } from "../src/state/store"
import {
  fixtureStore,
  fixtureStoreDetails,
  fixtureStoreFresh,
  fixtureStoreGets,
} from "../src/fixtures/fixture-store"

/** The page declares what it reads; the model must stay assignable to it. */
const asView = (model: ZoneStoreModel): ZoneStoreView => model

const run = (store: SurfaceStore, messages: readonly ZoneStoreMessage[], from: ZoneStoreModel = ZONE_STORE_START) =>
  messages.reduce(
    ({ model, requests }, message) => {
      const step = updateStore(model, message, { store })
      return { model: step.model, requests: [...requests, ...step.requests] }
    },
    { model: from, requests: [] as readonly unknown[] },
  )

const PROVIDER = "provider:@korri:pico8"

describe("GET GAMES' model", () => {
  test("opening the page asks Korri for its providers, on the providers", () => {
    const { model, requests } = run(fixtureStoreFresh, [{ _tag: "OpenedStore" }])
    expect(asView(model).page).toBe("providers")
    expect(model.open).toBe(true)
    expect(requests).toEqual([{ _tag: "OpenStore" }])
  })

  test("a provider not asked yet opens on the keyboard; DONE asks it for the words", () => {
    const opened = run(fixtureStoreFresh, [{ _tag: "OpenedStore" }, { _tag: "PressedRow", rowId: PROVIDER }])
    expect(opened.model.page).toBe("results")
    expect(opened.model.keyboard?.draft).toBe("")
    const typed = run(fixtureStoreFresh, [
      { _tag: "PressedKey", key: { kind: "char", char: "c" } },
      { _tag: "PressedKey", key: { kind: "char", char: "e" } },
      { _tag: "PressedKey", key: { kind: "char", char: "l" } },
      { _tag: "PressedKey", key: { kind: "space" } },
      { _tag: "PressedKey", key: { kind: "done" } },
    ], opened.model)
    expect(typed.requests).toEqual([{ _tag: "SearchStore", providerId: "@korri:pico8", query: "cel" }])
    expect(typed.model.keyboard).toBeUndefined()
    expect(typed.model.focused).toBe("search")
  })

  test("a provider already asked opens on its results; SEARCH raises the keyboard with the words", () => {
    const { model } = run(fixtureStore(), [{ _tag: "OpenedStore" }, { _tag: "PressedRow", rowId: PROVIDER }])
    expect(model.keyboard).toBeUndefined()
    expect(model.focused).toBe("search")
    const searching = run(fixtureStore(), [{ _tag: "PressedRow", rowId: "search" }], model)
    expect(searching.model.keyboard).toMatchObject({ draft: "cel", caret: 3 })
    expect(searching.requests).toEqual([])
  })

  test("an entry Korri listed is opened by its id; one it did not list is not", () => {
    const { model } = run(fixtureStore(), [{ _tag: "OpenedStore" }, { _tag: "PressedRow", rowId: PROVIDER }])
    const opened = run(fixtureStore(), [{ _tag: "PressedRow", rowId: "entry:celeste_classic_2-5" }], model)
    expect(opened.model.page).toBe("entry")
    expect(opened.requests).toEqual([{ _tag: "OpenStoreEntry", entryId: "celeste_classic_2-5" }])
    const unknown = run(fixtureStore(), [{ _tag: "PressedRow", rowId: "entry:not-listed" }], model)
    expect(unknown.requests).toEqual([])
    expect(unknown.model).toBe(model)
  })

  test("only an action Korri publishes now, enabled, is run", () => {
    const store = fixtureStore({ details: fixtureStoreDetails(fixtureStoreGets.offer) })
    expect(run(store, [{ _tag: "PressedRow", rowId: "action:store:1:get" }]).requests)
      .toEqual([{ _tag: "RunAction", actionId: "store:1:get" }])
    // An action from an older answer does nothing.
    expect(run(store, [{ _tag: "PressedAction", actionId: "store:0:get" }]).requests).toEqual([])
    const downloading = fixtureStore({ details: fixtureStoreDetails(fixtureStoreGets.downloading) })
    expect(run(downloading, [{ _tag: "PressedAction", actionId: "store:5:cancel" }]).requests)
      .toEqual([{ _tag: "RunAction", actionId: "store:5:cancel" }])
  })

  test("PLAY on an entry in the library launches that game and closes the page", () => {
    const store = fixtureStore({ details: fixtureStoreDetails(fixtureStoreGets.inLibrary) })
    const { model } = run(store, [{ _tag: "OpenedStore" }, { _tag: "PressedRow", rowId: PROVIDER }, { _tag: "PressedRow", rowId: "entry:celeste_classic_2-5" }])
    const played = run(store, [{ _tag: "PressedRow", rowId: "play:celeste" }], model)
    expect(played.requests).toEqual([{ _tag: "LaunchGame", gameId: "celeste" }])
    expect(played.model.open).toBe(false)
    // A game id Korri does not name for this entry launches nothing.
    expect(run(store, [{ _tag: "PressedRow", rowId: "play:hollow" }], model).requests).toEqual([])
  })

  test("Back walks the keyboard, the entry, the results and the providers, and asks Korri nothing", () => {
    const store = fixtureStore({ details: fixtureStoreDetails(fixtureStoreGets.downloading) })
    const deep = run(store, [
      { _tag: "OpenedStore" },
      { _tag: "PressedRow", rowId: PROVIDER },
      { _tag: "PressedRow", rowId: "entry:celeste_classic_2-5" },
    ]).model
    const back = (model: ZoneStoreModel) => {
      const step = updateStore(model, { _tag: "PressedBack" }, { store })
      expect(step.requests).toEqual([])
      return step.model
    }
    const results = back(deep)
    expect(results.page).toBe("results")
    // The cursor goes back to the entry it came from.
    expect(results.focused).toBe("entry:celeste_classic_2-5")
    const keys = run(store, [{ _tag: "PressedRow", rowId: "search" }], results).model
    expect(back(keys)).toMatchObject({ page: "results", keyboard: undefined, focused: "search" })
    const providers = back(results)
    expect(providers).toMatchObject({ page: "providers", focused: PROVIDER })
    expect(back(providers).open).toBe(false)
  })

  test("a host that cannot get games has no page", () => {
    const open = run(fixtureStore(), [{ _tag: "OpenedStore" }]).model
    const step = updateStore(open, { _tag: "KorriPublished" }, { store: undefined })
    expect(step.model).toEqual(ZONE_STORE_START)
    expect(updateStore(ZONE_STORE_START, { _tag: "OpenedStore" }, {}).requests).toEqual([])
  })
})
