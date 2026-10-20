/**
 * The launch title card and the results tally as pure values
 * (src/state/tally.ts, navigation.ts): each rule at its boundary.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceGame, SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { zoneTallyRows, zoneTallyTime, zoneTallyTotal, zoneTallyCounts } from "../src/plan/tally"
import { update, ZONE_START, type ZoneNavigation, type ZoneNavMessage } from "../src/state/navigation"
import {
  pressTally,
  settleLaunch,
  settleSession,
  zoneActOf,
  zoneLaunchOpen,
  zoneTallyOf,
  type ZoneLaunch,
  type ZoneSession,
} from "../src/state/tally"

const game = (over: Partial<SurfaceGame> = {}): SurfaceGame => ({ id: "celeste", title: "Celeste Classic", subtitle: "PICO-8", ...over })
const model = (games: readonly SurfaceGame[], status: SurfaceStatus = { _tag: "Browsing" }): SurfaceModel => ({
  presentation: { kind: "catalog" },
  catalog: { _tag: "Ready", games },
  status,
  actions: [],
  settings: [],
  settingsStatus: { _tag: "Idle" },
})
const running: SurfaceStatus = { _tag: "Running", kicker: "Playing", gameId: "celeste" }
const busy: SurfaceStatus = { _tag: "Busy", kicker: "Starting", gameId: "celeste" }

describe("the act", () => {
  test("is the next session: playCount + 1, and 1 when Korri has recorded none", () => {
    expect(zoneActOf({})).toBe(1)
    expect(zoneActOf({ playCount: 0 })).toBe(1)
    expect(zoneActOf({ playCount: 23 })).toBe(24)
  })
  test("a launch opens with the catalog's title, system and act; an unknown game opens nothing", () => {
    expect(zoneLaunchOpen(model([game({ playCount: 7 })]), "celeste"))
      .toEqual({ gameId: "celeste", title: "Celeste Classic", system: "PICO-8", act: 8, seen: false, leaving: false })
    expect(zoneLaunchOpen(model([game()]), "tetris")).toBeUndefined()
    expect(zoneLaunchOpen(model([game({ title: "" })]), "celeste")).toBeUndefined()
  })
})

describe("the launch card against Korri's status", () => {
  const open = zoneLaunchOpen(model([game()]), "celeste") as ZoneLaunch
  test("waits through Browsing before Korri answers", () => {
    expect(settleLaunch(open, { _tag: "Browsing" })).toBe(open)
  })
  test("is seen once Busy, Confirm or Running names the game, or names none", () => {
    expect(settleLaunch(open, busy)?.seen).toBe(true)
    expect(settleLaunch(open, { _tag: "Confirm", kicker: "Download?", actions: [], gameId: "celeste" })?.seen).toBe(true)
    expect(settleLaunch(open, running)?.seen).toBe(true)
    expect(settleLaunch(open, { _tag: "Busy", kicker: "Starting" })?.seen).toBe(true)
  })
  test("does not count another game's status as its own", () => {
    expect(settleLaunch(open, { _tag: "Running", kicker: "Playing", gameId: "tetris" })).toBe(open)
  })
  test("leaves when Korri lets go of a seen launch, or answers with a problem", () => {
    const seen = { ...open, seen: true }
    expect(settleLaunch(seen, running)).toBe(seen)
    expect(settleLaunch(seen, { _tag: "Browsing" })?.leaving).toBe(true)
    expect(settleLaunch(open, { _tag: "Problem", kicker: "No", reason: "zao did not answer.", canRetry: false })?.leaving).toBe(true)
  })
  test("leaving is final", () => {
    const leaving = { ...open, seen: true, leaving: true }
    expect(settleLaunch(leaving, running)).toBe(leaving)
  })
})

describe("the session and its tally", () => {
  const start = (playCount?: number, seconds?: number): ZoneSession | undefined =>
    settleSession(undefined, model([game({ playCount, totalPlaytimeSeconds: seconds })], running))
  const back = (playCount?: number, seconds?: number) => model([game({ playCount, totalPlaytimeSeconds: seconds })])

  test("the first Running notes the counts as they stood, and keeps them", () => {
    const session = start(23, 41_460)
    expect(session).toEqual({ gameId: "celeste", playCount: 23, seconds: 41_460 })
    expect(settleSession(session, model([game({ playCount: 24 })], running))).toBe(session)
  })
  test("Browsing ends the session; Busy between keeps it", () => {
    const session = start(23, 41_460)
    expect(settleSession(session, back(24))).toBeUndefined()
    expect(settleSession(session, model([game()], busy))).toBe(session)
  })
  test("a rise of exactly one is a tally: TIME the rise, PLAYS and TOTAL the new counts", () => {
    expect(zoneTallyOf(start(23, 41_460), back(24, 45_794)))
      .toEqual({ gameId: "celeste", title: "Celeste Classic", seconds: 4334, plays: 24, total: 45_794, counted: false, leaving: false })
  })
  test("no rise, a rise of two, or no count: no tally", () => {
    expect(zoneTallyOf(start(23, 41_460), back(23, 45_794))).toBeUndefined()
    expect(zoneTallyOf(start(23, 41_460), back(25, 45_794))).toBeUndefined()
    expect(zoneTallyOf(start(23, 41_460), back(undefined, 45_794))).toBeUndefined()
  })
  test("the first session counts from none: absent then 1 is a rise of one, from 0 seconds", () => {
    expect(zoneTallyOf(start(), back(1, 600))).toMatchObject({ plays: 1, seconds: 600, total: 600 })
  })
  test("TIME is omitted when the total did not move or cannot be known", () => {
    expect(zoneTallyOf(start(23, 41_460), back(24, 41_460))?.seconds).toBeUndefined()
    expect(zoneTallyOf(start(23), back(24, 41_460))?.seconds).toBeUndefined()
    expect(zoneTallyOf(start(23, 41_460), back(24))).toMatchObject({ plays: 24 })
    expect(zoneTallyOf(start(23, 41_460), back(24))?.total).toBeUndefined()
  })
  test("a press skips the count-up, the next closes it", () => {
    const tally = zoneTallyOf(start(23, 41_460), back(24, 45_794))!
    expect(pressTally(tally)).toMatchObject({ counted: true, leaving: false })
    expect(pressTally(pressTally(tally))).toMatchObject({ counted: true, leaving: true })
  })
})

describe("navigation holds the moments", () => {
  const korri = model([game({ playCount: 23, totalPlaytimeSeconds: 41_460 })])
  const step = (nav: ZoneNavigation, message: ZoneNavMessage, with_: SurfaceModel = korri) => update(nav, message, with_).model

  test("PLAY opens the card with the act; Korri's answer holds it; Browsing sends it out; the timer ends it", () => {
    let nav = step(ZONE_START, { _tag: "ChoseLaunch", gameId: "celeste" })
    expect(nav.launch).toMatchObject({ act: 24, seen: false })
    nav = step(nav, { _tag: "KorriPublished" }, model(korri.catalog._tag === "Ready" ? korri.catalog.games : [], running))
    expect(nav.launch?.seen).toBe(true)
    // A press while Korri holds the game does nothing.
    expect(step(nav, { _tag: "PressedMenu" }, model(korri.catalog._tag === "Ready" ? korri.catalog.games : [], running))).toEqual(nav)
    nav = step(nav, { _tag: "KorriPublished" })
    expect(nav.launch?.leaving).toBe(true)
    nav = step(nav, { _tag: "TitleCardLeft" })
    expect(nav.launch).toBeUndefined()
  })
  test("before Korri answers, a press lets go of the card and does what it does", () => {
    let nav = step(ZONE_START, { _tag: "ChoseLaunch", gameId: "celeste" })
    nav = step(nav, { _tag: "PressedMenu" })
    expect(nav.launch?.leaving).toBe(true)
    expect(nav.zone).toBe("tabs")
  })
  test("the tally holds every press: one skips, the next closes; its timers count and close it", () => {
    const after = model([game({ playCount: 24, totalPlaytimeSeconds: 45_794 })])
    let nav = step(ZONE_START, { _tag: "KorriPublished" }, model([game({ playCount: 23, totalPlaytimeSeconds: 41_460 })], running))
    nav = step(nav, { _tag: "KorriPublished" }, after)
    expect(nav.tally).toMatchObject({ plays: 24, counted: false })
    const skipped = step(nav, { _tag: "PressedBack" }, after)
    expect(skipped.tally).toMatchObject({ counted: true, leaving: false })
    expect(skipped.zone).toBe(nav.zone)
    expect(step(skipped, { _tag: "FocusedTab", tab: "options" }, after).tally?.leaving).toBe(true)
    expect(step(nav, { _tag: "TallyCounted" }, after).tally?.counted).toBe(true)
    const closing = step(nav, { _tag: "TallyElapsed" }, after)
    expect(closing.tally?.leaving).toBe(true)
    expect(step(closing, { _tag: "TallyLeft" }, after).tally).toBeUndefined()
    // Idle never starts over the tally.
    expect(step(nav, { _tag: "IdleElapsed" }, after).layers).toEqual([])
  })
  test("a tally opens only over GAMES with nothing of Zone's own over it", () => {
    const after = model([game({ playCount: 24 })])
    const start = step({ ...ZONE_START, tab: "options" }, { _tag: "KorriPublished" }, model([game({ playCount: 23 })], running))
    expect(step(start, { _tag: "KorriPublished" }, after).tally).toBeUndefined()
  })
})

describe("the tally's words", () => {
  test("TIME as minutes and seconds, TOTAL as hours and minutes", () => {
    expect(zoneTallyTime(4334)).toBe(`72'14"`)
    expect(zoneTallyTime(59)).toBe(`0'59"`)
    expect(zoneTallyTotal(11_520)).toBe("3H 12M")
    expect(zoneTallyTotal(3599)).toBe("59M")
    expect(zoneTallyTotal(3600)).toBe("1H 0M")
  })
  test("rows: TIME only with a session length; TIME's bar is the session's share of the total", () => {
    expect(zoneTallyRows({ plays: 1 }).map(row => row.kind)).toEqual(["plays"])
    expect(zoneTallyRows({ plays: 8, seconds: 3000, total: 12_000 }))
      .toEqual([{ kind: "time", label: "Time", share: 0.25 }, { kind: "plays", label: "Plays", share: 1 }, { kind: "total", label: "Total", share: 1 }])
  })
  test("the count-up writes whole numbers, reaching the value at its end", () => {
    expect(zoneTallyCounts("time", { plays: 1, seconds: 4334 }, 1)).toEqual({ "--zone-tally-time-m": 72, "--zone-tally-time-s": 14 })
    expect(zoneTallyCounts("time", { plays: 1, seconds: 4334 }, 0.5)).toEqual({ "--zone-tally-time-m": 36, "--zone-tally-time-s": 7 })
    expect(zoneTallyCounts("plays", { plays: 24 }, 0)).toEqual({ "--zone-tally-plays": 0 })
    expect(zoneTallyCounts("total", { plays: 1, total: 11_520 }, 1)).toEqual({ "--zone-tally-total-h": 3, "--zone-tally-total-m": 12 })
  })
})
