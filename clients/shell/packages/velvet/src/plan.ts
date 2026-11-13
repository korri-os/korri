/**
 * Velvet's layout: one pure function of the window's width and height
 * (contract amendment A2, the intrinsic-design method). Every threshold is a
 * named constant here; the focus graph (src/focus.ts, src/browse.ts) and the
 * view both read the plan it returns, and main.slint draws from it. Nothing
 * else decides structure: not a display, not a device, not `.slint`.
 *
 * The numbers for the home, the lobby, the set and the detail come from the
 * approved mockup's plan(); the list pages, the keyboard and the curtain are
 * Velvet's own.
 *
 * Yield order (intrinsic-design rule 6): pads and gaps scale with the window
 * down to their floors, then type falls to its floor, then a readout (the
 * clock, the "now" line) leaves; a control never does. Space and type yield at
 * different rates (owner, 2026-11-13, #423): spacing follows `k`, the smaller
 * axis; type follows `kt`, which shrinks more slowly (the mockup's round-5
 * plan), so type does not reach its floor while the stage still has room.
 */
import { clamp } from "./colour"
import { onForget } from "./memo"

/** Below this width / height the compositions stack. */
export const NARROW_RATIO = 0.9
/** The empty library stands side by side only from this ratio up. */
export const EMPTY_WIDE_RATIO = 1.5
/** How much a focused tile grows. Neighbours never move. */
export const FOCUS_SCALE = 1.08
/** A stored hero's usual shape: the room a narrow window keeps for its band. */
export const HERO_RATIO = 3.1
/** Every tile is a 2:3 portrait. */
export const TILE_RATIO = 2 / 3
/** Games in the row under the pick. */
export const ROW_SLOTS = 4
/** Games a lobby shelf shows before "See all": two shelves of posters are what the pixel budget holds. */
export const SHELF_SLOTS = 5
/** The most tiles a set shows across: two rows of posters are what the pixel budget holds (src/art.ts). */
export const MOST_COLUMNS = 5
/** A list page stands beside its panel only when the window is this wide for its height. */
export const PANEL_BESIDE_RATIO = 1.25
/** Type never grows past this many times the smaller axis's scale: a thin strip cannot set type its width cannot hold. */
export const TYPE_REACH = 2.5
/** The keyboard's grid. */
export const KEY_COLUMNS = 10
export const KEY_ROWS = 5

export interface Plan {
  readonly width: number
  readonly height: number
  /** The space scale's factor. */
  readonly k: number
  /** The type scale's factor: `k` at or above the reference stage, larger below it. */
  readonly kt: number
  readonly padX: number
  readonly padY: number
  readonly narrow: boolean
  // ---- type, px ----
  readonly title: number
  readonly head: number
  readonly body: number
  readonly fact: number
  readonly label: number
  readonly button: number
  readonly small: number
  // ---- home: the pick ----
  /** The pick's card when no hero lights the stage. */
  readonly posterX: number
  readonly posterY: number
  readonly posterH: number
  /** The text beside (or under) that card. */
  readonly textX: number
  readonly textY: number
  readonly textW: number
  readonly textH: number
  /** The marquee when the hero is the stage. */
  readonly heroX: number
  readonly heroY: number
  readonly heroW: number
  readonly heroH: number
  /** The pick's logo box and title size. */
  readonly markH: number
  readonly pickTitle: number
  // ---- home: the row under the pick ----
  /** At rest: a strip of small tiles. */
  readonly stripY: number
  readonly stripH: number
  readonly stripGap: number
  /** Focused: a shelf. */
  readonly rowY: number
  readonly rowH: number
  readonly rowGap: number
  /** The focused tile's marquee over the shelf. */
  readonly rowMarkY: number
  readonly rowMarkH: number
  readonly rowMarkW: number
  /** Where the library shows under the shelf; 0 when there is no room. */
  readonly peekY: number
  // ---- the library ----
  readonly headY: number
  readonly headH: number
  readonly libTop: number
  /** A shelf: its label's line, where its tiles start under it, its tile and its pitch. */
  readonly shelfLabel: number
  readonly shelfRowY: number
  readonly shelfH: number
  readonly shelfGap: number
  readonly shelfPitch: number
  readonly halo: number
  // ---- a set ----
  readonly columns: number
  readonly tileW: number
  readonly tileH: number
  readonly gridGap: number
  readonly gridInset: number
  /** Whole rows the set's window shows. */
  readonly gridRows: number
  /** Rows the view sends: the shown ones and one more each way. */
  readonly poolRows: number
  readonly sheetW: number
  // ---- the detail ----
  readonly detailLogoH: number
  readonly detailLogoW: number
  readonly detailTitle: number
  readonly detailFact: number
  readonly detailY: number
  readonly detailW: number
  /** Buttons one under the other. */
  readonly stack: boolean
  // ---- list pages ----
  readonly pageW: number
  readonly rowHeight: number
  /** 0: no room beside, the panel stands over the rows. 1: beside. */
  readonly panelBeside: boolean
  readonly panelW: number
  // ---- the keyboard ----
  readonly keyW: number
  readonly keyH: number
  readonly keyGap: number
  readonly keysY: number
  // ---- the curtain (status and questions) ----
  readonly curtainW: number
  readonly kicker: number
  // ---- the empty library ----
  readonly emptyWide: boolean
  // ---- the gameplay overlay ----
  readonly pauseW: number
}

export const DEFAULT_SIZE = { width: 1280, height: 720 } as const

const gapOf = (tile: number): number => Math.round(Math.max(10, tile * TILE_RATIO * 0.16))
const growOf = (tile: number): number => Math.ceil(tile * (FOCUS_SCALE - 1))

// The plan of the size asked for last: a window keeps its size for a long time.
let last: Plan | undefined
onForget(() => {
  last = undefined
})

export function plan(width: number, height: number): Plan {
  if (last !== undefined && last.width === width && last.height === height) return last
  const W = Math.max(1, width)
  const H = Math.max(1, height)
  const k = clamp(Math.min(W / 1600, H / 900), 0.4, 1.25)
  // Below the 1600 x 900 reference, type follows the fourth root of the area's
  // ratio: it shrinks slowly, and a tall or a wide window pays for it with its
  // plentiful axis. 933 x 704: k 0.58, kt 0.82 (facts 17 px, not 13).
  const kRaw = Math.min(W / 1600, H / 900)
  const kt = k >= 1 ? k : clamp(Math.min(Math.pow((W / 1600) * (H / 900), 0.25), TYPE_REACH * kRaw), Math.max(k, 0.4), 1)
  // Space: pads, gaps and room. `ft`: type.
  const fs = (px: number, min: number) => Math.max(min, Math.round(px * k))
  const ft = (px: number, min: number) => Math.max(min, Math.round(px * kt))
  const padX = Math.round(clamp(W * 0.05, 18, 110))
  const padY = Math.round(clamp(H * 0.055, 14, 80))
  const narrow = W / H < NARROW_RATIO
  const R = TILE_RATIO
  const fact = ft(21, 13)
  const label = ft(22, 13)

  // Home. The card and the strip under it are about 3 to 1 (owner, 2026-11-13, on usu: the
  // prototype's 6 to 1 made the card loom and the games under it too small to read).
  const stripH = Math.round(clamp(H * 0.17, 56, 200))
  const stripY = H - padY - stripH
  let posterH: number, posterX: number, posterY: number, gap: number
  let textX: number, textY: number, textW: number, textH: number
  let markH: number, pickTitle: number, rowH: number, rowY: number, rowMarkW: number
  if (!narrow) {
    posterH = Math.round(Math.min(H * 0.52, stripY - padY * 1.6, (W * 0.3) / R))
    posterX = padX
    posterY = Math.round(Math.max(padY, (stripY - posterH) * 0.45))
    gap = Math.round(Math.max(24, posterH * R * 0.16))
    textX = padX + Math.round(posterH * R) + gap
    textY = posterY
    textW = W - textX - padX
    textH = posterH
    markH = Math.round(clamp(posterH * 0.42, 60, 330))
    pickTitle = Math.round(clamp(posterH * 0.17, 30, 150))
    rowH = Math.round(clamp(H * 0.32, 100, 380))
    rowY = Math.round(H * 0.46)
    rowMarkW = Math.round(W * 0.6)
  } else {
    posterH = Math.round(Math.min(H * 0.36, ((W - 2 * padX) * 0.52) / R))
    posterX = Math.round((W - posterH * R) / 2)
    posterY = Math.round(padY * 1.3)
    gap = fs(24, 14)
    textX = padX
    textY = posterY + posterH + gap
    textW = W - 2 * padX
    textH = Math.max(0, Math.round(stripY - textY - fs(20, 10)))
    markH = Math.round(clamp(H * 0.1, 44, 120))
    pickTitle = ft(104, 30)
    rowH = Math.round(clamp((W - 2 * padX) / 2.3 / R, 100, 360))
    rowY = Math.round(H * 0.47)
    rowMarkW = W - 2 * padX
  }
  // The shelf never leaves the window: on a short one it yields height first.
  rowH = Math.min(rowH, Math.max(60, Math.round((H - padY - rowY) / FOCUS_SCALE)))
  const rowGap = gapOf(rowH)
  // The focused tile grows up by half its growth: the facts stay clear of it.
  const rowMarkH = Math.max(0, rowY - padY - Math.round(Math.max(fs(40, 16), (rowH * (FOCUS_SCALE - 1)) / 2 + fs(18, 12))))
  const peek = rowY + Math.round(rowH * FOCUS_SCALE) + Math.round(rowGap * 2)
  // The marquee widens as type outgrows space (kt over k), so the facts keep one line.
  const heroW = narrow ? W - 2 * padX : Math.round(W * Math.min(0.8, (0.56 * kt) / k))
  // A narrow window stacks: the hero's band (about 3.1 to 1) across the top, the words under it.
  const heroY = narrow ? Math.round(W / HERO_RATIO + padY * 0.6) : posterY
  const heroH = narrow ? Math.max(0, Math.round(stripY - padY - heroY)) : posterH

  // The library.
  const head = ft(48, 22)
  const headH = Math.round(head * 1.2)
  const libTop = padY + headH + fs(26, 10) + (narrow ? Math.round(fact * 1.7) : 0)
  const shelfLabel = Math.round(label * 1.75)
  const shelfH = Math.round(clamp(Math.min((H - libTop - padY) / 1.5 - shelfLabel, H * 0.3, (W - 2 * padX) / (narrow ? 2.3 : 5.2) / R), 84, 300))
  const shelfGap = gapOf(shelfH)
  const halo = fs(14, 8)
  const shelfRowY = shelfLabel + growOf(shelfH) + fs(10, 5)
  const shelfPitch = shelfRowY + shelfH + Math.round(shelfGap * 1.2)

  // A set.
  const wanted = clamp(Math.min(H * 0.34, (W * 0.24) / R), 96, 330)
  const gridGap = gapOf(wanted)
  const columns = Math.min(MOST_COLUMNS, Math.max(2, Math.floor((W - 2 * padX + gridGap) / (wanted * R + gridGap))))
  const tileW = Math.floor((W - 2 * padX - (columns - 1) * gridGap) / columns)
  const tileH = Math.round(tileW / R)
  const gridInset = growOf(tileH) + halo
  const gridRows = Math.max(1, Math.floor((H - libTop - gridInset + gridGap) / (tileH + gridGap)))
  const poolRows = Math.ceil((H - libTop) / (tileH + gridGap)) + 2

  // The detail.
  const detailLogoH = narrow ? Math.round(clamp(H * 0.12, 48, 140) * 1.25) : Math.round(clamp(H * 0.24, 70, 280) * 1.25)
  const detailW = narrow ? W - 2 * padX : Math.round(W * 0.5)
  const detailTitle = Math.round(clamp((detailLogoH / 1.25) * 0.46, 28, 120) * 1.2)

  // List pages.
  const rowHeight = Math.round(clamp(H * 0.078, 30, 72))
  const panelBeside = W / H >= PANEL_BESIDE_RATIO && W >= 560
  const pageW = panelBeside ? Math.round(Math.min((W - 2 * padX) * 0.58, 980)) : Math.min(W - 2 * padX, 980)
  const panelW = panelBeside ? W - 2 * padX - pageW - Math.round(padX * 0.8) : W - 2 * padX

  // The keyboard: five rows of ten, in the lower part of the window.
  const keyGap = Math.max(2, Math.round(4 * k))
  const keyW = Math.floor((Math.min(W - 2 * padX, 1100) - (KEY_COLUMNS - 1) * keyGap) / KEY_COLUMNS)
  const keyH = Math.floor(Math.min(keyW * 0.92, (H * 0.56 - (KEY_ROWS - 1) * keyGap) / KEY_ROWS, 78))
  const keysY = H - Math.round(padY * 0.8) - KEY_ROWS * keyH - (KEY_ROWS - 1) * keyGap

  last = {
    width: W, height: H, k, kt, padX, padY, narrow,
    title: ft(104, narrow ? 30 : 28), head, body: ft(24, 14), fact, label, button: ft(28, 15), small: ft(18, 12),
    posterX, posterY, posterH, textX, textY, textW, textH,
    heroX: padX, heroY, heroW, heroH, markH, pickTitle,
    stripY, stripH, stripGap: gapOf(stripH),
    rowY, rowH, rowGap, rowMarkY: padY, rowMarkH, rowMarkW,
    peekY: peek < H - 24 ? peek : 0,
    headY: padY, headH, libTop, shelfLabel, shelfRowY, shelfH, shelfGap, shelfPitch, halo,
    columns, tileW, tileH, gridGap, gridInset, gridRows, poolRows,
    sheetW: Math.round(Math.min(W, clamp(W * 0.3, 260, 420))),
    detailLogoH, detailLogoW: narrow ? Math.round(W * 0.9) - padX : Math.round(W * 0.5), detailTitle, detailFact: ft(19, 12),
    detailY: narrow ? Math.round(H * 0.3) : Math.round(H * 0.34), detailW,
    stack: narrow,
    pageW, rowHeight, panelBeside, panelW,
    keyW, keyH, keyGap, keysY,
    curtainW: Math.round(Math.min(W - 2 * padX, clamp(W * 0.6, 280, 980))),
    kicker: ft(72, 26),
    emptyWide: W / H >= EMPTY_WIDE_RATIO,
    pauseW: Math.round(Math.min(W, clamp(W * 0.42, 280, 640))),
  }
  return last
}
