/**
 * Pico's home shelf as a runtime-loaded package (spike, see ../../CONTRACT.md).
 *
 * The rules are Pico's own: `update` from surfaces/pico/src/state/update.ts
 * keeps the chosen cart, `shownScreen` decides what is drawn, and the cart's
 * plastics, initials and stats come from Pico's own view helpers. This file
 * only turns the shell's `Pressed` messages into Pico's messages, and Pico's
 * shelf into one plain view object for main.slint.
 *
 * Pure: no timers, no Date, no randomness, no globals but `surface`.
 */
import type { SurfaceModel } from "../../../../contracts/surface/korri-surface"
import { picoDownloadLabel, picoStatsFor } from "../../../../surfaces/pico/src/pico-detail-view"
import { picoInitials } from "../../../../surfaces/pico/src/pico-initials"
import { picoCartLookFor } from "../../../../surfaces/pico/src/pico-label"
import type { PicoShelfGame } from "../../../../surfaces/pico/src/pico-shelf-game"
import type { PicoMessage } from "../../../../surfaces/pico/src/state/messages"
import { initialNavigation, type PicoNavigation } from "../../../../surfaces/pico/src/state/navigation"
import { shownScreen } from "../../../../surfaces/pico/src/state/shown"
import { update as picoUpdate } from "../../../../surfaces/pico/src/state/update"

type Action = "up" | "down" | "left" | "right" | "confirm" | "back" | "menu" | "options"
type Message = { readonly _tag: "Pressed"; readonly action: Action }

interface State {
  readonly nav: PicoNavigation
  /** Whether the cursor is on the shelf. In the portal the first d-pad press
   * puts it there and the chosen cart starts its focus cycle; Pico's own model
   * leaves focus to the DOM, so the adapter keeps this one fact. */
  readonly cursorOnShelf: boolean
}

/* Home's hints and the shelf readout, as PicoHome.tsx words them. */
const SHELF_HINTS = [
  { hintKey: "a", label: "PLAY" },
  { hintKey: "b", label: "BACK" },
] as const

function shelfReadout(games: readonly PicoShelfGame[]): string {
  const resumable = games.filter((game) => game.resumable === true).length
  const carts = `${games.length} ${games.length === 1 ? "CART" : "CARTS"}`
  return resumable === 0 ? carts : `${carts} · ${resumable} RESUMABLE`
}

/** The shelf's games, when home shows the shelf. */
function shelfGames(nav: PicoNavigation, model: SurfaceModel): readonly PicoShelfGame[] {
  const shown = shownScreen(nav, model)
  return shown._tag === "Home" && shown.view._tag === "Shelf" ? shown.view.games : []
}

/** The chosen cart's index: PicoCartShelf picks the first when none is kept. */
function chosenIndex(nav: PicoNavigation, games: readonly PicoShelfGame[]): number {
  const found = games.findIndex((game) => game.id === nav.home.selectedGameId)
  return found === -1 ? 0 : found
}

function step(nav: PicoNavigation, message: PicoMessage, model: SurfaceModel): PicoNavigation {
  // Requests (waits, Korri actions) have no door in this spike; only the next model counts.
  return picoUpdate(nav, message, model).model
}

/**
 * The shell's buttons as Pico hears them. In the portal a d-pad press reaches
 * Pico as a focus move: the host moves focus to the neighbouring cart, which
 * reports `MovedFocus` and then `SelectedCart`. Left and right do that here.
 * The shelf does not wrap, as the host's focus mover does not.
 *
 * This package draws the shelf only, so Confirm (a game's own screen), Menu
 * (the layout cycle), Options (Find) and Up and Down (MENU, PLAYING NOW) are
 * not passed on: Pico would change to a screen this package cannot draw.
 * Back is Pico's own, and on a bare shelf it changes nothing.
 */
function update(state: State, message: Message, model: SurfaceModel): State {
  if (message._tag !== "Pressed") return state
  const nav = state.nav
  switch (message.action) {
    case "left":
    case "right": {
      const games = shelfGames(nav, model)
      if (games.length === 0) return state
      const from = chosenIndex(nav, games)
      const to = Math.max(0, Math.min(games.length - 1, from + (message.action === "left" ? -1 : 1)))
      const moved = step(nav, { _tag: "MovedFocus" }, model)
      const target = games[to]
      if (to === from || target === undefined) return { nav: moved, cursorOnShelf: true }
      return { nav: step(moved, { _tag: "SelectedCart", gameId: target.id }, model), cursorOnShelf: true }
    }
    case "back":
      return { ...state, nav: step(nav, { _tag: "PressedBack" }, model) }
    default:
      return state
  }
}

/** One cart as main.slint draws it: plastics, silhouette and sticker by index. */
function cartView(game: PicoShelfGame, hero: boolean) {
  const look = picoCartLookFor(game.id, game.subtitle)
  const progress = game.resumable === true ? "resume" : picoStatsFor(game).length === 0 ? "new" : "played"
  const art = game.artUrl === undefined || game.artUrl === "" ? undefined : game.artUrl
  return {
    id: game.id,
    title: game.title,
    ...(art === undefined ? {} : { art: { $image: art } }),
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

function view(state: State, model: SurfaceModel) {
  const games = shelfGames(state.nav, model)
  const index = chosenIndex(state.nav, games)
  const chosen = games[index]
  const stats = chosen === undefined ? [] : picoStatsFor(chosen)
  return {
    place: "korri",
    label: "LIBRARY",
    clockLabel: model.clockLabel ?? "",
    showClock: model.clockLabel !== undefined,
    readout: shelfReadout(games),
    hints: SHELF_HINTS.map((hint) => ({ key: hint.hintKey, label: hint.label })),
    menuLabel: "MENU",
    hasShelf: chosen !== undefined,
    selectedIndex: index,
    total: games.length,
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

;(globalThis as { surface?: unknown }).surface = {
  init: (_model: SurfaceModel): State => ({ nav: initialNavigation(), cursorOnShelf: false }),
  update,
  view,
}
