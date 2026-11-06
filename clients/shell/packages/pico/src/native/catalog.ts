/**
 * The catalog's screens as views and focus targets: home (shelf, grid, hero,
 * MENU, PLAYING NOW, Korri's launch and failure screens), Find and a game's
 * own screen. Ports of PicoHome, PicoLibrary and PicoGameDetail; the words
 * are theirs.
 */
import { PICO_FONTS } from "../pico-fonts"
import type { SurfaceAction, SurfaceGame } from "../model"
import { picoDetailViewFromGame, picoDownloadLabel, picoStatsFor } from "../pico-detail-view"
import { picoInvitePlaying, picoInvitesFrom, picoInviteWhere } from "../pico-invite-view"
import {
  PICO_ALL_SECTIONS,
  PICO_ORDER_LABELS,
  PICO_ORDERS,
  picoCollectionsFrom,
  picoHeroPick,
} from "../pico-library-view"
import { picoShelfGame } from "../pico-home-view"
import type { PicoScreenView } from "../pico-screen-view"
import type { PicoShelfGame } from "../pico-shelf-game"
import type { PicoHomeMode } from "../pico-types"
import type { PicoShown } from "../state/shown"
import type { Target } from "./focus"
import { frame, type FrameView } from "./frame"
import { keyboard, type KeyPress } from "./keyboard"
import {
  A,
  B,
  bar,
  button,
  card,
  type ButtonView,
  type CardView,
  cartView,
  type CartView,
  chip,
  type ChipView,
  type HintView,
  NO_BAR,
  NO_CARD,
  NO_STEPS,
  stepsView,
  type StepsView,
  narrowestArt,
  art,
  mark,
} from "./kit"
import { memo } from "./memo"
import { type Act, type Ctx, local, nav, type Screen } from "./types"

const MENU_LABEL = "MENU"
const MODE_LABELS: Record<PicoHomeMode, string> = { shelf: "LIBRARY", grid: "LIBRARY · GRID", hero: "LIBRARY · HERO" }
const VIEW_WORDS: Record<PicoHomeMode, string> = { shelf: "SHELF", grid: "GRID", hero: "HERO" }

export interface StageView {
  readonly shown: boolean
  readonly title: string
  readonly subtitle: string
  readonly kicker: string
  readonly chips: readonly ChipView[]
  readonly index: number
  readonly total: number
  readonly cart: CartView
}

export interface InviteView {
  readonly title: string
  readonly who: string
  readonly art?: { readonly $image: string; readonly pico8: "vivid" }
  readonly hasArt: boolean
  readonly actions: readonly ButtonView[]
}

export interface GridSectionView {
  readonly title: string
  /** Rows of carts in the sections before this one. */
  readonly rowStart: number
  readonly carts: readonly CartView[]
}

export interface HeroView {
  readonly title: string
  readonly subtitle: string
  readonly reason: string
  readonly chips: readonly ChipView[]
  readonly open: ButtonView
  readonly resume: readonly { readonly cart: CartView; readonly title: string; readonly subtitle: string }[]
}

export interface LaunchStageView {
  readonly kicker: string
  readonly detail: string
  readonly bar: ReturnType<typeof bar>
  readonly steps: StepsView
  readonly hasCart: boolean
  readonly cart: CartView
  /** "starting", "running" or "paused". */
  readonly phase: string
  readonly actions: readonly ButtonView[]
}

const EMPTY_CART: CartView = cartView({ id: "", title: "" })
const NO_STAGE: StageView = { shown: false, title: "", subtitle: "", kicker: "", chips: [], index: 0, total: 0, cart: EMPTY_CART }
/** A card not drawn: the shell keeps the fields it is not sent. */
const HIDDEN_CARD = { shown: false }
const NO_HERO: HeroView = { title: "", subtitle: "", reason: "", chips: [], open: button("OPEN"), resume: [] }
const NO_LAUNCH: LaunchStageView = { kicker: "", detail: "", bar: NO_BAR, steps: NO_STEPS, hasCart: false, cart: EMPTY_CART, phase: "starting", actions: [] }

/** RESUME, the download, then the play facts or NEVER PLAYED (PicoGameFacts). */
export function factChips(game: PicoShelfGame): readonly ChipView[] {
  const stats = picoStatsFor(game)
  const download = picoDownloadLabel(game)
  return [
    ...(game.resumable === true ? [chip("RESUME", "ok")] : []),
    ...(download === undefined ? [] : [chip(download, "info")]),
    ...(stats.length === 0 ? [chip("NEVER PLAYED", "empty")] : stats.map(stat => chip(stat.figure, "stat", stat.caption))),
  ]
}

const focusId = (ctx: Ctx) => ctx.s.focus

/** A card's answers and report lines as targets, stacked or side by side. */
export function cardTargets(
  shown: CardView,
  ids: readonly string[],
  acts: readonly (Act | undefined)[],
  focused: string | undefined,
  lines: number = 0,
  region: string = "card",
): { readonly card: CardView; readonly targets: Target<Act>[] } {
  const targets: Target<Act>[] = []
  for (let i = 0; i < lines; i += 1) targets.push({ id: `line:${i}`, region, box: { x: 0, y: i * 10, w: 100, h: 8 } })
  shown.actions.forEach((_, i) => {
    const id = ids[i] ?? `answer:${i}`
    const act = acts[i]
    if (shown.actions[i]?.disabled === true) return
    targets.push({
      id,
      region, box: shown.inline ? { x: i * 50, y: lines * 10 + 10, w: 48, h: 8 } : { x: 0, y: (lines + i) * 10 + 10, w: 100, h: 8 },
      ...(act === undefined ? {} : { act }),
    })
  })
  return {
    card: {
      ...shown,
      lines: shown.lines.map((line, i) => ({ ...line, ...mark(`line:${i}`, focused) })),
      actions: shown.actions.map((action, i) => ({ ...action, ...mark(ids[i] ?? `answer:${i}`, focused) })),
    },
    targets,
  }
}

function homeHints(
  view: PicoScreenView,
  menuOffered: boolean,
  menuOpen: boolean,
  aim: string | undefined,
): readonly HintView[] {
  if (menuOpen) return [...(aim === undefined || aim === MENU_LABEL ? [] : [A(aim)]), B("CLOSE")]
  if (menuOffered && aim === MENU_LABEL) return [A(MENU_LABEL), B("BACK")]
  if (view._tag === "Shelf") return [A("PLAY"), B("BACK")]
  if ((view._tag === "Busy" && (view.actions?.length ?? 0) === 0) || (view._tag === "Paused" && view.actions.length === 0)) return []
  return [B("BACK")]
}

function shelfReadout(games: readonly PicoShelfGame[], total: number): string {
  const resumable = resumableIn(games)
  const carts = `${total} ${total === 1 ? "CART" : "CARTS"}`
  return resumable === 0 ? carts : `${carts} · ${resumable} RESUMABLE`
}

/** How many of the games resume, once per list of games. */
function resumableIn(games: readonly PicoShelfGame[]): number {
  const known = RESUMABLE.get(games)
  if (known !== undefined) return known
  const count = games.filter(game => game.resumable === true).length
  RESUMABLE.set(games, count)
  return count
}

const RESUMABLE = new WeakMap<object, number>()

/** The cart the shelf has chosen: the kept one, else the first. */
export function chosenIndex(games: readonly PicoShelfGame[], selected: string | undefined): number {
  const found = indexOfGame(games, selected)
  return found === -1 ? 0 : found
}

/** Where the game `id` is in `games`, or -1. A loop: QuickJS calls a
 * callback for each game far slower. */
function indexOfGame(games: readonly PicoShelfGame[], id: string | undefined): number {
  for (let i = 0; i < games.length; i += 1) if (games[i]?.id === id) return i
  return -1
}

const statusActions = (actions: readonly SurfaceAction[] | undefined, focused: string | undefined): readonly ButtonView[] =>
  (actions ?? []).map(action => button(action.label, {
    detail: action.description ?? "",
    disabled: !action.enabled,
    ...mark(`status:${action.id}`, focused),
  }))

/**
 * How many carts either side of the chosen one PicoCartShelf can show on a
 * screen `width` virtual pixels wide (the plan's). The rack scrolls only to
 * keep the chosen cart in view with a slot to spare on its left and two on
 * its right, so a cart shows only while it lies less than the width less a
 * slot away. A slot is at least 29 virtual pixels wide and 32 apart (a short
 * body's: 20 of art, 6 of shell, 3 of shadow, 3 of gap).
 */
function shelfReach(width: number): number {
  return Math.max(1, Math.floor((width - 29) / 32) + 1)
}

/** The chosen game on the stage, once per game and place: a view part kept
 * from the last view is not looked through again (program.ts, `markIn`). */
function stageView(game: PicoShelfGame, index: number, total: number): StageView {
  const key = `${index}|${total}`
  let known = STAGES.get(game)
  if (known === undefined) {
    known = new Map()
    STAGES.set(game, known)
  }
  const cached = known.get(key)
  if (cached !== undefined) return cached
  const stage: StageView = {
    shown: true,
    title: game.title,
    subtitle: game.subtitle ?? "",
    kicker: (game.section ?? "").toUpperCase(),
    chips: factChips(game),
    index,
    total,
    cart: cartView(game, { hero: true }),
  }
  if (known.size > 8) known.clear()
  known.set(key, stage)
  return stage
}

const STAGES = new WeakMap<object, Map<string, StageView>>()

/** Home's frame, once per words and lead: as `stageView`. */
function homeFrame(label: string, clock: string, readout: string, hints: readonly HintView[], hasLead: boolean, leadFocused: boolean, lit: boolean): FrameView {
  const key = `${label}|${clock}|${readout}|${hints.map(hint => `${hint.key}:${hint.label}`).join(",")}|${hasLead}|${leadFocused}|${lit}`
  const cached = HOME_FRAMES.get(key)
  if (cached !== undefined) return cached
  const built = frame({ label, clock, readout, hints, hasLead, lead: button(MENU_LABEL, { ...mark("menu", leadFocused ? "menu" : undefined), lit }) })
  if (HOME_FRAMES.size > 16) HOME_FRAMES.clear()
  HOME_FRAMES.set(key, built)
  return built
}

const HOME_FRAMES = new Map<string, FrameView>()

/** A shelf cart's target, once per game, place and aim: a shelf of carts is
 * rebuilt on every press otherwise. */
function shelfTarget(game: PicoShelfGame, place: number, aiming: boolean): Target<Act> {
  const key = `${place}|${aiming}`
  let known = SHELF_TARGETS.get(game)
  if (known === undefined) {
    known = new Map()
    SHELF_TARGETS.set(game, known)
  }
  const cached = known.get(key)
  if (cached !== undefined) return cached
  const target: Target<Act> = {
    id: `cart:${game.id}`,
    region: "shelf",
    box: { x: place * 10, y: 500, w: 9, h: 20 },
    act: [nav({ _tag: "OpenedGame", gameId: game.id })],
    enter: [nav({ _tag: "SelectedCart", gameId: game.id }), ...(aiming ? [nav({ _tag: "AimedMenu", label: undefined })] : [])],
  }
  if (known.size > 8) known.clear()
  known.set(key, target)
  return target
}

const SHELF_TARGETS = new WeakMap<object, Map<string, Target<Act>>>()

/** The games that resume, once per window. */
const resumableOf = memo(2, (games: readonly PicoShelfGame[]) => [games], (games: readonly PicoShelfGame[]) => games.filter(game => game.resumable === true))

/** A RESUME cart's target, once per game and place. */
function resumeTarget(game: PicoShelfGame, place: number): Target<Act> {
  let known = RESUME_TARGETS.get(game)
  if (known === undefined) {
    known = new Map()
    RESUME_TARGETS.set(game, known)
  }
  let target = known.get(place)
  if (target === undefined) {
    target = { id: `resume:${game.id}`, region: "resume", box: { x: place * 10, y: 300, w: 9, h: 20 }, act: [nav({ _tag: "OpenedGame", gameId: game.id })], enter: [nav({ _tag: "AimedMenu", label: undefined })] }
    if (known.size > 8) known.clear()
    known.set(place, target)
  }
  return target
}

const RESUME_TARGETS = new WeakMap<object, Map<number, Target<Act>>>()

/** A RESUME entry, once per game and focus. */
function resumeView(game: PicoShelfGame, focused: boolean): HeroView["resume"][number] {
  let known = RESUME_VIEWS.get(game)
  if (known === undefined) {
    known = [undefined, undefined]
    RESUME_VIEWS.set(game, known)
  }
  const at = focused ? 1 : 0
  let view = known[at]
  if (view === undefined) {
    view = { cart: cartView(game, { focused, focusId: `resume:${game.id}`, hero: focused, band: false }), title: game.title, subtitle: game.subtitle ?? "" }
    known[at] = view
  }
  return view
}

const RESUME_VIEWS = new WeakMap<object, [HeroView["resume"][number] | undefined, HeroView["resume"][number] | undefined]>()

/** A grid cart's target, once per game, place and aim (as `shelfTarget`). */
function gridTarget(game: PicoShelfGame, x: number, y: number, aiming: boolean): Target<Act> {
  const key = `${x}|${y}|${aiming}`
  let known = GRID_TARGETS.get(game)
  if (known === undefined) {
    known = new Map()
    GRID_TARGETS.set(game, known)
  }
  const cached = known.get(key)
  if (cached !== undefined) return cached
  const target: Target<Act> = {
    id: `cart:${game.id}`,
    region: "grid", box: { x, y, w: 30, h: 44 },
    act: [nav({ _tag: "OpenedGame", gameId: game.id })],
    enter: [nav({ _tag: "SelectedCart", gameId: game.id }), ...(aiming ? [nav({ _tag: "AimedMenu", label: undefined })] : [])],
  }
  if (known.size > 8) known.clear()
  known.set(key, target)
  return target
}

const GRID_TARGETS = new WeakMap<object, Map<string, Target<Act>>>()

/** The grid's sections, once per window. */
const collectionsOf = memo(2, (games: readonly PicoShelfGame[]) => [games], picoCollectionsFrom)

export function homeScreen(ctx: Ctx, shown: Extract<PicoShown, { _tag: "Home" }>): Screen {
  const { s, korri, model } = ctx
  const view = shown.view
  const home = shown.home
  const focused = focusId(ctx)
  const library = view._tag === "Shelf" || view._tag === "Loading" || view._tag === "Empty" || view._tag === "Failed"
  const menuOpen = library && home.menu === "Open"
  const targets: Target<Act>[] = []
  const games = view._tag === "Shelf" ? view.games : []
  const catalog = korri.catalog
  const firstIndex = catalog._tag === "Ready" ? catalog.firstIndex : 0
  const total = catalog._tag === "Ready" ? catalog.total : games.length
  const chosen = chosenIndex(games, home.selectedGameId)
  const chosenGame = games[chosen]
  const chosenId = chosenGame === undefined ? undefined : `cart:${chosenGame.id}`

  // MENU's list, while it is open: FIND, GET GAMES, SETTINGS, VIEW.
  const entries: readonly { readonly label: string; readonly detail: string; readonly act: Act }[] = !menuOpen ? [] : [
    { label: "FIND", detail: "", act: [nav({ _tag: "ChoseMenuFind" })] },
    ...(model.store === undefined ? [] : [{ label: "GET GAMES", detail: "", act: [nav({ _tag: "ChoseMenuStore" })] }]),
    { label: "SETTINGS", detail: "", act: [nav({ _tag: "ChoseMenuSettings" })] },
    { label: "VIEW", detail: `◀ ${VIEW_WORDS[home.mode]} ▶`, act: [nav({ _tag: "ChoseMenuView" })] },
  ]
  const aimAt = (label: string | undefined): Act => [nav({ _tag: "AimedMenu", label })]
  const menuKey: Target<Act> = {
    id: "menu",
    region: "floor", box: { x: 0, y: 1000, w: 10, h: 8 },
    act: [nav({ _tag: "ToggledHomeMenu" })],
    enter: aimAt(MENU_LABEL),
  }

  let body = "none"
  let stage: StageView = NO_STAGE
  let carts: CartView[] = []
  /** The first game of the window the shelf's carts start at. */
  let shelfStart = 0
  let sections: GridSectionView[] = []
  let hero: HeroView = NO_HERO
  let shownCard: CardView = NO_CARD
  let launch: LaunchStageView = NO_LAUNCH
  let invites: InviteView[] = []

  if (menuOpen) {
    entries.forEach((entry, i) => targets.push({
      id: `menu:${entry.label}`,
      region: "menu", box: { x: 0, y: 900 + i * 10, w: 30, h: 8 },
      act: entry.act,
      enter: aimAt(entry.label),
    }))
    targets.push(menuKey)
  }

  if (library) {
    // PLAYING NOW: every invite Korri publishes, with exactly its actions.
    const standing = picoInvitesFrom(model)
    invites = standing.map((invite, row) => {
      const actions = invite.actions.map((action, column) => {
        const id = `invite:${action.id}`
        if (!menuOpen && action.enabled) {
          targets.push({
            id,
            region: "invites", box: { x: 50 + column * 20, y: row * 10, w: 18, h: 8 },
            act: [nav({ _tag: "PressedInviteAction", actionId: action.id })],
            enter: aimAt(undefined),
          })
        }
        return button(action.label, { disabled: !action.enabled, ...mark(id, focused) })
      })
      const cover = art(invite.artUrl)
      return {
        title: invite.gameTitle,
        who: `${picoInvitePlaying(invite.personName)} ${picoInviteWhere(invite.deviceLabel)}`,
        ...(cover === undefined ? {} : { art: { $image: cover.$image, pico8: "vivid" as const } }),
        hasArt: cover !== undefined,
        actions,
      }
    })
  }

  if (view._tag === "Shelf" && home.mode === "shelf") {
    body = "shelf"
    const aiming = home.aim !== undefined
    // Only the carts the shelf can show around the chosen and focused ones
    // are sent, and are targets: a view's JSON costs by its values.
    const at = focused === undefined || !focused.startsWith("cart:") ? chosen : indexOfGame(games, focused.slice("cart:".length))
    const reach = shelfReach(ctx.plan.width)
    shelfStart = Math.max(0, Math.min(chosen, at === -1 ? chosen : at) - reach)
    const end = Math.min(games.length, Math.max(chosen, at) + reach + 1)
    carts = games.slice(shelfStart, end).map((game, k) => {
      const i = shelfStart + k
      const id = `cart:${game.id}`
      if (!menuOpen) targets.push(shelfTarget(game, firstIndex + i, aiming))
      return cartView(game, { hero: i === chosen, focused: focused === id, focusId: id })
    })
    if (chosenGame !== undefined) stage = stageView(chosenGame, firstIndex + chosen + 1, total)
  } else if (view._tag === "Shelf" && home.mode === "grid") {
    body = "grid"
    const columns = ctx.plan.columns
    // As PicoCartGrid draws it, in virtual pixels: a section's title line
    // (the font's rows, 1.5 high), its rows of 44 in slots 34 apart, its
    // floor, and 4 between sections. The focus plan must be drawn to scale,
    // or a press picks a different nearest cart than the one drawn there.
    const rowsOfFont = (PICO_FONTS.find(font => font.id === ctx.s.font) ?? PICO_FONTS[0])?.rows ?? 7
    const title = rowsOfFont * 1.5
    let rowStart = 0
    let top = 0
    const aiming = home.aim !== undefined
    sections = collectionsOf(games).map((collection) => {
      const rowsTop = top + title + 2
      top = rowsTop + Math.ceil(collection.games.length / columns) * 44 + 2 + 2 + 4
      const section = {
        title: collection.title,
        rowStart,
        carts: collection.games.map((game, i) => {
          const id = `cart:${game.id}`
          if (!menuOpen) targets.push(gridTarget(game, (i % columns) * 34, rowsTop + Math.floor(i / columns) * 44, aiming))
          const on = focused === id
          return cartView(game, { focused: on, focusId: id, hero: on, band: false })
        }),
      }
      rowStart += Math.ceil(collection.games.length / columns)
      return section
    })
  } else if (view._tag === "Shelf" && home.mode === "hero") {
    body = "hero"
    // Only once Korri answers in `recent` order is the window's first game the
    // one played last in the whole library (program.ts, HERO_QUERY).
    const answered = catalog._tag === "Ready" && catalog.query.order === "recent" && catalog.query.text === "" && catalog.query.section == null
    const pick = answered ? picoHeroPick(games) : undefined
    const resumable = resumableOf(games)
    if (pick !== undefined && !menuOpen) {
      targets.push({ id: "hero:open", region: "hero", box: { x: 0, y: 100, w: 30, h: 10 }, act: [nav({ _tag: "OpenedGame", gameId: pick.game.id })], enter: aimAt(undefined) })
    }
    // The RESUME list is drawn under the hero only.
    if (pick !== undefined && !menuOpen) resumable.forEach((game, i) => targets.push(resumeTarget(game, i)))
    hero = pick === undefined ? hero : {
      title: pick.game.title,
      subtitle: pick.game.subtitle ?? "",
      reason: pick.reason ?? "",
      chips: factChips(pick.game),
      open: button("OPEN", { ...mark("hero:open", focused) }),
      resume: resumable.map(game => resumeView(game, focused === `resume:${game.id}`)),
    }
  } else if (view._tag === "Loading" || view._tag === "Empty" || view._tag === "Failed") {
    body = "card"
    const notice = view._tag === "Loading"
      ? card({ tone: "tell", kicker: "READING CARTS", message: "Korri is looking through your library." })
      : view._tag === "Empty"
        ? card({ tone: "tell", kicker: "NO CARTS", message: "Nothing to play yet. Add games to your library and they appear here." })
        : card({ tone: "warn", kicker: "SHELF JAMMED", message: view.message, actions: [button("TRY AGAIN")] })
    const placed = cardTargets(notice, ["retry"], [[nav({ _tag: "PressedRetry" })]], focused)
    if (!menuOpen) targets.push(...placed.targets.map(target => ({ ...target, box: { ...target.box, y: target.box.y + 500 } })))
    shownCard = placed.card
  } else if (view._tag === "Confirm") {
    body = "card"
    const ids = view.actions.map(action => `status:${action.id}`)
    const placed = cardTargets(
      card({ tone: "ask", kicker: view.kicker, title: view.cart?.title ?? "", message: view.detail ?? "", actions: view.actions.map(action => button(action.label, { disabled: !action.enabled })) }),
      ids,
      view.actions.map(action => [nav({ _tag: "PressedStatusAction", actionId: action.id })]),
      focused,
    )
    targets.push(...placed.targets)
    shownCard = placed.card
  } else if (view._tag === "Problem") {
    body = "card"
    const answers = view.canRetry ? [button("RETRY"), button("CANCEL")] : [button("OK")]
    const placed = cardTargets(
      card({ tone: "warn", kicker: view.kicker, title: view.gameTitle ?? "", message: view.reason, actions: answers }),
      view.canRetry ? ["retry", "dismiss"] : ["dismiss"],
      view.canRetry ? [[nav({ _tag: "PressedRetry" })], [nav({ _tag: "PressedDismiss" })]] : [[nav({ _tag: "PressedDismiss" })]],
      focused,
    )
    targets.push(...placed.targets)
    shownCard = placed.card
  } else if (view._tag === "Ended") {
    body = "card"
    const placed = cardTargets(card({ tone: "tell", kicker: view.kicker, message: view.reason, actions: [button("OK")] }), ["dismiss"], [[nav({ _tag: "PressedDismiss" })]], focused)
    targets.push(...placed.targets)
    shownCard = placed.card
  } else if (view._tag === "Busy" || view._tag === "Running" || view._tag === "Paused") {
    body = "stage"
    const actions = view._tag === "Running" ? [] : view.actions ?? []
    actions.forEach((action, i) => {
      if (action.enabled) targets.push({ id: `status:${action.id}`, region: "stage", box: { x: i * 20, y: 0, w: 18, h: 8 }, act: [nav({ _tag: "PressedStatusAction", actionId: action.id })] })
    })
    const progress = view._tag === "Busy" ? view.progress : undefined
    launch = {
      kicker: view.kicker,
      detail: view._tag === "Running" ? view.gameTitle ?? "" : view.detail ?? "",
      // A Busy with no progress keeps the clock meter: every cell lit. A
      // launch's steps are its meter then: a full meter under them would
      // read as done.
      bar: view._tag === "Busy"
        ? (progress !== undefined ? bar(progress) : view.steps === undefined ? { ...NO_BAR, shown: true, cells: 16 } : NO_BAR)
        : NO_BAR,
      steps: view._tag === "Busy" ? stepsView(view.steps) : NO_STEPS,
      hasCart: view.cart !== undefined,
      cart: view.cart === undefined ? EMPTY_CART : cartView({ id: view.cart.id, title: view.cart.title, ...(view.cart.artUrl === undefined ? {} : { artUrl: view.cart.artUrl }) }, { band: false }),
      phase: view._tag === "Busy" ? "starting" : view._tag === "Running" ? "running" : "paused",
      actions: statusActions(actions, focused),
    }
  }

  const menuOffered = library
  if (menuOffered && !menuOpen) targets.push(menuKey)

  const fallback = home.focusOnReturn === "MenuKey" ? "menu"
    : menuOpen ? "menu:FIND"
    : view._tag === "Confirm" && view.declineActionId !== undefined ? `status:${view.declineActionId}`
    : chosenId ?? targets[0]?.id

  return {
    key: `home:${home.mode}:${view._tag}:${menuOpen ? "menu" : ""}`,
    fallback,
    targets,
    ...(chosenId === undefined ? {} : { active: { shelf: chosenId } }),
    view: {
      page: "home",
      frame: homeFrame(
        MODE_LABELS[home.mode],
        korri.clockLabel ?? "",
        view._tag === "Shelf" ? shelfReadout(games, total) : "",
        homeHints(view, menuOffered, menuOpen, home.aim),
        menuOffered,
        focused === "menu",
        menuOpen,
      ),
      // Home draws its card only as its body (PicoHomeLayout).
      card: body === "card" ? shownCard : HIDDEN_CARD,
      home: {
        body,
        mode: home.mode,
        carts,
        selectedIndex: chosen - shelfStart,
        firstIndex: firstIndex + shelfStart,
        total,
        cursorOnShelf: focused !== undefined && focused.startsWith("cart:"),
        narrowestArt: body === "shelf" ? narrowestArt(games) : 1,
        stage,
        invites,
        invitesLabel: `${invites.length} ${invites.length === 1 ? "GAME" : "GAMES"}`,
        menuOpen,
        sections,
        columns: ctx.plan.columns,
        focusRow: focusedRow(targets, focused),
        // A layer not drawn is not sent: the shell keeps a field it is not
        // sent, and the layer is sent whole whenever it shows.
        ...(menuOpen ? { menu: entries.map(entry => button(entry.label, { detail: entry.detail, ...mark(`menu:${entry.label}`, focused) })) } : {}),
        ...(body === "hero" ? { hero } : {}),
        ...(body === "stage" ? { launch } : {}),
      },
    },
  }
}

/** The grid row the cursor is on, for main.slint's scroll. */
function focusedRow(targets: readonly Target<Act>[], focused: string | undefined): number {
  let target: Target<Act> | undefined
  for (const candidate of targets) {
    if (candidate.id === focused) {
      target = candidate
      break
    }
  }
  return target === undefined || target.box.y >= 900 ? 0 : Math.floor(target.box.y / 30)
}

// ---- Find ----

export function findScreen(ctx: Ctx, shown: Extract<PicoShown, { _tag: "Find" }>): Screen {
  const { s, korri, model } = ctx
  const search = shown.search
  const focused = focusId(ctx)
  const targets: Target<Act>[] = []
  const sections = [PICO_ALL_SECTIONS, ...s.sections]
  const sectionButtons = sections.map((section, i) => {
    const id = `section:${section}`
    targets.push({ id, region: "sections", box: { x: i * 10, y: 10, w: 9, h: 8 }, act: [nav({ _tag: "ChoseSection", section })] })
    return button(section.toUpperCase(), { lit: section === search.section, ...mark(id, focused) })
  })
  const orderButtons = PICO_ORDERS.map((order, i) => {
    const id = `order:${order}`
    targets.push({ id, region: "orders", box: { x: i * 10, y: 20, w: 9, h: 8 }, act: [nav({ _tag: "ChoseOrder", order })] })
    return button(PICO_ORDER_LABELS[order], { lit: order === search.order, ...mark(id, focused) })
  })
  const keys = keyboard("search", { capitals: false, symbols: false }, focused, 30, searchKeyAct)
  targets.push(...keys.targets)
  const catalog = korri.catalog
  // Korri answers the query (MoveCatalog); until it has, the window is the
  // last one, and Find shows nothing rather than the wrong games.
  const answered = catalog._tag === "Ready" && sameQuery(catalog.query, findQuery(search))
  const games = answered && model.catalog._tag === "Ready" ? model.catalog.games : []
  const firstIndex = catalog._tag === "Ready" ? catalog.firstIndex : 0
  const focusedResult = resultIndex(games, focused)
  // Only the results the rack can show around the focused one are sent, and
  // are targets, with the one the cursor comes back to (`remembered`).
  const back = resultIndex(games, s.remembered["results"])
  const reach = resultReach(ctx.plan.width)
  const anchor = Math.max(0, focusedResult)
  const first = Math.max(0, Math.min(anchor, back === -1 ? anchor : back) - reach)
  const end = Math.min(games.length, Math.max(anchor, back) + reach + 1)
  const results = games.slice(first, end).map((game, k) => {
    const id = `result:${game.id}`
    targets.push(resultTarget(game, firstIndex + first + k))
    // The game as the shelf holds it, once per game: its carts are kept.
    return resultView(picoShelfGame(game), focused === id)
  })
  return {
    key: "find",
    fallback: "key:0:0",
    targets,
    active: { sections: `section:${search.section}`, orders: `order:${search.order}` },
    view: {
      page: "find",
      frame: findFrame(korri.clockLabel ?? ""),
      // Find draws no card (PicoLibrary).
      card: HIDDEN_CARD,
      find: {
        query: search.query,
        sections: sectionButtons,
        orders: orderButtons,
        keys: keys.keys,
        keyRows: keys.rows,
        results,
        firstResult: first,
        resultCount: games.length,
        focusedResult: anchor,
        empty: answered && games.length === 0,
        count: catalog._tag === "Ready" && answered ? `${catalog.total} ${catalog.total === 1 ? "GAME" : "GAMES"}` : "",
      },
    },
  }
}

/** What Find's keys do. One function, so the keys' targets are kept (keyboard.ts). */
function searchKeyAct(press: KeyPress): Act {
  return press._tag === "Type" ? [nav({ _tag: "TypedCharacter", character: press.character })]
    : press._tag === "Backspace" ? [nav({ _tag: "PressedBackspace" })]
    : [nav({ _tag: "ClearedQuery" })]
}

/** Where the result with target `id` is among `games`, or -1. */
function resultIndex(games: readonly SurfaceGame[], id: string | undefined): number {
  if (id === undefined || !id.startsWith("result:")) return -1
  const gameId = id.slice("result:".length)
  for (let i = 0; i < games.length; i += 1) if (games[i]?.id === gameId) return i
  return -1
}

/**
 * How many results either side of the focused one PicoLibraryBrowser can show
 * on a screen `width` virtual pixels wide (the plan's): the rack centres the
 * focused result, held at either end, and a result is 54 virtual pixels apart.
 */
function resultReach(width: number): number {
  return Math.ceil((width + 1) / 54)
}

/** A result's target, once per game and place. */
function resultTarget(game: SurfaceGame, place: number): Target<Act> {
  let known = RESULT_TARGETS.get(game)
  if (known === undefined) {
    known = new Map()
    RESULT_TARGETS.set(game, known)
  }
  let target = known.get(place)
  if (target === undefined) {
    target = { id: `result:${game.id}`, region: "results", box: { x: place * 10, y: 100, w: 9, h: 20 }, act: [nav({ _tag: "OpenedGame", gameId: game.id })] }
    if (known.size > 8) known.clear()
    known.set(place, target)
  }
  return target
}

const RESULT_TARGETS = new WeakMap<object, Map<number, Target<Act>>>()

interface ResultView {
  readonly cart: CartView
  readonly title: string
}

/** A result's view, once per game and focus. */
function resultView(game: PicoShelfGame, focused: boolean): ResultView {
  let known = RESULT_VIEWS.get(game)
  if (known === undefined) {
    known = [undefined, undefined]
    RESULT_VIEWS.set(game, known)
  }
  const at = focused ? 1 : 0
  let view = known[at]
  if (view === undefined) {
    const id = `result:${game.id}`
    view = { cart: cartView(game, { focused, focusId: id, hero: focused, band: false }), title: game.title }
    known[at] = view
  }
  return view
}

const RESULT_VIEWS = new WeakMap<object, [ResultView | undefined, ResultView | undefined]>()

/** Find's frame, once per clock. */
function findFrame(clock: string): FrameView {
  let built = FIND_FRAMES.get(clock)
  if (built === undefined) {
    built = frame({ label: "FIND", clock, hints: [A("OPEN"), B("BACK")] })
    if (FIND_FRAMES.size > 4) FIND_FRAMES.clear()
    FIND_FRAMES.set(clock, built)
  }
  return built
}

const FIND_FRAMES = new Map<string, FrameView>()

/** The query Korri should answer for Find (contract 3.4). */
export function findQuery(search: { readonly query: string; readonly section: string; readonly order: string }) {
  return {
    text: search.query.trim(),
    ...(search.section === PICO_ALL_SECTIONS ? {} : { section: search.section }),
    order: search.order === "played" ? "mostPlayed" as const
      : search.order === "title" ? "title" as const
      : search.order === "recent" ? "recent" as const
      : "korri" as const,
  }
}

export function sameQuery(
  a: { readonly text: string; readonly section?: string; readonly order: string },
  b: { readonly text: string; readonly section?: string; readonly order: string },
): boolean {
  return a.text === b.text && (a.section ?? "") === (b.section ?? "") && a.order === b.order
}

// ---- A game's own screen ----

export function detailScreen(ctx: Ctx, shown: Extract<PicoShown, { _tag: "Detail" }>): Screen {
  const { korri } = ctx
  const focused = focusId(ctx)
  const game = picoDetailViewFromGame(shown.game)
  const question = shown.detail.question
  const focusedGame = korri.focusedGame
  const actions = focusedGame !== undefined && focusedGame.gameId === shown.game.id ? focusedGame.actions : []
  const targets: Target<Act>[] = []
  let shownCard: CardView = NO_CARD
  if (question._tag === "ChoosingLocation") {
    const locations = question.game.locations ?? []
    const placed = cardTargets(
      card({ tone: "ask", kicker: "PLAY WHERE?", title: question.game.title, actions: locations.map(location => button(location.label)) }),
      locations.map(location => `location:${location.id}`),
      locations.map(location => [nav({ _tag: "ChoseLocation", locationId: location.id })]),
      focused,
    )
    targets.push(...placed.targets)
    shownCard = placed.card
  } else if (question._tag === "ConfirmingAction") {
    const placed = cardTargets(
      card({
        tone: "warn",
        kicker: "ARE YOU SURE?",
        title: `${question.action.label.toUpperCase()}?`,
        message: `${game.title} — this cannot be undone.`,
        actions: [button(question.action.label.toUpperCase(), { warn: true }), button("CANCEL")],
      }),
      ["confirm", "cancel"],
      [[nav({ _tag: "ConfirmedGameAction" })], [nav({ _tag: "CancelledGameAction" })]],
      focused,
    )
    targets.push(...placed.targets)
    shownCard = placed.card
  } else {
    targets.push({ id: "play", region: "play", box: { x: 0, y: 0, w: 30, h: 10 }, act: [nav({ _tag: "PressedPlay" })] })
    actions.forEach((action, i) => {
      if (action.enabled) targets.push({ id: `action:${action.id}`, region: "actions", box: { x: 0, y: 20 + i * 10, w: 100, h: 8 }, act: [nav({ _tag: "PressedGameAction", action })] })
    })
  }
  const stats = factChips({ ...shown.game, ...(game.artUrl === undefined ? {} : { artUrl: game.artUrl }) } as PicoShelfGame)
  return {
    key: `detail:${shown.game.id}:${question._tag}`,
    fallback: question._tag === "ConfirmingAction" ? "cancel" : "play",
    targets,
    view: {
      page: "detail",
      question: question._tag === "ChoosingLocation" ? "location" : question._tag === "ConfirmingAction" ? "modal" : "",
      frame: frame({ place: "LIBRARY", sep: true, label: game.title, clock: korri.clockLabel ?? "", hints: [A("PLAY"), B("BACK")] }),
      card: shownCard,
      detail: {
        stacked: ctx.plan.detailStacked,
        cart: cartView({ ...shown.game, ...(game.artUrl === undefined ? {} : { artUrl: game.artUrl }) }, { hero: true }),
        title: game.title,
        subtitle: [game.subtitle, game.profile].filter(part => part !== undefined && part !== "").join(" · "),
        chips: stats,
        primary: button(game.primaryLabel, { ...mark("play", focused) }),
        bar: game.downloadProgress === undefined ? NO_BAR : bar(game.downloadProgress, game.download ?? ""),
        actions: actions.map(action => button(action.label, {
          detail: action.description ?? "",
          disabled: !action.enabled,
          warn: action.destructive === true,
          ...mark(`action:${action.id}`, focused),
        })),
      },
    },
  }
}

/** The game the cursor is on, for FocusGame and the catalog window. */
export function cursorGame(focused: string | undefined): string | undefined {
  if (focused === undefined) return undefined
  for (const prefix of ["cart:", "result:", "resume:"]) if (focused.startsWith(prefix)) return focused.slice(prefix.length)
  return undefined
}

export { local }
