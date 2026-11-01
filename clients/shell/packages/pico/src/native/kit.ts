/**
 * The view structs main.slint declares (ui.slint, pages.slint), built in one
 * place. Every key here is a property or a struct field there: the shell
 * converts `fooBar` to `foo-bar` and refuses anything undeclared, and the
 * install-time verifier checks every view against the declaration.
 */
import { picoInitials } from "../pico-initials"
import { picoCartLookFor } from "../pico-label"
import { picoStatsFor } from "../pico-detail-view"
import type { PicoShelfGame } from "../pico-shelf-game"

export interface HintView { readonly key: "a" | "b" | "lr"; readonly label: string }

export interface ButtonView {
  readonly label: string
  readonly detail: string
  readonly focused: boolean
  readonly disabled: boolean
  readonly warn: boolean
  readonly lit: boolean
}

export interface LineView { readonly text: string; readonly focused: boolean }
export interface ChipView { readonly text: string; readonly caption: string; readonly kind: string }

export interface CardView {
  readonly shown: boolean
  readonly tone: "ask" | "warn" | "tell"
  readonly kicker: string
  readonly title: string
  readonly message: string
  readonly lines: readonly LineView[]
  readonly actions: readonly ButtonView[]
  readonly inline: boolean
}

export interface BarView { readonly shown: boolean; readonly cells: number; readonly walking: boolean; readonly label: string }

export interface KeyView {
  readonly cap: string
  readonly focused: boolean
  readonly lit: boolean
  readonly row: number
  readonly col: number
  readonly cols: number
}

export interface ImageRef { readonly $image: string; readonly pico8: "vivid" | "flat" }

export interface CartView {
  readonly id: string
  readonly title: string
  readonly art?: ImageRef
  readonly hasArt: boolean
  readonly artRatio: number
  readonly initials: string
  readonly system: string
  readonly shell: number
  readonly sticker: number
  readonly stickerInkLight: boolean
  readonly progress: "new" | "played" | "resume"
  /** Lifted: the chosen cart on a shelf. */
  readonly hero: boolean
  /** The cursor is on it. */
  readonly focused: boolean
}

export const button = (label: string, options: Partial<ButtonView> = {}): ButtonView => ({
  label,
  detail: "",
  focused: false,
  disabled: false,
  warn: false,
  lit: false,
  ...options,
})

export const NO_CARD: CardView = {
  shown: false,
  tone: "ask",
  kicker: "",
  title: "",
  message: "",
  lines: [],
  actions: [],
  inline: false,
}

export const card = (options: Partial<CardView>): CardView => ({ ...NO_CARD, shown: true, ...options })

export const chip = (text: string, kind: string, caption = ""): ChipView => ({ text, caption, kind })

export const NO_BAR: BarView = { shown: false, cells: 0, walking: false, label: "" }

/** Sixteen cells from Korri's bytes, or a walking band when the end is unknown. */
export function bar(progress: { readonly done: number; readonly total?: number } | undefined, label = ""): BarView {
  if (progress === undefined) return { ...NO_BAR, shown: true, walking: true, label }
  if (progress.total === undefined || progress.total <= 0) return { shown: true, cells: 0, walking: true, label }
  const cells = Math.max(0, Math.min(16, Math.floor((progress.done / progress.total) * 16)))
  return { shown: true, cells, walking: false, label }
}

/** Cover art, sampled to the sixteen by the shell (contract 3.8), as
 * use-pico-quantized-art.ts did: vivid for covers. */
export const art = (url: string | undefined): ImageRef | undefined =>
  url === undefined || url === "" ? undefined : { $image: url, pico8: "vivid" }

/** One game as a cartridge. */
export function cartView(
  game: Pick<PicoShelfGame, "id" | "title" | "subtitle" | "artUrl" | "resumable" | "playCount" | "totalPlaytimeSeconds">,
  options: { readonly hero?: boolean; readonly focused?: boolean; readonly band?: boolean } = {},
): CartView {
  const look = picoCartLookFor(game.id, game.subtitle)
  const cover = art(game.artUrl)
  // The NEW band is the shelf's: other racks (the grid, Find, the store) draw none.
  const progress = game.resumable === true ? "resume" : picoStatsFor(game).length === 0 && options.band !== false ? "new" : "played"
  return {
    id: game.id,
    title: game.title,
    ...(cover === undefined ? {} : { art: cover }),
    hasArt: cover !== undefined,
    artRatio: artRatio(game.artUrl),
    initials: picoInitials(game.title),
    system: look.system,
    shell: look.shell,
    sticker: look.sticker,
    stickerInkLight: look.ink === "light",
    progress,
    hero: options.hero === true,
    focused: options.focused === true,
  }
}

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

/** Width and height of a PNG data URL from its IHDR; undefined for anything else. */
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
  const word = (at: number) =>
    (((bytes[at] ?? 0) << 24) | ((bytes[at + 1] ?? 0) << 16) | ((bytes[at + 2] ?? 0) << 8) | (bytes[at + 3] ?? 0)) >>> 0
  const width = word(16)
  const height = word(20)
  return width > 0 && height > 0 ? { width, height } : undefined
}

/**
 * The shape Pico draws a cover at, width over height: the cover sampled onto
 * about 64 x 64 cells by area at its own shape (use-pico-quantized-art.ts),
 * so the small canvas's rounded size sets the ratio. 0 when it cannot be
 * read here; then main.slint reads it from the decoded image.
 */
export function artRatio(url: string | undefined): number {
  if (url === undefined || url === "") return 1
  const size = pngSize(url)
  if (size === undefined) return 0
  const shape = size.width / size.height
  return Math.max(1, Math.round(64 * Math.sqrt(shape))) / Math.max(1, Math.round(64 / Math.sqrt(shape)))
}

/** The narrowest art among carts: it makes the tallest cart, and every
 * shelf slot is that tall. */
export function narrowestArt(carts: readonly CartView[]): number {
  return carts.reduce((narrowest, cart) => Math.min(narrowest, cart.hasArt ? cart.artRatio : 1), carts.length === 0 ? 1 : Number.POSITIVE_INFINITY)
}

export const A = (label: string): HintView => ({ key: "a", label })
export const B = (label: string): HintView => ({ key: "b", label })
