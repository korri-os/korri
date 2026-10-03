/**
 * A visit to the store: where you stand, what you hold, what is in the deck.
 *
 * You play a game by carrying its tape to the viewing room and putting it in
 * the deck. That is the only way to start one. The fast path keeps to the
 * same rule: a tape you can resume is already in your hand in the viewing
 * room, so one confirm puts it in.
 */
import { describe, expect, test } from "bun:test"
import type { StoreSpots } from "../src/boxbuster-spots"
import type { TapeFacts } from "../src/boxbuster-store-view"
import {
  closedVisit,
  step,
  type Visit,
  visitAfterStore,
} from "../src/boxbuster-visit"

function tape(id: string, aisle: TapeFacts["aisle"]): TapeFacts {
  return { id, title: id, aisle, launch: { _tag: "Here" } }
}

const SPOTS: StoreSpots = {
  byId: new Map(
    ["door", "hub", "viewing", "shelf:1"].map(id => [
      id,
      { id, title: id, eye: { x: 0, y: 0, z: 0 }, yaw: 0, pitch: 0, tapeIds: [], exits: [] },
    ]),
  ),
  door: "door",
  viewing: "viewing",
}

const open = (overrides: Partial<Visit> = {}): Visit => ({
  opened: true,
  spot: "shelf:1",
  hand: { _tag: "Empty" },
  ...overrides,
})

describe("opening the store", () => {
  test("waits at the door until there are tapes", () => {
    const closed = closedVisit(SPOTS)
    expect(visitAfterStore(closed, [], SPOTS)).toBe(closed)
  })

  test("with a tape you can resume, opens in the viewing room holding it", () => {
    expect(
      visitAfterStore(
        closedVisit(SPOTS),
        [tape("resume", "returns"), tape("new", "newReleases")],
        SPOTS,
      ),
    ).toEqual({
      opened: true,
      spot: "viewing",
      hand: { _tag: "Holding", tapeId: "resume", face: "front", pose: "carrying" },
    })
  })

  test("with nothing to resume, opens at the door with empty hands", () => {
    expect(
      visitAfterStore(closedVisit(SPOTS), [tape("new", "newReleases")], SPOTS),
    ).toEqual({ opened: true, spot: "door", hand: { _tag: "Empty" } })
  })
})

describe("when Korri republishes the catalog", () => {
  const tapes = [tape("a", "newReleases"), tape("b", "classics")]

  test("an unchanged visit is the same value", () => {
    const visit = open({
      hand: { _tag: "Holding", tapeId: "a", face: "front", pose: "carrying" },
      deck: "b",
    })
    expect(visitAfterStore(visit, tapes, SPOTS)).toBe(visit)
  })

  test("a tape that left the store leaves your hand and the deck", () => {
    expect(
      visitAfterStore(
        open({
          hand: { _tag: "Holding", tapeId: "gone", face: "back", pose: "carrying" },
          deck: "also-gone",
        }),
        tapes,
        SPOTS,
      ),
    ).toEqual(open())
  })

  test("a spot the rebuilt store no longer has sends you to the door", () => {
    expect(visitAfterStore(open({ spot: "shelf:9" }), tapes, SPOTS)).toEqual(
      open({ spot: "door" }),
    )
  })
})

describe("walking", () => {
  test("moves you and remembers where you came from", () => {
    expect(step(open(), { _tag: "Walk", to: "hub" }, SPOTS)).toEqual({
      visit: open({ spot: "hub", cameFrom: "shelf:1" }),
    })
  })

  test("lowers a tape you were reading to carry it", () => {
    const reading = open({
      hand: { _tag: "Holding", tapeId: "a", face: "back", pose: "reading" },
    })
    expect(step(reading, { _tag: "Walk", to: "hub" }, SPOTS).visit.hand).toEqual(
      { _tag: "Holding", tapeId: "a", face: "back", pose: "carrying" },
    )
  })

  test("a problem brings you to the TV", () => {
    expect(step(open(), { _tag: "ProblemShown" }, SPOTS).visit.spot).toBe(
      "viewing",
    )
  })
})

describe("a tape in hand", () => {
  test("confirm on a tape picks it up and brings it close, cover toward you", () => {
    expect(step(open(), { _tag: "PickUp", tapeId: "a" }, SPOTS)).toEqual({
      visit: open({
        hand: { _tag: "Holding", tapeId: "a", face: "front", pose: "reading" },
      }),
    })
  })

  test("Options brings it close and turns it over, and back again", () => {
    const held = open({
      hand: { _tag: "Holding", tapeId: "a", face: "front", pose: "carrying" },
    })
    const turned = step(held, { _tag: "Turn" }, SPOTS).visit
    expect(turned.hand).toEqual({
      _tag: "Holding",
      tapeId: "a",
      face: "back",
      pose: "reading",
    })
    expect(step(turned, { _tag: "Turn" }, SPOTS).visit.hand).toEqual({
      _tag: "Holding",
      tapeId: "a",
      face: "front",
      pose: "reading",
    })
  })

  test("Back puts it back on its shelf", () => {
    expect(
      step(
        open({ hand: { _tag: "Holding", tapeId: "a", face: "back", pose: "carrying" } }),
        { _tag: "Back", problem: false },
        SPOTS,
      ),
    ).toEqual({ visit: open() })
  })

  test("Back with empty hands does nothing", () => {
    const visit = open()
    expect(step(visit, { _tag: "Back", problem: false }, SPOTS)).toEqual({
      visit,
    })
  })

  test("Back answers a problem before it puts anything down", () => {
    const visit = open({
      hand: { _tag: "Holding", tapeId: "a", face: "front", pose: "carrying" },
    })
    expect(step(visit, { _tag: "Back", problem: true }, SPOTS)).toEqual({
      visit,
      command: { _tag: "Dismiss" },
    })
  })

  test("Back on a problem ejects the tape that failed into your hand", () => {
    expect(
      step(open({ spot: "viewing", deck: "a" }), { _tag: "Back", problem: true }, SPOTS),
    ).toEqual({
      visit: open({
        spot: "viewing",
        hand: { _tag: "Holding", tapeId: "a", face: "front", pose: "reading" },
      }),
      command: { _tag: "Dismiss" },
    })
  })
})

describe("the deck", () => {
  const holding = open({
    spot: "viewing",
    hand: { _tag: "Holding", tapeId: "a", face: "front", pose: "carrying" },
  })

  test("putting a tape in starts the game", () => {
    expect(step(holding, { _tag: "Insert" }, SPOTS)).toEqual({
      visit: open({ spot: "viewing", deck: "a" }),
      command: { _tag: "Launch", tapeId: "a" },
    })
  })

  test("the deck you choose is where it plays", () => {
    expect(
      step(holding, { _tag: "Insert", locationId: "copy-zao" }, SPOTS).command,
    ).toEqual({ _tag: "Launch", tapeId: "a", locationId: "copy-zao" })
  })

  test("a new tape replaces the one in the deck", () => {
    expect(
      step({ ...holding, deck: "b" }, { _tag: "Insert" }, SPOTS).visit.deck,
    ).toBe("a")
  })

  test("nothing goes in with empty hands", () => {
    const visit = open({ spot: "viewing" })
    expect(step(visit, { _tag: "Insert" }, SPOTS)).toEqual({ visit })
  })

  test("ejecting puts the tape back in your hand, close enough to read", () => {
    expect(
      step(open({ spot: "viewing", deck: "a" }), { _tag: "Eject" }, SPOTS),
    ).toEqual({
      visit: open({
        spot: "viewing",
        hand: { _tag: "Holding", tapeId: "a", face: "front", pose: "reading" },
      }),
    })
  })
})
