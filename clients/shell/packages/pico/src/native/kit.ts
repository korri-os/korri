/**
 * The view structs main.slint declares (ui.slint, pages.slint), built in one
 * place. Every key here is a property or a struct field there: the shell
 * converts `fooBar` to `foo-bar` and refuses anything undeclared, and the
 * install-time verifier checks every view against the declaration.
 */
import { picoInitials } from "../pico-initials"
import { picoCartLookFor } from "../pico-label"
import { picoPlayed } from "../pico-detail-view"
import type { PicoShelfGame } from "../pico-shelf-game"

export interface HintView { readonly key: "a" | "b" | "lr"; readonly label: string }

/**
 * The target a view entry stands for. view() turns it into the convention's
 * `focusRegion` and `focusActive` (contract 8.4), or drops `focused` from an
 * entry that is no target.
 */
export interface Focusable {
  readonly focused: boolean
  readonly focusId?: string
}

/** A view entry's focus: whether the cursor is on target `id`. */
export const mark = (id: string, focused: string | undefined) => ({ focused: focused === id, focusId: id })

export interface ButtonView extends Focusable {
  readonly label: string
  readonly detail: string
  readonly disabled: boolean
  readonly warn: boolean
  readonly lit: boolean
}

export interface LineView extends Focusable { readonly text: string }
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

export interface KeyView extends Focusable {
  readonly cap: string
  readonly lit: boolean
  readonly row: number
  readonly col: number
  readonly cols: number
}

export interface ImageRef { readonly $image: string; readonly pico8: "vivid" | "flat" }

export interface CartView extends Focusable {
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

/** A launch's named steps (amendment A4): one cell per step, the one running
 * now, and "Step i of n · its label". */
export interface StepsView { readonly shown: boolean; readonly count: number; readonly current: number; readonly words: string }

export const NO_STEPS: StepsView = { shown: false, count: 0, current: 0, words: "" }

export function stepsView(steps: { readonly labels: readonly string[]; readonly current: number } | undefined): StepsView {
  if (steps === undefined || steps.labels.length === 0) return NO_STEPS
  const current = Math.min(Math.max(0, steps.current), steps.labels.length - 1)
  return { shown: true, count: steps.labels.length, current, words: `Step ${current + 1} of ${steps.labels.length} · ${steps.labels[current]}` }
}

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
  options: { readonly hero?: boolean; readonly focused?: boolean; readonly focusId?: string; readonly band?: boolean } = {},
): CartView {
  // A game's cart, once per game object and look: QuickJS builds a shelf of
  // carts on every press otherwise.
  const key = `${options.hero === true}|${options.focused === true}|${options.focusId ?? ""}|${options.band !== false}`
  let byGame = CARTS.get(game)
  if (byGame === undefined) {
    byGame = new Map()
    CARTS.set(game, byGame)
  }
  const known = byGame.get(key)
  if (known !== undefined) return known
  const built = buildCart(game, options)
  byGame.set(key, built)
  return built
}

const CARTS = new WeakMap<object, Map<string, CartView>>()

function buildCart(
  game: Pick<PicoShelfGame, "id" | "title" | "subtitle" | "artUrl" | "resumable" | "playCount" | "totalPlaytimeSeconds">,
  options: { readonly hero?: boolean; readonly focused?: boolean; readonly focusId?: string; readonly band?: boolean },
): CartView {
  const base = cartBase(game)
  // The NEW band is the shelf's: other racks (the grid, Find, the store) draw none.
  const progress = game.resumable === true ? "resume" : base.unplayed && options.band !== false ? "new" : "played"
  // Set field by field: QuickJS spreads an object several times slower.
  const cart: { -readonly [K in keyof CartView]: CartView[K] } = {
    id: base.id,
    title: base.title,
    hasArt: base.hasArt,
    artRatio: base.artRatio,
    initials: base.initials,
    system: base.system,
    shell: base.shell,
    sticker: base.sticker,
    stickerInkLight: base.stickerInkLight,
    progress,
    hero: options.hero === true,
    focused: options.focused === true,
  }
  if (base.art !== undefined) cart.art = base.art
  if (options.focusId !== undefined) cart.focusId = options.focusId
  FLAT_ENTRIES.add(cart)
  return cart
}

/** Entries whose fields hold no other entry (a cart: its art is plain).
 * view() copies one whole rather than field by field (program.ts, `markIn`). */
const FLAT_ENTRIES = new WeakSet<object>()

export const flatEntry = (entry: object): boolean => FLAT_ENTRIES.has(entry)

/** What a game's carts share whatever their look: worked out once per game
 * object, as a cart in a new look (lifted, focused, in the attract) is
 * otherwise worked out again. */
interface CartBase extends Pick<CartView, "id" | "title" | "art" | "hasArt" | "artRatio" | "initials" | "system" | "shell" | "sticker" | "stickerInkLight"> {
  /** Korri gave no play figure: a shelf's cart wears the NEW band. */
  readonly unplayed: boolean
}

function cartBase(game: Pick<PicoShelfGame, "id" | "title" | "subtitle" | "artUrl" | "playCount" | "totalPlaytimeSeconds">): CartBase {
  const known = BASES.get(game)
  if (known !== undefined) return known
  const look = picoCartLookFor(game.id, game.subtitle)
  const cover = art(game.artUrl)
  const base: CartBase = {
    id: game.id,
    title: game.title,
    art: cover,
    hasArt: cover !== undefined,
    artRatio: artRatio(game.artUrl),
    initials: picoInitials(game.title),
    system: look.system,
    shell: look.shell,
    sticker: look.sticker,
    stickerInkLight: look.ink === "light",
    unplayed: !picoPlayed(game),
  }
  BASES.set(game, base)
  return base
}

const BASES = new WeakMap<object, CartBase>()

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
  const known = RATIOS.get(url)
  if (known !== undefined) return known
  const ratio = measuredRatio(url)
  if (RATIOS.size >= 256) RATIOS.clear()
  RATIOS.set(url, ratio)
  return ratio
}

/** Art ratios by URL: reading a PNG's header out of a data URL is not free. */
const RATIOS = new Map<string, number>()

function measuredRatio(url: string): number {
  const size = pngSize(url)
  if (size === undefined) return 0
  const shape = size.width / size.height
  return Math.max(1, Math.round(64 * Math.sqrt(shape))) / Math.max(1, Math.round(64 / Math.sqrt(shape)))
}

/** The narrowest art among games' carts: it makes the tallest cart, and
 * every shelf slot is that tall. Once per list of games. */
export function narrowestArt(games: readonly Pick<PicoShelfGame, "artUrl">[]): number {
  const known = NARROWEST.get(games)
  if (known !== undefined) return known
  const narrowest = games.reduce((least, game) => Math.min(least, art(game.artUrl) === undefined ? 1 : artRatio(game.artUrl)), games.length === 0 ? 1 : Number.POSITIVE_INFINITY)
  NARROWEST.set(games, narrowest)
  return narrowest
}

const NARROWEST = new WeakMap<object, number>()

export const A = (label: string): HintView => ({ key: "a", label })
export const B = (label: string): HintView => ({ key: "b", label })
