/**
 * Where the player is in Pico's catalog, as one value.
 *
 * The shape follows what the screen draws, bottom to top:
 *
 *     settings   drawn over everything
 *     detail     a game's own screen, drawn over Find so the query survives
 *     find
 *     home       always there
 *
 * Each layer is present or absent, and each owns the one question it can be
 * asking. A question cannot outlive the layer that asks it, so Back can never
 * withdraw a question the player cannot see.
 *
 * Korri's SurfaceModel is not stored here. Korri owns it and replaces it
 * whole; `update` receives it beside this value.
 */
import type { SurfaceAction } from "@contracts/surface/korri-surface"
import type { PicoHomeMode } from "../pages/PicoHome"
import { PICO_ALL_SECTIONS, type PicoOrder } from "../pico-library-view"
import type { PicoInitialView } from "../pico-initial-view"
import type { PicoSessionReturn } from "../pico-session-return"
import type { PicoConfirmation } from "../pico-settings-view"
import type { PicoShelfGame } from "../pico-shelf-game"

export interface Home {
  /** How home lays the library out. About this person in this chair: Korri
   * has no opinion on it. */
  readonly mode: PicoHomeMode
  /** The cart the cursor was left on. Home unmounts behind every screen it
   * opens, and coming back should find the same cart. */
  readonly selectedGameId: string | undefined
  readonly menu: "Closed" | "Open"
  /** Where the cursor goes when home appears again: back to MENU after a
   * screen MENU opened, back to the cart after its game's own screen, or
   * wherever the shelf puts it. */
  readonly focusOnReturn: "None" | "Cart" | "MenuKey"
}

/** What Find has typed and chosen. Outlives Find: reopening it keeps them. */
export interface Search {
  readonly query: string
  readonly section: string
  readonly order: PicoOrder
}

export type DetailQuestion =
  | { readonly _tag: "None" }
  /** PLAY on a game Korri can start in more than one place. */
  | { readonly _tag: "ChoosingLocation"; readonly game: PicoShelfGame }
  /** A destructive game action awaiting a yes. Korri's game actions carry no
   * confirmation copy, so the question is built from the label. */
  | { readonly _tag: "ConfirmingAction"; readonly action: SurfaceAction }

/** A game's own screen. An id, not a game, so the screen always shows the
 * copy Korri published last. */
export interface Detail {
  readonly gameId: string
  readonly question: DetailQuestion
}

export type SettingsQuestion =
  | { readonly _tag: "None" }
  /** A destructive setting action Korri asked to be confirmed. */
  | { readonly _tag: "ConfirmingAction"; readonly actionId: string; readonly confirmation: PicoConfirmation }
  /** A text setting open in the editor. */
  | { readonly _tag: "EditingText"; readonly settingId: string }
  /** The editor asking whether to clear its text. */
  | { readonly _tag: "ConfirmingClear"; readonly settingId: string }
  /** The identity dialog, opened by an `identity:` action. */
  | { readonly _tag: "Identity"; readonly actionId: string }

export interface Settings {
  readonly question: SettingsQuestion
}

/** The attract screen. `activity` counts presses and focus moves; the idle
 * timer restarts whenever it changes. */
export interface Idle {
  readonly _tag: "Awake" | "Attracting"
  readonly activity: number
}

export interface PicoNavigation {
  readonly home: Home
  readonly find: boolean
  readonly detail: Detail | undefined
  readonly settings: Settings | undefined
  readonly search: Search
  readonly idle: Idle
  /** Whether a game played from its own screen should, when it ends, return
   * the player to the shelf (pico-session-return.ts). */
  readonly session: PicoSessionReturn
}

export type TopLayer = "Settings" | "Detail" | "Find" | "Home"

/** The layer on top of the stack. Korri's status and the runner picker can
 * still cover it; `shownScreen` decides what is actually drawn. */
export function topLayer(nav: PicoNavigation): TopLayer {
  if (nav.settings !== undefined) return "Settings"
  if (nav.detail !== undefined) return "Detail"
  if (nav.find) return "Find"
  return "Home"
}

const HOME: Home = { mode: "shelf", selectedGameId: undefined, menu: "Closed", focusOnReturn: "None" }

/** Where a fresh Pico starts. `initial` is for previews and tests; hosts omit it. */
export function initialNavigation(initial?: PicoInitialView): PicoNavigation {
  return {
    home: initial?._tag === "Home" ? { ...HOME, mode: initial.mode ?? "shelf" } : HOME,
    find: initial?._tag === "Find",
    detail: initial?._tag === "Detail" ? { gameId: initial.gameId, question: { _tag: "None" } } : undefined,
    settings: initial?._tag === "Settings" ? { question: { _tag: "None" } } : undefined,
    search: {
      query: "",
      section: initial?._tag === "Find" ? initial.section ?? PICO_ALL_SECTIONS : PICO_ALL_SECTIONS,
      order: initial?._tag === "Find" ? initial.order ?? "korri" : "korri",
    },
    idle: { _tag: "Awake", activity: 0 },
    session: { _tag: "Idle" },
  }
}
