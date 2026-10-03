/**
 * The hand: which tape you are looking at, or holding.
 *
 * The fast path is a contract: when something can be resumed, Boxbuster opens
 * with that tape already in your hand, so one confirm resumes it. Back puts a
 * tape down before it does anything else, except clear a problem.
 */
import { describe, expect, test } from "bun:test"
import {
  backFrom,
  handAfterCatalog,
  openingHand,
} from "../src/boxbuster-hand"
import type { CounterStatus, CounterTape } from "../src/boxbuster-store-view"

function tape(id: string, aisle: CounterTape["aisle"]): CounterTape {
  return {
    id,
    title: id,
    aisle,
    verb: aisle === "returns" ? "Resume" : "Play",
    facts: [],
    launch: { _tag: "Here" },
  }
}

const idle: CounterStatus = { _tag: "Idle" }

describe("opening the store", () => {
  test("hands you the first tape on the return cart", () => {
    expect(
      openingHand([
        tape("resume-a", "returns"),
        tape("resume-b", "returns"),
        tape("recent", "newReleases"),
      ]),
    ).toEqual({ _tag: "Holding", tapeId: "resume-a" })
  })

  test("with nothing to resume, stands you before the first tape", () => {
    expect(
      openingHand([tape("recent", "newReleases"), tape("old", "classics")]),
    ).toEqual({ _tag: "Browsing", tapeId: "recent" })
  })

  test("with no tapes, holds nothing", () => {
    expect(openingHand([])).toEqual({ _tag: "NoTape" })
  })
})

describe("when Korri republishes the catalog", () => {
  const tapes = [tape("resume", "returns"), tape("recent", "newReleases")]

  test("the first stocked catalog opens the store", () => {
    // The surface mounts while the catalog is still loading.
    expect(handAfterCatalog({ _tag: "NoTape" }, tapes)).toEqual({
      _tag: "Holding",
      tapeId: "resume",
    })
  })

  test("a tape still in the store stays where it was", () => {
    const browsing = { _tag: "Browsing", tapeId: "recent" } as const
    expect(handAfterCatalog(browsing, tapes)).toBe(browsing)
    const holding = { _tag: "Holding", tapeId: "recent" } as const
    expect(handAfterCatalog(holding, tapes)).toBe(holding)
  })

  test("a tape that left the store leaves your hand", () => {
    expect(
      handAfterCatalog({ _tag: "Holding", tapeId: "gone" }, tapes),
    ).toEqual({ _tag: "Browsing", tapeId: "resume" })
  })

  test("a store that stays empty keeps the same hand", () => {
    // The surface reconciles during render, so an unchanged hand must be the
    // same value or React would render forever.
    const none = { _tag: "NoTape" } as const
    expect(handAfterCatalog(none, [])).toBe(none)
  })

  test("an emptied store leaves you holding nothing", () => {
    expect(
      handAfterCatalog({ _tag: "Browsing", tapeId: "recent" }, []),
    ).toEqual({ _tag: "NoTape" })
  })
})

describe("Back", () => {
  test("clears a problem first", () => {
    expect(
      backFrom(
        { _tag: "Holding", tapeId: "a" },
        { _tag: "Problem", kicker: "No", reason: "No.", canRetry: false },
      ),
    ).toEqual({ _tag: "Dismiss" })
  })

  test("puts a held tape back, leaving you in front of it", () => {
    expect(backFrom({ _tag: "Holding", tapeId: "a" }, idle)).toEqual({
      _tag: "PutDown",
      hand: { _tag: "Browsing", tapeId: "a" },
    })
  })

  test("does nothing with empty hands", () => {
    expect(backFrom({ _tag: "Browsing", tapeId: "a" }, idle)).toEqual({
      _tag: "Ignore",
    })
    expect(backFrom({ _tag: "NoTape" }, idle)).toEqual({ _tag: "Ignore" })
  })
})
