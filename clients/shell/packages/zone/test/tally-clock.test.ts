/**
 * The results tally's timed behaviour, from the shell's Ticks (contract 7.1)
 * and Zone's own state, as the portal's timers had it (use-zone-games.ts):
 * TallyCounted once the count-up has run (zoneTallyCountMs), TallyElapsed
 * at ZONE_TALLY_CLOSE_MS (8 s), counted across presses; the attract waits
 * while the tally shows. ZoneTally counts the values up with their bars
 * from each row's `kind` and `amount`.
 */
import { describe, expect, test } from "bun:test"
import { zoneTallyCountMs } from "../src/plan/motion"
import { ZONE_TALLY_CLOSE_MS } from "../src/state/navigation"
import { Walk } from "./walk"

type Tally = { shown: boolean; counted: boolean; rows: { kind: string; amount: number; value: string }[] }
const tallyOf = (walk: Walk) => walk.view().tally as Tally

/** Ticks once a second from `from` ms of idle time, as the shell sends them. */
function ticks(walk: Walk, from: number, seconds: number): void {
  for (let s = 0; s <= seconds; s++) walk.deliver({ _tag: "Tick", idleMs: from + s * 1000 })
}

describe("the results tally's clock", () => {
  test("the scenario opens a tally that is still counting", () => {
    const walk = new Walk("games-tally", 640, 480)
    expect(tallyOf(walk)).toMatchObject({ shown: true, counted: false })
  })

  test("TallyCounted once the count-up has run, then the close at 8 s", () => {
    const walk = new Walk("games-tally", 640, 480)
    const count = zoneTallyCountMs(tallyOf(walk).rows.length)
    ticks(walk, 0, Math.ceil(count / 1000))
    expect(tallyOf(walk)).toMatchObject({ shown: true, counted: true })
    ticks(walk, Math.ceil(count / 1000) * 1000, Math.ceil((ZONE_TALLY_CLOSE_MS - count) / 1000))
    expect(tallyOf(walk).shown).toBe(false)
  })

  test("not before its time: 7 s of ticks leave it up", () => {
    const walk = new Walk("games-tally", 640, 480)
    ticks(walk, 0, 7)
    expect(tallyOf(walk).shown).toBe(true)
  })

  test("a long idle time before the tally does not close it early, and the attract waits", () => {
    const walk = new Walk("games-tally", 640, 480)
    ticks(walk, 600_000, 5)
    expect(tallyOf(walk).shown).toBe(true)
    expect(walk.view().attract).toBe(false)
    ticks(walk, 606_000, 4)
    expect(tallyOf(walk).shown).toBe(false)
  })

  test("600 s of ticks never leave it up uncounted (the review's case)", () => {
    const walk = new Walk("games-tally", 640, 480)
    ticks(walk, 0, 600)
    expect(tallyOf(walk).shown).toBe(false)
  })

  test("the clock runs on across a press (which resets the shell's idle time)", () => {
    const walk = new Walk("games-tally", 640, 480)
    ticks(walk, 0, 1)
    // The first press skips the count-up and leaves the tally up.
    walk.press("confirm")
    expect(tallyOf(walk)).toMatchObject({ shown: true, counted: true })
    ticks(walk, 0, 5)
    expect(tallyOf(walk).shown).toBe(true)
    ticks(walk, 6000, 1)
    expect(tallyOf(walk).shown).toBe(false)
  })

  test("each row carries what its value counts up to", () => {
    const rows = tallyOf(new Walk("games-tally", 640, 480)).rows
    expect(rows.map(row => row.kind)).toContain("plays")
    for (const row of rows) expect(row.amount).toBeGreaterThan(0)
  })
})
