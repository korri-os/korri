/**
 * Velvet's words about time. "Now" is the model's `publishedAt`, which can be
 * a minute behind and comes from a device clock that can be wrong: a session
 * that ended "after" it is "Just now", never a negative time. Without a
 * `publishedAt` the fact is a date, and nothing is "within" a week.
 */
import { describe, expect, test } from "bun:test"
import { ago, bytes, caps, duration, games, played, playtime, within } from "../src/words"

const NOW = Date.UTC(2026, 10, 11, 21, 47)
const MINUTE = 60_000, HOUR = 60 * MINUTE, DAY = 24 * HOUR

describe("ago", () => {
  test("a session Korri has not seen yet is 'Just now', never negative", () => {
    expect(ago(NOW + 45_000, NOW)).toBe("Just now")
    expect(ago(NOW + 3 * DAY, NOW)).toBe("Just now")
    expect(ago(NOW, NOW)).toBe("Just now")
    expect(ago(NOW - 59_000, NOW)).toBe("Just now")
  })
  test("minutes, hours, days, weeks, months, years", () => {
    expect(ago(NOW - MINUTE, NOW)).toBe("A minute ago")
    expect(ago(NOW - 12 * MINUTE, NOW)).toBe("12 minutes ago")
    expect(ago(NOW - HOUR, NOW)).toBe("An hour ago")
    expect(ago(NOW - 2 * HOUR, NOW)).toBe("2 hours ago")
    expect(ago(NOW - 27 * HOUR, NOW)).toBe("Yesterday")
    expect(ago(NOW - 5 * DAY, NOW)).toBe("5 days ago")
    expect(ago(NOW - 21 * DAY, NOW)).toBe("3 weeks ago")
    expect(ago(NOW - 122 * DAY, NOW)).toBe("4 months ago")
    expect(ago(NOW - 800 * DAY, NOW)).toBe("2 years ago")
  })
  test("never played", () => {
    expect(ago(undefined, NOW)).toBe("Not played yet")
    expect(played(undefined, NOW)).toBe("Not played yet")
  })
  test("without Korri's time the fact is a date, not a guess", () => {
    expect(ago(Date.UTC(2026, 10, 6, 12), undefined)).toBe("6 Nov 2026")
    expect(played(Date.UTC(2026, 10, 6, 12), undefined)).toBe("Played 6 Nov 2026")
  })
  test("inside a sentence", () => {
    expect(played(NOW - 2 * HOUR, NOW)).toBe("Played 2 hours ago")
    expect(played(NOW + MINUTE, NOW)).toBe("Played just now")
  })
})

describe("within", () => {
  test("the pick's week", () => {
    expect(within(NOW - 6 * DAY, NOW, 7)).toBe(true)
    expect(within(NOW - 8 * DAY, NOW, 7)).toBe(false)
    expect(within(NOW + HOUR, NOW, 7)).toBe(true)
  })
  test("false with nothing to compare", () => {
    expect(within(undefined, NOW, 7)).toBe(false)
    expect(within(NOW, undefined, 7)).toBe(false)
  })
})

describe("amounts", () => {
  test("playtime", () => {
    expect(playtime(undefined)).toBeUndefined()
    expect(playtime(0)).toBeUndefined()
    expect(playtime(45 * 60)).toBe("45 min")
    expect(playtime(61 * 3600 + 20 * 60)).toBe("61 h 20 min")
    expect(duration(90 * 60)).toBe("1 h 30 min")
  })
  test("games", () => {
    expect(games(0)).toBe("0 games")
    expect(games(1)).toBe("1 game")
    expect(games(10000)).toBe("10,000 games")
  })
  test("bytes", () => {
    expect(bytes(1_240_000_000)).toMatch(/^1\.[12] GB$/)
    expect(bytes(612_000_000)).toMatch(/MB$/)
  })
  test("caps keeps a small letter that has no capital of its own size", () => {
    expect(caps("Ember Road")).toBe("EMBER ROAD")
    expect(caps("µCity")).not.toContain("Μ")
  })
})
