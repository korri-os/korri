/**
 * Pico's home shelf and pause menu for the native shell
 * (docs/briefs/2026-10-31-native-surfaces-contract.md, section 3.1).
 * Adapted from the surface-shell spike's Pico package.
 *
 * The rules are Pico's own: `update` from surfaces/pico/src/state/update.ts
 * keeps the chosen cart, `shownScreen` decides what is drawn, and the cart's
 * plastics, initials and stats come from Pico's own view helpers. This file
 * turns the shell's messages into Pico's, and Pico's shelf into one plain
 * view object for main.slint.
 *
 * The catalog is a cursor window (contract 3.4). Pico's code reads
 * `catalog.games`, so the window is handed over as that list; Pico keeps the
 * chosen cart by id, so a new window keeps it. Near a window edge the adapter
 * asks for a window around the chosen cart. Each change of chosen cart is
 * reported (`FocusGame`). Options cycles Pico's face and remembers it
 * (`RememberPreference pico.font`). Covers are quantised to the PICO-8
 * palette by the shell (`pico8: "vivid"`, contract 3.8).
 *
 * While Korri presents the gameplay overlay, the Overlay component shows its
 * controls: up and down move, confirm invokes, back, menu and system resume
 * (Pico's own bindings, src/state/overlay.ts).
 *
 * Pure: no timers, no Date, no randomness, no globals but `surface`.
 */
// Pico's code still speaks the TypeScript treaty it was written against;
// `legacy` below hands it the model in that shape.
import type { SurfaceGameplayControl, SurfaceModel } from "../../../../contracts/surface/korri-surface"
import { picoDownloadLabel, picoStatsFor } from "../../../../surfaces/pico/src/pico-detail-view"
import { PICO_DEFAULT_FONT, PICO_FONT_IDS, type PicoFontId } from "../../../../surfaces/pico/src/pico-fonts"
import { picoInitials } from "../../../../surfaces/pico/src/pico-initials"
import { picoCartLookFor } from "../../../../surfaces/pico/src/pico-label"
import type { PicoShelfGame } from "../../../../surfaces/pico/src/pico-shelf-game"
import type { PicoMessage } from "../../../../surfaces/pico/src/state/messages"
import { initialNavigation, type PicoNavigation } from "../../../../surfaces/pico/src/state/navigation"
import { shownScreen } from "../../../../surfaces/pico/src/state/shown"
import { update as picoUpdate } from "../../../../surfaces/pico/src/state/update"

import type {
  SurfaceButton as Button,
  SurfaceMessage as Message,
  SurfaceModel as Model,
  SurfaceRequest as Request,
} from "../../../../contracts/generated/surface"

interface State {
  readonly nav: PicoNavigation
  /** Whether the cursor is on the shelf. In the portal the first d-pad press
   * puts it there and the chosen cart starts its focus cycle; Pico's own model
   * leaves focus to the DOM, so the adapter keeps this one fact. */
  readonly cursorOnShelf: boolean
  readonly font: PicoFontId
  /** The chosen cart Korri last heard of (FocusGame). */
  readonly reported: string | null
  /** A MoveCatalog is out; no second one until Korri publishes. */
  readonly moving: boolean
  /** The focused control of the gameplay overlay. */
  readonly overlayFocus: number
}

const FONT_KEY = "pico.font"
const EDGE = 8
const WINDOW = 50

const isFont = (value: string | undefined): value is PicoFontId =>
  (PICO_FONT_IDS as readonly (string | undefined)[]).includes(value)

let cachedModel: Model | undefined
let cachedLegacy: SurfaceModel | undefined
/** Pico's code reads `catalog.games`: give it the window. */
function legacy(model: Model): SurfaceModel {
  if (model === cachedModel && cachedLegacy !== undefined) return cachedLegacy
  const catalog = model.catalog._tag === "Ready" ? { _tag: "Ready" as const, games: model.catalog.window } : model.catalog
  cachedModel = model
  cachedLegacy = { ...model, catalog } as unknown as SurfaceModel
  return cachedLegacy
}

/* Home's hints and the shelf readout, as PicoHome.tsx words them. */
const SHELF_HINTS = [
  { hintKey: "a", label: "PLAY" },
  { hintKey: "b", label: "BACK" },
] as const

/** The readout counts every cart Korri has; RESUMABLE counts the window's. */
function shelfReadout(games: readonly PicoShelfGame[], total: number): string {
  const resumable = games.filter((game) => game.resumable === true).length
  const carts = `${total} ${total === 1 ? "CART" : "CARTS"}`
  return resumable === 0 ? carts : `${carts} · ${resumable} RESUMABLE`
}

/** The shelf's games, when home shows the shelf. */
function shelfGames(nav: PicoNavigation, model: Model): readonly PicoShelfGame[] {
  const shown = shownScreen(nav, legacy(model))
  return shown._tag === "Home" && shown.view._tag === "Shelf" ? shown.view.games : []
}

/** The chosen cart's index: PicoCartShelf picks the first when none is kept. */
function chosenIndex(nav: PicoNavigation, games: readonly PicoShelfGame[]): number {
  const found = games.findIndex((game) => game.id === nav.home.selectedGameId)
  return found === -1 ? 0 : found
}

function step(nav: PicoNavigation, message: PicoMessage, model: Model): PicoNavigation {
  // Pico's own requests (waits, QR codes, its face) have no door in this
  // package: only the next model counts. Korri's requests are made below.
  return picoUpdate(nav, message, legacy(model)).model
}

/** Every control of the gameplay overlay, overlay-owned first. */
function overlayControls(model: Model): readonly SurfaceGameplayControl[] {
  const presentation = model.presentation
  if (presentation.kind !== "gameplay-overlay") return []
  return [...presentation.controls, ...presentation.groups.flatMap((group) => group.controls)]
}

/**
 * The shell's buttons as Pico hears them. In the portal a d-pad press reaches
 * Pico as a focus move: the host moves focus to the neighbouring cart, which
 * reports `MovedFocus` and then `SelectedCart`. Left and right do that here.
 * The shelf does not wrap, as the host's focus mover does not.
 *
 * This package draws the shelf and the pause menu only, so Confirm (a game's
 * own screen), Menu (the layout cycle) and Up and Down (MENU, PLAYING NOW)
 * are not passed on to the shelf.
 */
function update(state: State, message: Message, model: Model): { state: State; requests: Request[] } {
  const requests: Request[] = []
  let next = state
  if (message._tag === "KorriPublished") {
    next = { ...state, nav: step(state.nav, { _tag: "KorriPublished" }, model), moving: false }
    const controls = overlayControls(model).length
    if (next.overlayFocus >= controls) next = { ...next, overlayFocus: Math.max(0, controls - 1) }
  } else if (message._tag === "Pressed" && model.presentation.kind === "gameplay-overlay") {
    const controls = overlayControls(model)
    switch (message.button) {
      case "up":
      case "down": {
        const to = Math.max(0, Math.min(controls.length - 1, state.overlayFocus + (message.button === "up" ? -1 : 1)))
        next = { ...state, overlayFocus: to }
        break
      }
      case "confirm": {
        const control = controls[state.overlayFocus]
        if (control !== undefined && control.enabled && control.interaction.kind === "command")
          requests.push({ _tag: "InvokeGameplayControl", controlId: control.id })
        break
      }
      case "back":
      case "menu":
      case "system":
        requests.push({ _tag: "DismissGameplayOverlay" })
        break
      default:
        break
    }
    return { state: next, requests }
  } else if (message._tag === "Pressed") {
    const nav = state.nav
    switch (message.button) {
      case "left":
      case "right": {
        const games = shelfGames(nav, model)
        if (games.length === 0) break
        const from = chosenIndex(nav, games)
        const to = Math.max(0, Math.min(games.length - 1, from + (message.button === "left" ? -1 : 1)))
        const moved = step(nav, { _tag: "MovedFocus" }, model)
        const target = games[to]
        next = to === from || target === undefined
          ? { ...state, nav: moved, cursorOnShelf: true }
          : { ...state, nav: step(moved, { _tag: "SelectedCart", gameId: target.id }, model), cursorOnShelf: true }
        break
      }
      case "back":
        next = { ...state, nav: step(nav, { _tag: "PressedBack" }, model) }
        break
      case "options": {
        const font = PICO_FONT_IDS[(PICO_FONT_IDS.indexOf(state.font) + 1) % PICO_FONT_IDS.length] ?? PICO_DEFAULT_FONT
        next = { ...state, font }
        requests.push({ _tag: "RememberPreference", key: FONT_KEY, value: font })
        break
      }
      default:
        break
    }
  }
  const games = shelfGames(next.nav, model)
  const index = chosenIndex(next.nav, games)
  const chosen = games[index]?.id ?? null
  if (chosen !== next.reported) {
    requests.push(chosen === null ? { _tag: "FocusGame" } : { _tag: "FocusGame", gameId: chosen })
    next = { ...next, reported: chosen }
  }
  const catalog = model.catalog
  if (!next.moving && chosen !== null && catalog._tag === "Ready"
    && ((catalog.after !== undefined && index >= games.length - EDGE) || (catalog.before !== undefined && index < EDGE))) {
    requests.push({ _tag: "MoveCatalog", query: catalog.query, from: { _tag: "Around", gameId: chosen }, count: WINDOW })
    next = { ...next, moving: true }
  }
  return { state: next, requests }
}

/** One cart as main.slint draws it: plastics, silhouette and sticker by index. */
function cartView(game: PicoShelfGame, hero: boolean) {
  const look = picoCartLookFor(game.id, game.subtitle)
  const progress = game.resumable === true ? "resume" : picoStatsFor(game).length === 0 ? "new" : "played"
  const art = game.artUrl === undefined || game.artUrl === "" ? undefined : game.artUrl
  return {
    id: game.id,
    title: game.title,
    ...(art === undefined ? {} : { art: { $image: art, pico8: "vivid" } }),
    hasArt: art !== undefined,
    artRatio: artRatio(art),
    initials: picoInitials(game.title),
    system: look.system,
    shell: look.shell,
    sticker: look.sticker,
    stickerInkLight: look.ink === "light",
    progress,
    hero,
  }
}

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

/** Width and height of a PNG data URL, read from its IHDR; undefined for
 * anything else. Bytes 16 to 23 of a PNG are its width and height. */
function pngSize(url: string): { readonly width: number; readonly height: number } | undefined {
  const comma = url.indexOf(",")
  if (!url.startsWith("data:image/png;base64,") || comma === -1) return undefined
  const bytes: number[] = []
  let bits = 0
  let value = 0
  for (const char of url.slice(comma + 1, comma + 1 + 32)) {
    const digit = BASE64.indexOf(char)
    if (digit === -1) break
    value = ((value << 6) | digit) & 0xffffff
    bits += 6
    if (bits >= 8) {
      bits -= 8
      bytes.push((value >> bits) & 0xff)
    }
  }
  if (bytes.length < 24) return undefined
  const word = (at: number) => (((bytes[at] ?? 0) << 24) | ((bytes[at + 1] ?? 0) << 16) | ((bytes[at + 2] ?? 0) << 8) | (bytes[at + 3] ?? 0)) >>> 0
  const width = word(16)
  const height = word(20)
  return width > 0 && height > 0 ? { width, height } : undefined
}

/**
 * The shape Pico draws a cover at, as width over height. Pico samples a cover
 * onto 64 cells by area at its own shape (use-pico-quantized-art.ts), and the
 * cart takes the shape of that small canvas, so the canvas's rounded size,
 * not the file's, sets the ratio. 0 when the shape cannot be read here; then
 * main.slint reads it from the decoded image.
 */
function artRatio(url: string | undefined): number {
  if (url === undefined || url === "") return 1
  const size = pngSize(url)
  if (size === undefined) return 0
  const shape = size.width / size.height
  return Math.max(1, Math.round(64 * Math.sqrt(shape))) / Math.max(1, Math.round(64 / Math.sqrt(shape)))
}

/**
 * The narrowest art on the shelf, as width over height. Every shelf slot is as
 * tall as the tallest cart (PicoCartShelf.css), and the narrowest art makes
 * the tallest cart; main.slint turns this into a height. Missing art is a
 * square sticker; art whose shape cannot be read counts as the tallest there
 * can be (0).
 */
function narrowestArt(games: readonly PicoShelfGame[]): number {
  return games.reduce((narrowest, game) => Math.min(narrowest, artRatio(game.artUrl)), games.length === 0 ? 1 : Number.POSITIVE_INFINITY)
}

function view(state: State, model: Model) {
  const games = shelfGames(state.nav, model)
  const catalog = model.catalog
  const index = chosenIndex(state.nav, games)
  const chosen = games[index]
  const stats = chosen === undefined ? [] : picoStatsFor(chosen)
  return {
    place: "korri",
    label: "LIBRARY",
    clockLabel: model.clockLabel ?? "",
    showClock: model.clockLabel !== undefined,
    readout: shelfReadout(games, catalog._tag === "Ready" ? catalog.total : games.length),
    hints: SHELF_HINTS.map((hint) => ({ key: hint.hintKey, label: hint.label })),
    menuLabel: "MENU",
    hasShelf: chosen !== undefined,
    selectedIndex: index,
    firstIndex: catalog._tag === "Ready" ? catalog.firstIndex : 0,
    total: catalog._tag === "Ready" ? catalog.total : games.length,
    cursorOnShelf: state.cursorOnShelf,
    narrowestArt: narrowestArt(games),
    carts: games.map((game, at) => cartView(game, at === index)),
    stage: chosen === undefined
      ? { title: "", subtitle: "", kicker: "", resumable: false, download: "", stats: [], cart: null }
      : {
          title: chosen.title,
          subtitle: chosen.subtitle ?? "",
          kicker: chosen.section?.toUpperCase() ?? "",
          resumable: chosen.resumable === true,
          download: picoDownloadLabel(chosen) ?? "",
          stats,
          cart: cartView(chosen, true),
        },
  }
}

/** The pause menu: Korri's controls, in its order. */
function overlay(state: State, model: Model) {
  const presentation = model.presentation
  const controls = overlayControls(model)
  return {
    heading: presentation.kind === "gameplay-overlay" ? (presentation.title ?? "PAUSED").toUpperCase() : "",
    controls: controls.map((control, at) => ({
      label: control.label.toUpperCase(),
      focused: at === state.overlayFocus,
      enabled: control.enabled,
    })),
    hints: [{ key: "a", label: "CHOOSE" }, { key: "b", label: "RESUME" }],
  }
}

;(globalThis as { surface?: unknown }).surface = {
  preferences: { [FONT_KEY]: { mark: "device", initial: PICO_DEFAULT_FONT } },
  init: (_model: Model, preferences: Readonly<Record<string, string>>): State => {
    const stored = preferences[FONT_KEY]
    return {
      nav: initialNavigation(),
      cursorOnShelf: false,
      font: isFont(stored) ? stored : PICO_DEFAULT_FONT,
      reported: null,
      moving: false,
      overlayFocus: 0,
    }
  },
  update,
  view,
  overlay,
}
