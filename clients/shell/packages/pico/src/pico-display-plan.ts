import type { Entry, ExitChoices } from "./native/focus"

// Pico draws one screen. Its two-display split read the display facts korrid
// no longer publishes (ADR 0086): a surface now declares panes, and korrid
// places them. Pico has not been moved to panes yet.

/** The surface window's size in logical pixels, from the shell's `Resized` (contract A2, 8.1). */
export interface PicoWindow {
  readonly width: number
  readonly height: number
}

/** Until the first `Resized` (the shell sends one before the first view). */
export const PICO_WINDOW_BEFORE_RESIZED: PicoWindow = { width: 640, height: 480 }

/** Spines stand beside the card when the screen is this wide, in virtual pixels. */
export const PICO_SPINES_BESIDE = 400

/**
 * Pico's one layout plan (contract A2, 8.2): a pure function of the window's
 * width and height. The focus graph and `view()` both read it, and
 * main.slint draws what it decides, so a press always moves to what is drawn
 * in its direction.
 */
export interface PicoLayoutPlan {
  /** The virtual pixel, in logical pixels: as `Tokens.p` in ui/tokens.slint. */
  readonly p: number
  /** The screen that chooses, in virtual pixels. */
  readonly width: number
  readonly height: number
  /** Carts to a row in the home grid. */
  readonly columns: number
  /** Settings' spines stand beside the card, not above it as tabs. */
  readonly spinesBeside: boolean
  /** A game's screen puts its cart over its facts, not beside them. */
  readonly detailStacked: boolean
  /** Where the cursor goes on leaving each region (contract 8.4). */
  readonly exits: ExitChoices
}

export function picoLayoutPlan(window: PicoWindow): PicoLayoutPlan {
  // As Tokens.fit: the virtual pixel from the whole window.
  const p = Math.max(2, Math.floor(Math.min(window.width, window.height) / 360))
  const width = Math.floor(window.width / p)
  const height = Math.floor(window.height / p)
  // The screen shell's body: 8 virtual pixels in from each side.
  const body = width - 16
  return {
    p,
    width,
    height,
    // 30-pixel slots, 4 apart.
    columns: Math.max(1, Math.floor((body + 4) / 34)),
    spinesBeside: width >= PICO_SPINES_BESIDE,
    detailStacked: body < 240,
    exits: picoExits(width >= PICO_SPINES_BESIDE),
  }
}

const to = (region: string, entry: Entry) => ({ to: region, entry })

/**
 * Every region Pico draws, and its exits (contract 8.4). Inside a region the
 * nearest target ahead wins; at its edge the cursor leaves by the first
 * exit listed for that direction whose region the screen has. A direction
 * with none stays.
 *
 * - `floor`: MENU on home, RELOAD on launch settings, under everything.
 * - `menu`: MENU's list. `invites`: PLAYING NOW's buttons, at the top.
 * - `shelf`, `grid`, `hero` and `resume`: home's three ways to show the
 *   library. `card`: a notice's or question's answers. `stage`: the
 *   buttons of a launch or a download under way.
 * - Find: `sections` over `orders` over the `keys`, then the `results`.
 * - A panel: `spines` and `rows`. Beside, the spines are a column left of
 *   the rows; above, they are tabs. Launch settings have no spines: their
 *   one list of `rows`, then the `keep` row under it while a row is changed.
 * - `keys` over `answers` in the editor; setup's `list`, `pack` answers,
 *   `answers` and `go`, top to bottom; the store's `search` over its
 *   `results`; a game's `play` (its first answer) over its other
 *   `actions`, over its `card`.
 */
export function picoExits(spinesBeside: boolean): ExitChoices {
  const spines = spinesBeside
    ? { right: [to("rows", "first")] }
    : { down: [to("rows", "first")] }
  const rows = spinesBeside
    ? { left: [to("spines", "active")], down: [to("keep", "first"), to("floor", "first")] }
    : { up: [to("spines", "active")], down: [to("keep", "first"), to("floor", "first")] }
  return {
    floor: {
      up: [
        to("menu", "remembered"), to("shelf", "active"), to("grid", "remembered"), to("resume", "remembered"),
        to("hero", "first"), to("card", "remembered"), to("keep", "first"), to("rows", "remembered"), to("invites", "remembered"),
      ],
    },
    menu: { down: [to("floor", "first")] },
    invites: {
      down: [
        to("shelf", "active"), to("grid", "remembered"), to("hero", "first"), to("resume", "first"),
        to("card", "first"), to("floor", "first"),
      ],
    },
    shelf: { up: [to("invites", "remembered")], down: [to("floor", "first")] },
    grid: { up: [to("invites", "remembered")], down: [to("floor", "first")] },
    hero: { up: [to("invites", "remembered")], down: [to("resume", "first"), to("floor", "first")] },
    resume: { up: [to("hero", "first"), to("invites", "remembered")], down: [to("floor", "first")] },
    card: {
      up: [to("actions", "remembered"), to("play", "first"), to("search", "first"), to("invites", "remembered")],
      down: [to("floor", "first")],
    },
    stage: { down: [to("floor", "first")] },
    sections: { down: [to("orders", "active")] },
    orders: { up: [to("sections", "active")], down: [to("keys", "first")] },
    keys: { up: [to("orders", "active")], down: [to("results", "remembered"), to("answers", "first")] },
    results: { up: [to("keys", "remembered"), to("search", "first")] },
    search: { down: [to("results", "first"), to("card", "first")] },
    play: { down: [to("actions", "first"), to("card", "first")] },
    actions: { up: [to("play", "first")], down: [to("card", "first")] },
    spines,
    rows,
    keep: { up: [to("rows", "remembered")], down: [to("floor", "first")] },
    answers: { up: [to("keys", "remembered"), to("pack", "remembered"), to("list", "remembered")], down: [to("go", "first")] },
    list: { down: [to("pack", "first"), to("answers", "first"), to("go", "first")] },
    pack: { up: [to("list", "remembered")], down: [to("answers", "first"), to("go", "first")] },
    go: { up: [to("answers", "remembered"), to("pack", "remembered"), to("list", "remembered")] },
  }
}
