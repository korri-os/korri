/**
 * Korri's long words in the sky dialog (src/plan/dialog-words.ts): the
 * headline and the buttons always stand whole, the lines fit the column,
 * no word is lost, and up and down reach every line.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel } from "../../../../../contracts/generated/surface"
import { dialogButtons, zoneScreen } from "../src/plan/screen"
import { dialogInnerWidth, dialogRoom, dialogWindow, dialogWords, lastFirst, wrapWords } from "../src/plan/dialog-words"
import { measureWords } from "../src/plan/measure"
import { ZONE_LOOKS } from "../src/plan/look"
import { SIZES, Walk } from "./walk"

const WINDOWS = [[240, 480], [320, 240], [1280, 300], ...SIZES] as const

/** A reason as korrid sent one to an RG353M (608 characters). */
const LONG = [
  "average-day-in-america-by-bug",
  "eggshell-66c0dfbd9ceda85a76e3e25b",
  "eggshell-68c27d6be7c30cea4ebd4641",
  "eggshell-6a2ee4c46c21aa46014604d2",
  "eggshell-rust-spike",
  "mega-man-rock-n-roll",
].map(game => `local games: LocalRouteUnavailable: game ${game} has no installed Linux runner`).join(" · ")

const UNBROKEN = "supercalifragilisticexpialidocious".repeat(4)

/** A problem dialog as src/view.ts makes one. */
const problem = (reason: string, headline = "Catalog problem", title = "Spelunky") =>
  ({ headline, korri: true, lines: [reason], title, buttons: [{ label: "Dismiss" }, { label: "Retry" }] })

/** The words without spaces and hyphens: what a line break may not lose. */
const letters = (text: string) => text.replace(/[\s-]/g, "")

describe("the dialog's lines", () => {
  test("every line fits the column, and no letter is lost", () => {
    const out: string[] = []
    for (const look of ZONE_LOOKS) {
      for (const [w, h] of WINDOWS) {
        const screen = zoneScreen(w, h, look)
        for (const text of [LONG, UNBROKEN, "Short.", `${UNBROKEN} ${LONG}`]) {
          const words = dialogWords(problem(text), dialogRoom(screen, false, true), screen)
          for (const line of words.lines) {
            const width = measureWords(line, 10 * screen.a, "fact", look)
            if (width > dialogInnerWidth(screen)) out.push(`${look} ${w}x${h}: "${line}" is ${width} px`)
          }
          if (letters(words.lines.join(" ")) !== letters(text)) out.push(`${look} ${w}x${h}: letters lost from ${text.slice(0, 20)}`)
        }
      }
    }
    expect(out).toEqual([])
  })

  test("a line breaks between words; a word wider than the line breaks after its own hyphen, else with one", () => {
    expect(wrapWords("one two three", 1000, 10, "fact", "blend")).toEqual(["one two three"])
    const narrow = measureWords("average-day-", 10, "fact", "blend") + 1
    expect(wrapWords("average-day-in-america", narrow, 10, "fact", "blend")[0]).toBe("average-day-")
    const unbroken = wrapWords(UNBROKEN, 100, 10, "fact", "blend")
    expect(unbroken.length).toBeGreaterThan(1)
    for (const line of unbroken.slice(0, -1)) expect(line.endsWith("-")).toBe(true)
  })

  test("the headline, the lines, the game and the buttons fit the layer at every size", () => {
    const out: string[] = []
    for (const look of ZONE_LOOKS) {
      for (const [w, h] of WINDOWS) {
        const screen = zoneScreen(w, h, look)
        const a = screen.a
        for (const overlay of [false, true]) {
          for (const headline of ["Catalog problem", "Could not start Hollow Knight on zao"]) {
            const room = dialogRoom(screen, overlay, true)
            const words = dialogWords(problem(LONG, headline), room, screen)
            const n = words.shown
            const head = (words.head.length - 1) * 1.4 * words.headCap + words.headCap
            const panel = 16 * a + head + 12 * a + 16 * a + dialogButtons(["Dismiss", "Retry"], screen).needed + (n - 1) * 14 * a + 10 * a
            if (panel > room) out.push(`${look} ${w}x${h}${overlay ? " overlay" : ""} "${headline}": ${panel} px in ${room}`)
            if (n < 1) out.push(`${look} ${w}x${h}: no line shown`)
          }
        }
      }
    }
    expect(out).toEqual([])
  })

  test("the headline is never cut: caps-2 while it fits the column, else caps-1 broken between words", () => {
    const out: string[] = []
    for (const look of ZONE_LOOKS) {
      for (const [w, h] of WINDOWS) {
        const screen = zoneScreen(w, h, look)
        for (const headline of ["Catalog problem", "Could not start Hollow Knight on zao", "Paused"]) {
          const words = dialogWords(problem("Short.", headline), dialogRoom(screen, false, true), screen)
          for (const line of words.head) {
            const width = measureWords(line, words.headCap, "korri", look)
            if (width > dialogInnerWidth(screen)) out.push(`${look} ${w}x${h}: "${line}" is ${width} px`)
          }
          if (letters(words.head.join(" ")) !== letters(headline)) out.push(`${look} ${w}x${h}: letters lost from ${headline}`)
        }
      }
    }
    expect(out).toEqual([])
    const wide = zoneScreen(1920, 1080, "blend")
    expect(dialogWords(problem("Short."), 1000, wide)).toMatchObject({ head: ["Catalog problem"], headCap: 2 * 10 * wide.a })
    const narrow = zoneScreen(480, 800, "retro")
    expect(dialogWords(problem("Short."), 1000, narrow).headCap).toBe(10 * narrow.a)
  })

  test("the window: whole lines, inside them, with what stands above and below", () => {
    const screen = zoneScreen(640, 480, "blend")
    const words = dialogWords(problem(LONG, "Catalog problem", ""), dialogRoom(screen, false, true), screen)
    expect(words.shown).toBeLessThan(words.lines.length)
    expect(dialogWindow(words, 0)).toMatchObject({ above: false, below: true })
    const end = dialogWindow(words, 999)
    expect(end).toMatchObject({ above: true, below: false })
    expect(end.lines).toEqual(words.lines.slice(lastFirst(words)))
  })
})

describe("the status layer's dialog scrolls with up and down", () => {
  for (const [w, h] of [...SIZES, [240, 480]] as const) {
    test(`${w}x${h}`, () => {
      const walk = new Walk("status-problem-long", w, h)
      const shown = () => walk.view().dialog as { headline: string; lines: string[]; breaks: number; above: boolean; below: boolean; buttons: { label: string; focused?: boolean }[] }
      // ZoneDialog's one text, a line break between each line.
      const dialog = () => {
        const view = shown()
        const lines = view.lines.flatMap(text => text.split("\n"))
        expect(view.breaks).toBe(Math.max(0, lines.length - 1))
        return { ...view, lines }
      }
      const start = dialog()
      expect(start.headline).toBe("Catalog problem")
      expect(start.buttons.map(button => button.label)).toEqual(["Dismiss", "Retry"])
      expect(start.above).toBe(false)
      // The reason as read on the way down: each step brings one new line in at the foot.
      const read = [...start.lines]
      let before = start.lines
      for (let i = 0; i < 40; i++) {
        walk.press("down")
        const now = dialog().lines
        if (now.join("\n") !== before.join("\n")) {
          expect(now.slice(0, -1)).toEqual(before.slice(1))
          read.push(now.at(-1)!)
        }
        before = now
      }
      const end = dialog()
      expect(end.below).toBe(false)
      // Every line of the reason was shown on the way down, in order.
      expect(letters(read.join(" "))).toBe(letters((walk.model.status as { reason: string }).reason))
      for (let i = 0; i < 40; i++) walk.press("up")
      expect(dialog().above).toBe(false)
      expect(dialog().lines).toEqual(start.lines)
      // The buttons are still reachable, and DISMISS still dismisses.
      expect(dialog().buttons.some(button => button.focused === true)).toBe(true)
    })
  }

  test("a new reason starts at its first line", () => {
    const walk = new Walk("status-problem-long", 640, 480)
    for (let i = 0; i < 5; i++) walk.press("down")
    expect(walk.state.zone.dialogLine.line).toBeGreaterThan(0)
    const status = walk.model.status as Extract<SurfaceModel["status"], { _tag: "Problem" }>
    walk.model = { ...walk.model, status: { ...status, reason: `${status.reason} · one more` } }
    walk.deliver({ _tag: "KorriPublished" })
    expect((walk.view().dialog as { above: boolean }).above).toBe(false)
    // and the next down moves it from there.
    walk.press("down")
    expect(walk.state.zone.dialogLine.line).toBe(1)
  })

  test("every sky dialog: GAMES' catalog error moves its lines, then up goes to the tabs", () => {
    const walk = new Walk("games-error", 640, 480)
    walk.model = { ...walk.model, catalog: { _tag: "Error", message: LONG } }
    walk.deliver({ _tag: "KorriPublished" })
    const dialog = () => walk.view().dialog as { lines: string[]; above: boolean; below: boolean }
    expect(dialog()).toMatchObject({ above: false, below: true })
    walk.press("down")
    walk.press("down")
    expect(dialog().above).toBe(true)
    walk.press("up")
    walk.press("up")
    expect(dialog().above).toBe(false)
    expect(walk.state.zone.nav.zone).not.toBe("tabs")
    walk.press("up")
    expect(walk.state.zone.nav.zone).toBe("tabs")
  })

  test("a short reason does not move", () => {
    const walk = new Walk("status-problem", 640, 480)
    const before = walk.state
    walk.press("down")
    walk.press("up")
    expect(walk.state.zone.dialogLine).toEqual(before.zone.dialogLine)
    expect(walk.state.zone.status).toEqual(before.zone.status)
    expect(walk.view().dialog).toMatchObject({ above: false, below: false })
  })
})
