/**
 * The shell's top strip as a pure function of its width.
 *
 * The strip holds three things in one row: the B hint, the tabs, the A hint.
 * When the row is too narrow for them, it yields in the intrinsic order,
 * space first, then readouts, never controls:
 *
 *  1. `inline`  everything at its plan size: words on the hints, full gaps.
 *  2. `tight`   the gaps and the type shrink to their declared least
 *               (`--zone-strip-gap-least`, `--zone-type-tab-least`,
 *               `--zone-type-hint-least`).
 *  3. `keys`    the hints drop their words and keep the round keycap. The
 *               word stays as the hint's accessible name.
 *  4. `stacked` two rows: the hints in the corners of the first, the tabs
 *               across the whole of the second. The hints take their words
 *               back while the row has room for both
 *               (`ZONE_TOP_STACK_WORDS_WIDTH`), and the tabs shrink to fit
 *               the row rather than lose one.
 *
 * No tab and no hint is ever hidden. Every threshold was measured at the
 * narrow type (tabs 17 px, hints 15 px) with GAMES, OPTIONS and SAVE DATA
 * and their sub-lines: the row needs 592 px at plan size, about 500 at the
 * least type and about 375 with the keycaps alone. Each threshold keeps a
 * little room over that. They are guesses until measured on a real screen
 * with a longer word in a tab.
 *
 * The shell measures its own box and writes the answer as `data-top`; its
 * stylesheet follows the attribute. The plan never reads CSS.
 */

/** At and above this width the strip takes its plan size. */
export const ZONE_TOP_INLINE_WIDTH = 620
/** At and above this width (and under inline) the gaps and type take their least. */
export const ZONE_TOP_TIGHT_WIDTH = 530
/** At and above this width (and under tight) the hints keep only their keycaps. */
export const ZONE_TOP_KEYS_WIDTH = 400
/** In two rows, the hints keep their words at and above this width. */
export const ZONE_TOP_STACK_WORDS_WIDTH = 240

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
