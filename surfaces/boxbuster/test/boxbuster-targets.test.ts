/**
 * Targets: the focusable points laid over the store for where you stand.
 *
 * The host moves focus between them by screen position, so each must sit on
 * the thing it stands for, inside the container, and apart from the others.
 * Where focus lands on arrival decides what one confirm does next.
 */
import { describe, expect, test } from "bun:test"
import { spotsFrom } from "../src/boxbuster-spots"
import type { TapeFacts } from "../src/boxbuster-store-view"
import { landingFor, type Target, targetsFor } from "../src/boxbuster-targets"
import type { Visit } from "../src/boxbuster-visit"
import { computeMap, type StoreGame, type StoreShelving } from "../src/map"
import { placeTapes } from "../src/tape-placement"

const games = (prefix: string, count: number): StoreGame[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `${prefix}-${i}`,
    title: `${prefix} ${i}`,
  }))

const SHELVING: StoreShelving = {
  returns: games("return", 2),
  newReleases: games("new", 12),
  staffPicks: games("staff", 20),
  classics: games("classic", 30),
}

const map = computeMap(SHELVING)
const placed = placeTapes(map)
const spots = spotsFrom(map, placed)
const tapes: TapeFacts[] = placed.map(tape => ({
  id: tape.game.id,
  title: tape.game.title,
  aisle: "classics",
  launch:
    tape.game.id === "return-1"
      ? {
          _tag: "Choose",
          locations: [
            { id: "copy-local", label: "This device" },
            { id: "copy-zao", label: "zao" },
          ],
        }
      : { _tag: "Here" },
}))

const SIZES = [
  [640, 480],
  [480, 640],
  [1920, 1080],
] as const

function at(visit: Visit, width = 640, height = 480): Target[] {
  return targetsFor({ spots, visit, tapes, placed, width, height })
}

const empty: Visit = { opened: true, spot: spots.door, hand: { _tag: "Empty" } }

describe("targets", () => {
  test("at the door, the cart's tapes and the way into the lobby", () => {
    const targets = at(empty)
    expect(
      targets.filter(t => t._tag === "Tape").map(t => t.tapeId),
    ).toEqual(["return-0", "return-1"])
    expect(
      targets.flatMap(t => (t._tag === "Exit" ? [t.exit.label] : [])),
    ).toContain("Lobby")
  })

  test("with a tape in hand, shelves offer no tapes, only the ways on", () => {
    const targets = at({
      ...empty,
      hand: { _tag: "Holding", tapeId: "return-0", face: "front", pose: "carrying" },
    })
    expect(targets.some(t => t._tag === "Tape")).toBe(false)
    expect(targets.some(t => t._tag === "Exit")).toBe(true)
  })

  test("in the viewing room, a tape in hand goes in the deck", () => {
    const targets = at({
      ...empty,
      spot: spots.viewing,
      hand: { _tag: "Holding", tapeId: "new-0", face: "front", pose: "carrying" },
    })
    expect(
      targets.flatMap(t => (t._tag === "Deck" ? [t.label] : [])),
    ).toEqual(["Put new 0 in the deck"])
  })

  test("a tape Korri can play in several places gets one deck per place", () => {
    const decks = at({
      ...empty,
      spot: spots.viewing,
      hand: { _tag: "Holding", tapeId: "return-1", face: "front", pose: "carrying" },
    }).filter(t => t._tag === "Deck")
    expect(decks.map(t => [t.label, t.locationId])).toEqual([
      ["Put return 1 in the This device deck", "copy-local"],
      ["Put return 1 in the zao deck", "copy-zao"],
    ])
    expect(decks[0]!.x).toBeLessThan(decks[1]!.x)
  })

  test("after a failed launch, the deck is pressed again to retry", () => {
    const targets = targetsFor({
      spots,
      visit: { ...empty, spot: spots.viewing, deck: "new-0" },
      tapes,
      placed,
      width: 640,
      height: 480,
      retry: true,
    })
    const decks = targets.filter(t => t._tag !== "Exit" && t._tag !== "Tape")
    expect(decks.map(t => t._tag)).toEqual(["Retry"])
    expect(landingFor(targets, { ...empty, spot: spots.viewing })).toBe("retry")
  })

  test("every way on is a mark on the floor", () => {
    for (const spot of spots.byId.values()) {
      for (const exit of spot.exits) expect(exit.anchor.y).toBeLessThan(0.1)
    }
  })

  test("a loaded deck with empty hands can be ejected", () => {
    const targets = at({ ...empty, spot: spots.viewing, deck: "new-0" })
    expect(
      targets.flatMap(t => (t._tag === "Eject" ? [t.label] : [])),
    ).toEqual(["Take new 0 out"])
  })

  test.each(SIZES)(
    "at %ix%i every target is inside the container and apart",
    (width, height) => {
      for (const spot of spots.byId.values()) {
        const targets = at({ ...empty, spot: spot.id }, width, height)
        for (const t of targets) {
          expect(t.x).toBeGreaterThanOrEqual(0)
          expect(t.x).toBeLessThanOrEqual(width)
          expect(t.y).toBeGreaterThanOrEqual(0)
          expect(t.y).toBeLessThanOrEqual(height)
        }
        for (const [i, a] of targets.entries()) {
          for (const b of targets.slice(i + 1)) {
            expect(Math.hypot(a.x - b.x, a.y - b.y)).toBeGreaterThanOrEqual(36)
          }
        }
      }
    },
  )
})

describe("where focus lands", () => {
  test("holding a tape in the viewing room, on the deck", () => {
    const visit: Visit = {
      ...empty,
      spot: spots.viewing,
      hand: { _tag: "Holding", tapeId: "new-0", face: "front", pose: "carrying" },
    }
    const targets = at(visit)
    expect(targets.find(t => t.key === landingFor(targets, visit))?._tag).toBe(
      "Deck",
    )
  })

  test("after a step along a shelf, on the tape nearest the way back", () => {
    const glide = [...spots.byId.values()]
      .flatMap(spot => spot.exits.map(exit => ({ spot, exit })))
      .find(({ exit }) => exit.glide)
    if (glide === undefined) throw new Error("expected a shelf long enough to step along")
    const visit: Visit = { ...empty, spot: glide.exit.to, cameFrom: glide.spot.id }
    const targets = at(visit)
    const back = targets.find(
      t => t._tag === "Exit" && t.exit.to === glide.spot.id,
    )
    const landed = targets.find(t => t.key === landingFor(targets, visit))
    expect(landed?._tag).toBe("Tape")
    const distance = (t: Target) => Math.hypot(t.x - back!.x, t.y - back!.y)
    for (const t of targets.filter(candidate => candidate._tag === "Tape")) {
      expect(distance(landed!)).toBeLessThanOrEqual(distance(t))
    }
  })

  test("arriving where there are no tapes, on the way back", () => {
    const visit: Visit = { ...empty, spot: "hub", cameFrom: spots.door }
    const targets = at(visit)
    const landed = targets.find(t => t.key === landingFor(targets, visit))
    expect(landed?._tag === "Exit" && landed.exit.to).toBe(spots.door)
  })

  test("on first arrival, on the first tape", () => {
    const targets = at(empty)
    expect(landingFor(targets, empty)).toBe("tape:return-0")
  })
})
