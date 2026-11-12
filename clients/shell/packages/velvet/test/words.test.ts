/**
 * Velvet's words about time. "Now" is the model's `publishedAt`, which can be
 * a minute behind and comes from a device clock that can be wrong: a session
 * that ended "after" it is "Just now", never a negative time. Without a
 * `publishedAt` the fact is a date, and nothing is "within" a week.
 */
import { describe, expect, test } from "bun:test"
import { join } from "node:path"
import { ago, bytes, caps, dayOf, duration, games, played, playtime, saved, within } from "../src/words"

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

describe("a surface has no time zone", () => {
  // Every time Korri publishes is UTC. The words must be the same on a device set to any zone.
  const zones = ["UTC", "America/Los_Angeles", "Pacific/Kiritimati", "Asia/Kolkata"]
  const run = (zone: string) => {
    const result = Bun.spawnSync([process.execPath, join(import.meta.dir, "words-zone.ts")], { env: { ...process.env, TZ: zone } })
    expect(result.exitCode, `${zone}: ${result.stderr.toString()}`).toBe(0)
    return JSON.parse(result.stdout.toString()) as Record<string, unknown>
  }
  const words = zones.map(run)

  test("the zones under test really differ", () => {
    const hours = words.map(entry => entry.localHourOfTheZoneUnderTest)
    expect(hours[0]).toBe(0)
    expect(new Set(hours).size).toBe(zones.length)
  })

  test("the same words under every zone", () => {
    const strip = ({ localHourOfTheZoneUnderTest: _hour, ...rest }: Record<string, unknown>) => rest
    for (let i = 1; i < zones.length; i++) expect(strip(words[i]!), zones[i]).toEqual(strip(words[0]!))
  })

  test("an absolute date is the UTC date", () => {
    expect(dayOf(Date.UTC(2026, 10, 6, 0, 30))).toBe("6 Nov 2026")
    expect(dayOf(Date.UTC(2026, 10, 5, 23, 59))).toBe("5 Nov 2026")
    expect((words[1]!.dayOf as string[])[0]).toBe("6 Nov 2026")
    expect((words[2]!.dayOf as string[])[0]).toBe("6 Nov 2026")
  })

  test("how long ago is the time between Korri's two numbers, never a calendar day", () => {
    // 26 hours is "Yesterday" whatever the hour it is counted from.
    expect(new Set(words[0]!.boundary as string[])).toEqual(new Set(["Yesterday"]))
    // 23 hours ago is hours, though the date changed; 25 hours is a day, though only one midnight passed.
    expect(ago(NOW - 23 * HOUR, NOW)).toBe("23 hours ago")
    expect(ago(NOW - 47 * HOUR, NOW)).toBe("Yesterday")
    expect(ago(NOW - 49 * HOUR, NOW)).toBe("2 days ago")
  })

  test("no clock time is worked out: a save says how long ago, then the UTC date", () => {
    expect(saved(NOW + MINUTE, NOW)).toBe("Just now")
    expect(saved(NOW - 45 * MINUTE, NOW)).toBe("45 min ago")
    expect(saved(NOW - (3 * HOUR + 20 * MINUTE), NOW)).toBe("3 h 20 min ago")
    expect(saved(NOW - 7 * HOUR, NOW)).toBe("7 h ago")
    expect(saved(NOW - (DAY + 30 * MINUTE), NOW)).toBe("1 day ago")
    expect(saved(NOW - (5 * DAY + 3 * HOUR), NOW)).toBe("5 days 3 h ago")
    expect(saved(NOW - 45 * DAY, NOW)).toBe(dayOf(NOW - 45 * DAY))
    expect(saved(Date.UTC(2026, 10, 9, 14, 47), undefined)).toBe("9 Nov 2026")
    for (const entry of words) for (const text of [...(entry.saved as string[]), ...(entry.undated as string[])]) expect(text).not.toMatch(/\d:\d\d/)
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
