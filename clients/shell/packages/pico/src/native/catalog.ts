/**
 * The catalog's screens as views and focus targets: home (shelf, grid, hero,
 * MENU, PLAYING NOW, Korri's launch and failure screens), Find and a game's
 * own screen. Ports of PicoHome, PicoLibrary and PicoGameDetail; the words
 * are theirs.
 */
import { PICO_FONTS } from "../pico-fonts"
import type { SurfaceAction } from "../model"
import { picoDetailViewFromGame, picoDownloadLabel, picoStatsFor } from "../pico-detail-view"
import { picoInvitePlaying, picoInvitesFrom, picoInviteWhere } from "../pico-invite-view"
import {
  PICO_ALL_SECTIONS,
  PICO_ORDER_LABELS,
  PICO_ORDERS,
  picoCollectionsFrom,
  picoHeroPick,
} from "../pico-library-view"
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
  narrowestArt,
  art,
  mark,
} from "./kit"
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
  readonly hasCart: boolean
  readonly cart: CartView
  /** "starting", "running" or "paused". */
  readonly phase: string
  readonly actions: readonly ButtonView[]
}

const EMPTY_CART: CartView = cartView({ id: "", title: "" })
const NO_STAGE: StageView = { shown: false, title: "", subtitle: "", kicker: "", chips: [], index: 0, total: 0, cart: EMPTY_CART }

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
  const resumable = games.filter(game => game.resumable === true).length
  const carts = `${total} ${total === 1 ? "CART" : "CARTS"}`
  return resumable === 0 ? carts : `${carts} · ${resumable} RESUMABLE`
}

/** The cart the shelf has chosen: the kept one, else the first. */
export function chosenIndex(games: readonly PicoShelfGame[], selected: string | undefined): number {
  const found = games.findIndex(game => game.id === selected)
  return found === -1 ? 0 : found
}

const statusActions = (actions: readonly SurfaceAction[] | undefined, focused: string | undefined): readonly ButtonView[] =>
  (actions ?? []).map(action => button(action.label, {
    detail: action.description ?? "",
    disabled: !action.enabled,
    ...mark(`status:${action.id}`, focused),
  }))

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

  // MENU's list: FIND, GET GAMES, SETTINGS, VIEW.
  const entries: { readonly label: string; readonly detail: string; readonly act: Act }[] = [
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
  let sections: GridSectionView[] = []
  let hero: HeroView = { title: "", subtitle: "", reason: "", chips: [], open: button("OPEN"), resume: [] }
  let shownCard: CardView = NO_CARD
  let launch: LaunchStageView = { kicker: "", detail: "", bar: NO_BAR, hasCart: false, cart: EMPTY_CART, phase: "starting", actions: [] }
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
    carts = games.map((game, i) => {
      const id = `cart:${game.id}`
      if (!menuOpen) {
        targets.push({
          id,
          region: "shelf",
          box: { x: (firstIndex + i) * 10, y: 500, w: 9, h: 20 },
          act: [nav({ _tag: "OpenedGame", gameId: game.id })],
          enter: [nav({ _tag: "SelectedCart", gameId: game.id }), ...(home.aim === undefined ? [] : aimAt(undefined))],
        })
      }
      return cartView(game, { hero: i === chosen, ...mark(id, focused) })
    })
    if (chosenGame !== undefined) {
      stage = {
        shown: true,
        title: chosenGame.title,
        subtitle: chosenGame.subtitle ?? "",
        kicker: (chosenGame.section ?? "").toUpperCase(),
        chips: factChips(chosenGame),
        index: firstIndex + chosen + 1,
        total,
        cart: cartView(chosenGame, { hero: true }),
      }
    }
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
    sections = picoCollectionsFrom(games).map((collection) => {
      const rowsTop = top + title + 2
      top = rowsTop + Math.ceil(collection.games.length / columns) * 44 + 2 + 2 + 4
      const section = {
        title: collection.title,
        rowStart,
        carts: collection.games.map((game, i) => {
          const id = `cart:${game.id}`
          if (!menuOpen) {
            targets.push({
              id,
              region: "grid", box: { x: (i % columns) * 34, y: rowsTop + Math.floor(i / columns) * 44, w: 30, h: 44 },
              act: [nav({ _tag: "OpenedGame", gameId: game.id })],
              enter: [nav({ _tag: "SelectedCart", gameId: game.id }), ...(home.aim === undefined ? [] : aimAt(undefined))],
            })
          }
          return cartView(game, { ...mark(id, focused), hero: focused === id, band: false })
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
    const resumable = games.filter(game => game.resumable === true)
    if (pick !== undefined && !menuOpen) {
      targets.push({ id: "hero:open", region: "hero", box: { x: 0, y: 100, w: 30, h: 10 }, act: [nav({ _tag: "OpenedGame", gameId: pick.game.id })], enter: aimAt(undefined) })
    }
    resumable.forEach((game, i) => {
      if (!menuOpen) targets.push({ id: `resume:${game.id}`, region: "resume", box: { x: i * 10, y: 300, w: 9, h: 20 }, act: [nav({ _tag: "OpenedGame", gameId: game.id })], enter: aimAt(undefined) })
    })
    hero = pick === undefined ? hero : {
      title: pick.game.title,
      subtitle: pick.game.subtitle ?? "",
      reason: pick.reason ?? "",
      chips: factChips(pick.game),
      open: button("OPEN", { ...mark("hero:open", focused) }),
      resume: resumable.map(game => ({
        cart: cartView(game, { ...mark(`resume:${game.id}`, focused), hero: focused === `resume:${game.id}`, band: false }),
        title: game.title,
        subtitle: game.subtitle ?? "",
      })),
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
      // A Busy with no progress keeps the clock meter: every cell lit.
      bar: view._tag === "Busy" ? (progress === undefined ? { ...NO_BAR, shown: true, cells: 16 } : bar(progress)) : NO_BAR,
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
      frame: frame({
        label: MODE_LABELS[home.mode],
        clock: korri.clockLabel ?? "",
        readout: view._tag === "Shelf" ? shelfReadout(games, total) : "",
        hints: homeHints(view, menuOffered, menuOpen, home.aim),
        hasLead: menuOffered,
        lead: button(MENU_LABEL, { ...mark("menu", focused), lit: menuOpen }),
      }),
      card: shownCard,
      home: {
        body,
        mode: home.mode,
        carts,
        selectedIndex: chosen,
        firstIndex,
        total,
        cursorOnShelf: focused !== undefined && focused.startsWith("cart:"),
        narrowestArt: narrowestArt(carts),
        stage,
        invites,
        invitesLabel: `${invites.length} ${invites.length === 1 ? "GAME" : "GAMES"}`,
        menuOpen,
        menu: entries.map(entry => button(entry.label, { detail: entry.detail, ...mark(`menu:${entry.label}`, focused) })),
        sections,
        columns: ctx.plan.columns,
        focusRow: focusedRow(targets, focused),
        hero,
        launch,
      },
    },
  }
}

/** The grid row the cursor is on, for main.slint's scroll. */
function focusedRow(targets: readonly Target<Act>[], focused: string | undefined): number {
  const target = targets.find(candidate => candidate.id === focused)
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
  const keys = keyboard("search", { capitals: false, symbols: false }, focused, 30, (press: KeyPress): Act =>
    press._tag === "Type" ? [nav({ _tag: "TypedCharacter", character: press.character })]
      : press._tag === "Backspace" ? [nav({ _tag: "PressedBackspace" })]
      : [nav({ _tag: "ClearedQuery" })])
  targets.push(...keys.targets)
  const catalog = korri.catalog
  // Korri answers the query (MoveCatalog); until it has, the window is the
  // last one, and Find shows nothing rather than the wrong games.
  const answered = catalog._tag === "Ready" && sameQuery(catalog.query, findQuery(search))
  const games = answered && model.catalog._tag === "Ready" ? model.catalog.games : []
  const firstIndex = catalog._tag === "Ready" ? catalog.firstIndex : 0
  const results = games.map(game => {
    const id = `result:${game.id}`
    targets.push({ id, region: "results", box: { x: (firstIndex + games.indexOf(game)) * 10, y: 100, w: 9, h: 20 }, act: [nav({ _tag: "OpenedGame", gameId: game.id })] })
    const shelf: PicoShelfGame = {
      id: game.id,
      title: game.title,
      ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
      ...(game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl }),
      ...(game.resumable === undefined ? {} : { resumable: game.resumable }),
      ...(game.playCount === undefined ? {} : { playCount: game.playCount }),
      ...(game.totalPlaytimeSeconds === undefined ? {} : { totalPlaytimeSeconds: game.totalPlaytimeSeconds }),
    }
    return { cart: cartView(shelf, { ...mark(id, focused), hero: focused === id, band: false }), title: game.title }
  })
  const focusedResult = results.findIndex(result => focused === `result:${result.cart.id}`)
  return {
    key: "find",
    fallback: "key:0:0",
    targets,
    active: { sections: `section:${search.section}`, orders: `order:${search.order}` },
    view: {
      page: "find",
      frame: frame({ label: "FIND", clock: korri.clockLabel ?? "", hints: [A("OPEN"), B("BACK")] }),
      card: NO_CARD,
      find: {
        query: search.query,
        sections: sectionButtons,
        orders: orderButtons,
        keys: keys.keys,
        keyRows: keys.rows,
        results,
        focusedResult: Math.max(0, focusedResult),
        empty: answered && results.length === 0,
        count: catalog._tag === "Ready" && answered ? `${catalog.total} ${catalog.total === 1 ? "GAME" : "GAMES"}` : "",
      },
    },
  }
}

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
    targets.push({ id: "play", region: "actions", box: { x: 0, y: 0, w: 30, h: 10 }, act: [nav({ _tag: "PressedPlay" })] })
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
