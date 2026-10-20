/**
 * The shell's top strip as a pure function of its width.
 *
 * The strip holds three things in one row: the B hint, the tabs, the A hint.
 * When the row is too narrow for them, it yields in the intrinsic order,
 * space first, then readouts, never controls:
 *
 *  1. `inline`  everything at its plan size: words on the hints, full gaps.
 *  2. `tight`   the gaps, the tabs' padding and the type shrink to their
 *               declared least (`--zone-strip-gap-least`,
 *               `--zone-games-tab-pad-y-least`, `--zone-type-tab-least`,
 *               `--zone-type-hint-least`).
 *  3. `keys`    the hints drop their words and keep the round keycap. The
 *               word stays as the hint's accessible name.
 *  4. `stacked` two rows: the hints in the corners of the first, the tabs
 *               across the whole of the second. The hints take their words
 *               back while the row has room for both
 *               (`ZONE_TOP_STACK_WORDS_WIDTH`); tabs that do not fit
 *               their row wrap onto a second one, whole.
 *
 * No tab and no hint is ever hidden, shrunk or cut. Every threshold is in
 * art px (src/plan/art-px.ts): the shell divides its width by its art
 * pixel first. Measured in Chromium (Zone Caps, caps-1 tabs and hints, the
 * caps-2 banner, GAMES, OPTIONS and SAVE DATA), with the hints' columns
 * free to differ so neither hint pays for the other's word: one row needs
 * 472 art px at plan size, 414 once the gaps and the tabs' padding are at
 * their least (a tab's panel leans by its height, so a shorter panel is
 * narrower), and 307 with the keycaps alone; the two hints with their
 * words need 147. So 1280x720 (426 art px) keeps the words in one row,
 * 640x480 (320) keeps one row with the keycaps, and 480x800 (240) takes
 * two rows with the words, its tabs wrapping onto a second row of their
 * own before any label is cut. Each threshold keeps a little room; a
 * longer word in a tab needs them measured again.
 *
 * The shell measures its own box and writes the answer as `data-top`; its
 * stylesheet follows the attribute. The plan never reads CSS.
 */

/** At and above this width in art px the strip takes its plan size. */
export const ZONE_TOP_INLINE_WIDTH = 476
/** At and above this width (and under inline) the gaps and type take their least. */
export const ZONE_TOP_TIGHT_WIDTH = 420
/** At and above this width (and under tight) the hints keep only their keycaps. */
export const ZONE_TOP_KEYS_WIDTH = 312
/** In two rows, the hints keep their words at and above this width. */
export const ZONE_TOP_STACK_WORDS_WIDTH = 152

export type ZoneTopLevel = "inline" | "tight" | "keys" | "stacked"

export interface ZoneTopStripPlan {
  readonly level: ZoneTopLevel
  /** One row (hint, tabs, hint) or two (hints, then tabs). */
  readonly rows: 1 | 2
  /** Whether the hints show their words beside the keycap. */
  readonly words: boolean
  /** Whether the gaps and the type are at their declared least. */
  readonly least: boolean
}

export function planTopStrip(width: number): ZoneTopStripPlan {
  if (width >= ZONE_TOP_INLINE_WIDTH) return { level: "inline", rows: 1, words: true, least: false }
  if (width >= ZONE_TOP_TIGHT_WIDTH) return { level: "tight", rows: 1, words: true, least: true }
  if (width >= ZONE_TOP_KEYS_WIDTH) return { level: "keys", rows: 1, words: false, least: true }
  return { level: "stacked", rows: 2, words: width >= ZONE_TOP_STACK_WORDS_WIDTH, least: true }
}

/**
 * The strip's answer as the one attribute value its stylesheet reads. A
 * stacked strip whose hints have no room for words is `stacked-keys`.
 */
export function topStripAttribute(plan: ZoneTopStripPlan): string {
  return plan.level === "stacked" && !plan.words ? "stacked-keys" : plan.level
}
