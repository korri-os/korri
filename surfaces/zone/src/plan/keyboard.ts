/**
 * The slanted keyboard's layout as a pure function of the box it may take,
 * in art px (src/plan/art-px.ts), and FIND's split of the GAMES body
 * between the band and the keys.
 *
 * A key never loses a letter and the keys never scroll: the plan gives
 * every key the height and width its label needs and counts the lines the
 * rows wrap to, so the keyboard's height is known before it is drawn.
 * What yields, in order (intrinsic-design rule 6, brief section 5: never
 * shrink a control to fit):
 *
 *  1. the band above the keys, down to its least (`ZONE_FIND_BAND_LEAST`);
 *  2. the keyboard's spacing: its padding, gaps, side margins, the
 *     panel's lean and the slab's room round its line (`full`, then
 *     `tight`);
 *  3. the band, to hidden while typing (it shows the result after DONE);
 *  4. the keys' height (20, then 18, then 16 art px: a caps-1 label with
 *     its drop and a Jersey 10 descender still stand whole in 16). Key type
 *     is caps-1 at every size already, the least Zone Caps step.
 *
 * Only a box smaller than every step holds (under about 120 art rows) lets
 * the panel scroll, as a last resort; `fits` says so.
 *
 * Every constant is a token in zone-tokens.css as well, held equal by
 * test/plan-keyboard.test.ts.
 */

/** The character rows of each layer. Digits head every layer. */
export const ZONE_KEYBOARD_ROWS = {
  letters: ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"],
  shift: ["1234567890", "QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"],
  symbols: ["1234567890", "!@#$%^&*()", "-_=+[]{}\\|", ";:'\",.<>/?~`"],
  uri: ["1234567890", "qwertyuiop:/", "asdfghjkl.-_", "zxcvbnm?=&@"],
} as const

export type ZoneKeyboardLayerName = keyof typeof ZONE_KEYBOARD_ROWS

/**
 * The most keys each character row holds over every layer. The plan lays
 * out the widest so a layer change never moves the panel's height.
 */
export const ZONE_KEYBOARD_ROW_KEYS: readonly number[] = [0, 1, 2, 3].map(row =>
  Math.max(...Object.values(ZONE_KEYBOARD_ROWS).map(rows => [...(rows[row] ?? "")].length)))

/** The widest row is shared out in this many key units (12 in the uri and symbols layers). */
export const ZONE_KEYBOARD_UNITS = Math.max(...ZONE_KEYBOARD_ROW_KEYS)

/**
 * The control row's keys in order, each its least width in art px: its
 * label's advance (Zone Caps at caps-1, 7 a letter; `abc` in Jersey 10,
 * one Jersey pixel an art pixel) and `ZONE_KEY_LABEL_SIDE` either side for
 * the key's slant, so the label stands inside its own key. The width a key
 * prefers is `units` key units and the gaps between them.
 */
export const ZONE_KEY_LABEL_SIDE = 4
export const ZONE_KEYBOARD_CONTROLS = [
  { id: "layer:letters", units: 1, label: 17 },
  { id: "layer:shift", units: 1, label: 21 },
  { id: "layer:symbols", units: 1, label: 17 },
  { id: "layer:uri", units: 1, label: 17 },
  { id: "space", units: 4, label: 35 },
  { id: "delete", units: 2, label: 42 },
  { id: "done", units: 2, label: 28 },
] as const

/** A key never narrower than this; a row wraps instead (`--zone-key-min`): a W and its drop with the slant's room. */
export const ZONE_KEY_UNIT_LEAST = 16
/** A key's unit is never wider than its height by this much (the old clamp's 1.8). */
export const ZONE_KEY_UNIT_MOST_SHARE = 1.8
/** The keys' heights, tallest first: the last thing to yield. */
export const ZONE_KEY_HEIGHTS = [20, 18, 16] as const
/** The widest the FIND keyboard grows (`--zone-games-keys-max`). */
export const ZONE_FIND_KEYS_MOST = 450
/** The least band that still shows a card while typing (`--zone-games-band-min`). */
export const ZONE_FIND_BAND_LEAST = 60

/**
 * The keyboard's spacing, generous then tight. `pad` is the panel's room
 * inside its edge and between the slab and the keys; `gap` between keys
 * and between lines; `slab` the typed slab's height (a Jersey line is 14);
 * `margin` the room either side of the panel in its holder; `inset` under
 * it; `slant` the panel's own lean either side of the keys.
 */
export const ZONE_KEYBOARD_SPACING = {
  full: { pad: 7, gap: 3, slab: 24, margin: 12, inset: 4, slant: 14 },
  tight: { pad: 4, gap: 2, slab: 18, margin: 2, inset: 2, slant: 7 },
} as const

export type ZoneKeyboardSpacing = keyof typeof ZONE_KEYBOARD_SPACING

export interface ZoneKeyboardPlan {
  readonly spacing: ZoneKeyboardSpacing
  readonly pad: number
  readonly gap: number
  readonly slab: number
  readonly margin: number
  readonly inset: number
  readonly slant: number
  readonly keyH: number
  /** One key unit's width, a whole art px. */
  readonly unit: number
  /** Keys on one line of a character row. */
  readonly perLine: number
  /** The control row at its preferred widths on one line, or at its least widths, wrapping. */
  readonly controls: "preferred" | "least"
  /** Lines of keys: the character rows' and the control row's. */
  readonly lines: number
  /** The panel's width and the height it takes with its inset, in art px. */
  readonly width: number
  readonly height: number
}

const controlLines = (widths: readonly number[], room: number, gap: number): number => {
  let lines = 1
  let used = 0
  for (const width of widths) {
    if (used > 0 && used + gap + width > room) {
      lines += 1
      used = width
    } else {
      used += (used > 0 ? gap : 0) + width
    }
  }
  return lines
}

/** The keyboard at one spacing and key height, in a box `wArt` wide (the FIND keyboard's holder: margins either side, the panel no wider than its widest). */
export function zoneKeyboardAt(wArt: number, spacing: ZoneKeyboardSpacing, keyH: number): ZoneKeyboardPlan {
  const { margin } = ZONE_KEYBOARD_SPACING[spacing]
  return zoneKeyboardPanel(Math.max(0, Math.min(wArt - 2 * margin, ZONE_FIND_KEYS_MOST)), spacing, keyH)
}

/** The keyboard at one spacing and key height, its panel `width` art px wide. */
export function zoneKeyboardPanel(width: number, spacing: ZoneKeyboardSpacing, keyH: number): ZoneKeyboardPlan {
  const { pad, gap, slab, margin, inset, slant } = ZONE_KEYBOARD_SPACING[spacing]
  const inner = Math.max(0, width - 2 * (pad + slant))
  const share = Math.floor((inner - (ZONE_KEYBOARD_UNITS - 1) * gap) / ZONE_KEYBOARD_UNITS)
  const unit = Math.max(ZONE_KEY_UNIT_LEAST, Math.min(Math.floor(keyH * ZONE_KEY_UNIT_MOST_SHARE), share))
  const perLine = Math.max(1, Math.floor((inner + gap) / (unit + gap)))
  const charLines = ZONE_KEYBOARD_ROW_KEYS.reduce((sum, keys) => sum + Math.ceil(keys / perLine), 0)
  const least = ZONE_KEYBOARD_CONTROLS.map(key => key.label + 2 * ZONE_KEY_LABEL_SIDE)
  const preferred = ZONE_KEYBOARD_CONTROLS.map((key, index) =>
    Math.max(least[index] ?? 0, key.units * unit + (key.units - 1) * gap))
  const oneLine = (widths: readonly number[]) => widths.reduce((sum, w) => sum + w, 0) + (widths.length - 1) * gap
  const controls = oneLine(preferred) <= inner ? "preferred" : "least"
  const lines = charLines + (controls === "preferred" ? 1 : controlLines(least, inner, gap))
  const height = pad + slab + pad + lines * keyH + (lines - 1) * gap + pad + inset
  return { spacing, pad, gap, slab, margin, inset, slant, keyH, unit, perLine, controls, lines, width, height }
}

/**
 * Every keyboard step in yield order, generous first: the spacing tightens
 * before the keys get shorter. A keyboard whose holder does not plan it
 * (OPTIONS', SAVE DATA's, first boot's) takes the first step whose panel
 * its holder's room holds (useZoneKeyboardFit).
 */
export const ZONE_KEYBOARD_STEPS: readonly (readonly [ZoneKeyboardSpacing, number])[] =
  ZONE_KEY_HEIGHTS.flatMap(keyH => (["full", "tight"] as const).map(spacing => [spacing, keyH] as const))

export interface ZoneFindPlan {
  /** The band's height while typing, in art px; 0 when it is hidden. */
  readonly band: number
  readonly keys: ZoneKeyboardPlan
  /** False only when even the least keyboard is taller than the box: the panel scrolls. */
  readonly fits: boolean
}

/**
 * FIND's split of the box under the tabs (the GAMES body, or the screen
 * below the tabs when the keyboard rises over the foot): the band keeps
 * what the keys leave while that is at least its least; then the
 * keyboard's spacing tightens; then the band hides; then the keys get
 * shorter. With `band` false (a box with no band in it) the plan starts at
 * the third step.
 */
export function planZoneFind(wArt: number, hArt: number, band = true): ZoneFindPlan {
  const [tallest] = ZONE_KEY_HEIGHTS
  if (band) {
    for (const spacing of ["full", "tight"] as const) {
      const keys = zoneKeyboardAt(wArt, spacing, tallest)
      if (hArt - keys.height >= ZONE_FIND_BAND_LEAST) return { band: hArt - keys.height, keys, fits: true }
    }
  }
  for (const [spacing, keyH] of ZONE_KEYBOARD_STEPS) {
    const keys = zoneKeyboardAt(wArt, spacing, keyH)
    if (keys.height <= hArt) return { band: 0, keys, fits: true }
  }
  return { band: 0, keys: zoneKeyboardAt(wArt, "tight", ZONE_KEY_HEIGHTS[ZONE_KEY_HEIGHTS.length - 1] ?? tallest), fits: false }
}
