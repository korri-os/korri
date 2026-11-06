/**
 * The sky dialog's words (ZoneDialog): Korri's lines, broken into the lines
 * ZoneDialog draws, and the window of them that fits between the headline
 * and the buttons.
 *
 * The headline and the buttons always stand whole: they are the dialog's
 * name and its only ways out. The headline stands at caps-2 while it fits
 * the column, else at caps-1 broken between words. Korri's lines take the
 * height that is left.
 * When they need more, the dialog shows as many whole lines as fit and the
 * up and down presses that no button takes move that window one line at a
 * time (src/press.ts; the surface state's `dialogLine` keeps it). Two triangles beside the lines show it: black where
 * more lines stand that way, steel at the end, as the card band's end
 * triangles do.
 *
 * Words are measured as .slint draws them (src/plan/measure.ts), so the
 * lines here are the lines drawn. A line breaks between words; a word wider
 * than the line breaks at the line's end with a hyphen (REFINEMENT.md, the
 * stress set: the one break allowed inside a word, for a Korri sentence).
 *
 * All lengths are px; `a` is the art px.
 */
import { measureWords, type ZoneWordFace } from "./measure"
import { dialogButtons, type ZoneScreen } from "./screen"
import type { ZoneLook } from "./look"

/** ZoneDialog's lengths, in art px. */
const PAD = 8
const GAP = 6
/** A line's cap (caps-1), and lines 1.4 caps apart (ZoneCaps wrapped). */
const CAP = 10
const PITCH = 14
/** The scroll triangles: 7 art px wide, in room kept for them on each side of the lines. */
export const ZONE_DIALOG_MARK_W = 7
export const ZONE_DIALOG_MARK_ROOM = 14
/** The lines' column is never wider than this (art px). */
const MOST_INNER = 46 * 8

/** The lines' column, as ZoneDialog's `inner-w`. */
export function dialogInnerWidth(screen: ZoneScreen): number {
  const a = screen.a
  return Math.min(screen.width - 2 * PAD * a - screen.width / 4, MOST_INNER * a)
}

/**
 * The height ZoneDialog stands in (ZoneStatusLayer: the window between the
 * page's header and its foot; PAUSED's overlay: the whole window).
 */
export function dialogRoom(screen: ZoneScreen, overlay: boolean, foot: boolean): number {
  if (overlay) return screen.height
  return screen.height - screen.topH - (foot ? screen.footH : 0)
}

/** `text` broken into lines no wider than `width` (px), measured as ZoneCaps draws them. */
export function wrapWords(text: string, width: number, cap: number, face: ZoneWordFace, look: ZoneLook): string[] {
  const fits = (words: string) => measureWords(words, cap, face, look) <= width
  const out: string[] = []
  for (const paragraph of text.split("\n")) {
    let line = ""
    for (const found of paragraph.split(/\s+/)) {
      if (found === "") continue
      const joined = line === "" ? found : `${line} ${found}`
      if (fits(joined)) {
        line = joined
        continue
      }
      if (line !== "") out.push(line)
      // A word wider than the line breaks after its own last hyphen that fits,
      // else at the line's end with a hyphen added.
      let chars = [...found]
      while (!fits(chars.join(""))) {
        let take = 1
        while (take + 1 < chars.length && fits(`${chars.slice(0, take + 1).join("")}-`)) take++
        const own = chars.slice(0, take + 1).lastIndexOf("-")
        if (own > 0 && own < chars.length - 1 && fits(chars.slice(0, own + 1).join(""))) take = own + 1
        const head = chars.slice(0, take).join("")
        out.push(head.endsWith("-") ? head : `${head}-`)
        chars = chars.slice(take)
      }
      line = chars.join("")
    }
    if (line !== "") out.push(line)
  }
  return out
}

/** What the plan reads of a dialog (src/view.ts dialogView). */
export interface ZoneDialogShape {
  readonly headline: string
  /** The headline is Korri's (its names' face), not Zone's caps. */
  readonly korri: boolean
  readonly lines: readonly string[]
  readonly title: string
  readonly buttons: readonly { readonly label: string }[]
}

export interface ZoneDialogWords {
  /** The headline's lines: one at caps-2 while it fits the column, else broken between words at caps-1. */
  readonly head: readonly string[]
  /** The headline's cap, px. */
  readonly headCap: number
  /** Every line of Korri's words, as ZoneDialog would draw them all. */
  readonly lines: readonly string[]
  /** How many lines the dialog shows at once (all of them when they fit). */
  readonly shown: number
}

const planned = new Map<string, ZoneDialogWords>()

/** The headline, Korri's lines and the window of them that fits, for one dialog in one window. */
export function dialogWords(dialog: ZoneDialogShape, room: number, screen: ZoneScreen): ZoneDialogWords {
  const labels = dialog.buttons.map(button => button.label)
  const key = `${screen.width}|${screen.height}|${screen.look}|${room}|${dialog.korri}|${dialog.title !== ""}|${labels.join("\u0000")}|${dialog.headline}|${dialog.lines.join("\u0000")}`
  let plan = planned.get(key)
  if (plan === undefined) {
    if (planned.size > 32) planned.clear()
    plan = planWords(dialog, labels, room, screen)
    planned.set(key, plan)
  }
  return plan
}

/** Lines `count` long, 1.4 caps apart, at a cap of `cap` px (ZoneCaps' drawn height). */
const drawnHeight = (count: number, cap: number): number => (count <= 0 ? 0 : (count - 1) * 1.4 * cap + cap)

function planWords(dialog: ZoneDialogShape, labels: readonly string[], room: number, screen: ZoneScreen): ZoneDialogWords {
  const a = screen.a
  const inner = dialogInnerWidth(screen)
  // The headline is the dialog's name: caps-2, then caps-1 broken between words, never cut.
  const face = dialog.korri ? "korri" : "caps"
  const big = 2 * CAP * a
  const wide = measureWords(dialog.headline, big, face, screen.look) > inner
  const head = wide ? wrapWords(dialog.headline, inner, CAP * a, face, screen.look) : [dialog.headline]
  const headCap = wide ? CAP * a : big
  const text = dialog.lines.filter(line => line !== "").join("\n")
  if (text === "") return { head, headCap, lines: [], shown: 0 }
  const all = wrapWords(text, inner, CAP * a, "fact", screen.look)
  // Everything but the lines: the pads, the headline, the gaps, the game, the buttons.
  const fixed = 2 * PAD * a + drawnHeight(head.length, headCap) + 2 * GAP * a + (dialog.title !== "" ? CAP * a + GAP * a : 0) + dialogButtons(labels, screen).needed
  const fit = Math.max(1, Math.floor((room - fixed - CAP * a) / (PITCH * a)) + 1)
  if (all.length <= fit) return { head, headCap, lines: all, shown: all.length }
  // Too many: the lines narrow to keep room for the triangles on either side.
  const lines = wrapWords(text, Math.max(CAP * a, inner - 2 * ZONE_DIALOG_MARK_ROOM * a), CAP * a, "fact", screen.look)
  return { head, headCap, lines, shown: Math.min(fit, lines.length) }
}

/** The words a window of lines is for: a dialog with other words shows its lines from the first. */
export const dialogKey = (dialog: { readonly headline: string; readonly lines: readonly string[] }): string =>
  `${dialog.headline}\n${dialog.lines.join("\n")}`

/** The first line the dialog can start on: the window's last place. */
export const lastFirst = (words: ZoneDialogWords): number => Math.max(0, words.lines.length - words.shown)

/** The window of lines from `first` (brought inside), with whether more stand above and below it. */
export function dialogWindow(words: ZoneDialogWords, first: number): { readonly lines: string[]; readonly above: boolean; readonly below: boolean } {
  const from = Math.min(Math.max(0, first), lastFirst(words))
  return {
    lines: words.lines.slice(from, from + words.shown),
    above: from > 0,
    below: from + words.shown < words.lines.length,
  }
}
