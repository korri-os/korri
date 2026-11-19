/**
 * The view: one object for main.slint's properties, a pure function of the
 * state and Korri's model. It sends only what is on screen: the tiles near
 * the window, one marquee, the rows of the one page shown. A part that is
 * not shown is `{ shown: false }` and its lists are empty.
 *
 * Every list is a property of its own (`homeRow`, `listRows`, `keys`, ...),
 * never a field of a struct: the shell updates a list's rows in place, and
 * a list inside a struct that changes with it can miss a change that comes
 * twice before a frame (ui/types.slint).
 *
 * Focus targets carry `focused`, `focusId` and `focusRegion`, and the view
 * names each region's exits in `focusPlan` (contract 8.4). Only the top
 * layer holds focus; `layer` tells main.slint which that is, so everything
 * under it is drawn inert (no target, no tap).
 */
import type { SurfaceModel as Model } from "../../../../../contracts/generated/surface"
import { gameCard, marqueeOf, NO_IMAGE, NO_MARQUEE, pictures, stageOf, type Pictures, type StageView } from "./art"
import { detailGame, detailView, focusedGame, homeExits, homeView, NO_PILL, pill, setExits, setView, type PillView } from "./browse"
import { curtainLayout, curtainOf } from "./curtain"
import { breakLines, fitCaps } from "./fit"
import { keyGrid } from "./keys"
import { gameById, homeOf } from "./lists"
import { entryHeading, screenPanel, sheetPanel, shown } from "./pages"
import { QUIET, qrPath } from "./steam"
import { plan, type Plan } from "./plan"
import type { PanelSpec, RowSpec, State } from "./types"
import { emptyAt, emptyButtons, panelFocus, panelStacks, split, takesFocus, topLayer } from "./update"
import { caps } from "./words"

interface RowView {
  readonly kind: string
  readonly label: string
  readonly value: string
  readonly note: string
  readonly mark: string
  readonly bars: number
  readonly dim: boolean
  readonly danger: boolean
  readonly share: number
  readonly focusId: string
  readonly focused?: boolean
  readonly focusRegion?: string
}


function rowView(row: RowSpec, region: string, focus: string, live: boolean): RowView {
  const base = {
    kind: row.kind, label: row.label, value: row.value ?? "", note: row.note ?? "", mark: row.mark ?? "none", signal: row.bars !== undefined, bars: row.bars ?? 0,
    dim: row.dim === true, danger: row.danger === true, share: row.share ?? 0, tier: row.tier ?? "",
  }
  return takesFocus(row) ? { ...base, focusId: row.id, focused: live && row.id === focus, focusRegion: region } : { ...base, focusId: "" }
}

const NO_SHEET = { shown: false }
const NO_PAGE = { shown: false }
const NO_KEYBOARD = { shown: false }
const NO_CURTAIN = { shown: false }
const NO_NOTICE = { shown: false }
const OFF = { shown: false }
const NONE: readonly never[] = []

function sheetRows(panel: PanelSpec, state: State, live: boolean): RowView[] {
  const focus = panelFocus(panel, state.panels[panel.key]).focus
  return panel.rows.map(row => rowView(row, "sheet", focus, live))
}


function pageView(asked: Pictures, panel: PanelSpec, state: State, p: Plan, live: boolean) {
  const focus = panelFocus(panel, state.panels[panel.key]).focus
  const side = panel.side
  const game = side?.kind === "tile" ? side.game : undefined
  return {
    list: {
      shown: true,
      key: panel.key,
      title: caps(panel.title),
      sub: panel.sub ?? "",
      step: panel.step ?? 0,
      steps: panel.steps ?? 0,
      stack: panelStacks(p),
      panel: side?.kind ?? "none",
      panelTitle: side?.title ?? "",
    },
    rows: panel.rows.map(row => rowView(row, "rows", focus, live)),
    // Down from the rows lands on the first button that can be pressed (a dimmed Next waits
    // beside Skip): that one is the region's active entry, and the rows' exit says so.
    buttons: panel.buttons.map((button): PillView => ({
      ...pill(button.label, "buttons", button.id, live && button.id === focus, {
        icon: button.icon ?? "", quiet: button.quiet === true, dim: button.dim === true, danger: button.danger === true, note: "",
      }),
      focusActive: button === (panel.buttons.find(candidate => candidate.dim !== true) ?? panel.buttons[0]),
    } as PillView)),
    // A code's words stand beside it or under it, at a width the view decides: they wrap there.
    panelLines: side?.kind === "qr" ? side.lines ?? [] : (side?.lines ?? []).flatMap(line => breakLines(line.length > 60 && !line.includes(" ") ? chunk(line, 28) : line, p.fact, p.panelW)),
    tiles: game === undefined ? [] : [gameCard(asked, game)],
    qr: side?.kind === "qr" && side.qr !== undefined ? qrPath(side.qr) : "",
    qrSize: side?.kind === "qr" && side.qr !== undefined ? side.qr.size + 2 * QUIET : 0,
  }
}

/** A long word with no break in groups, so it wraps where a person can keep their place. */
function chunk(text: string, size: number): string {
  const parts: string[] = []
  for (let i = 0; i < text.length; i += size) parts.push(text.slice(i, i + size))
  return parts.join("\n")
}

function panelExits(panel: PanelSpec): Record<string, Record<string, { to: string; entry: string }>> {
  const rows = panel.rows.some(takesFocus)
  return rows && panel.buttons.length > 0 ? { rows: { down: { to: "buttons", entry: "active" } }, buttons: { up: { to: "rows", entry: "remembered" } } } : {}
}

function keyboardView(state: State, live: boolean) {
  const keyboard = state.keyboard
  if (keyboard === undefined) return { keyboard: NO_KEYBOARD, keys: [] }
  const heading = entryHeading(keyboard.entry)
  const grid = keyGrid(keyboard.layer, heading.clearLabel)
  return {
    keyboard: {
      shown: true,
      heading: heading.title,
      name: heading.name ?? "",
      before: heading.secret ? "•".repeat([...keyboard.draft].length) : keyboard.draft,
      after: "",
      placeholder: keyboard.draft === "" ? heading.placeholder ?? "" : "",
      note: heading.note ?? "",
    },
    keys: grid.map(key => ({
      focusId: key.id, focused: live && key.id === keyboard.key, focusRegion: "keys",
      label: key.label, icon: key.icon, column: key.column, row: key.row, span: key.span,
      lit: key.key.kind === "done", quiet: key.key.kind !== "char",
    })),
  }
}

function curtainView(asked: Pictures, state: State, model: Model, p: Plan) {
  const spec = curtainOf(state, model)
  if (spec === undefined) return { curtain: NO_CURTAIN, kicker: [], lines: [], steps: [], buttons: [] }
  // A game on its way: its mark as the prototype's launch sets it, the logo when the game
  // has one, else its title at the pick's size, in three fifths of the width.
  const launch = spec.kind === "busy" && spec.game !== undefined
  const mark = launch
    ? marqueeOf(asked, spec.game!, { width: Math.min(p.width * 0.6, p.width - 2 * p.padX), logoH: Math.round(Math.min(220, Math.max(56, p.height * 0.2))), title: p.title, fact: p.fact, mark: true, badge: false }, model.publishedAt, [])
    : undefined
  const layout = curtainLayout(spec, p)
  const line = Math.max(0, Math.min(state.curtain.line, Math.max(0, layout.lines.length - layout.room)))
  const steps = spec.steps
  const at = Math.max(0, Math.min(state.curtain.at, spec.buttons.length - 1))
  const kicker = mark !== undefined
    ? { size: mark.size, lines: mark.logoShown ? [] : [mark.line1, mark.line2, mark.line3].slice(0, mark.lines) }
    : fitCaps(spec.kicker, p.curtainW, p.kicker * 3, p.kicker, Math.min(p.kicker, 18))
  const now = steps === undefined ? 0 : Math.max(0, Math.min(steps.current, steps.labels.length - 1))
  return {
    curtain: {
      shown: true,
      kind: spec.kind,
      kickerSize: kicker.size,
      name: spec.name ?? "",
      moreUp: line > 0,
      moreDown: line + layout.room < layout.lines.length,
      stepLine: steps === undefined ? "" : `Step ${now + 1} of ${steps.labels.length} · ${steps.labels[now] ?? ""}`,
      // The union, as plain properties for the view: its kind, and the share when it is measured.
      progress: spec.progress.kind,
      progressShare: spec.progress.kind === "measured" ? spec.progress.fraction : 0,
      // Korri's own words about the progress stand; Velvet adds its count only when Korri says nothing.
      progressLine: spec.words.length > 0 ? "" : spec.progressLine ?? "",
      stack: layout.stack,
      // A game on its way: black, as the prototype's launch curtain; anything else dims the screen.
      solid: launch,
      logoShown: mark?.logoShown ?? false,
      logo: mark?.logoShown === true ? mark.logo : NO_IMAGE,
      logoW: mark?.logoShown === true ? mark.logoW : 0,
      logoH: mark?.logoShown === true ? mark.logoH : 0,
    },
    kicker: kicker.lines,
    lines: layout.lines.slice(line, line + layout.room),
    steps: steps === undefined ? [] : steps.labels.map((label, index) => ({ label, state: index < steps.current ? "done" : index === steps.current ? "now" : "next" })),
    buttons: spec.buttons.map((button, index): PillView => pill(button.label, "curtain", `x:${index}`, index === at, {
      note: button.note ?? "", quiet: !layout.stack && index !== at, dim: button.dim === true, danger: button.danger === true,
    })),
  }
}

function noticeView(state: State, model: Model) {
  const invite = state.notice === undefined ? undefined : (model.invites ?? []).find(candidate => candidate.id === state.notice!.id)
  if (invite === undefined) return NO_NOTICE
  return { shown: true, title: `${invite.personName} plays ${invite.gameTitle}`, words: `On ${invite.deviceLabel}. Open the menu to join.` }
}

function emptyView(state: State, model: Model, p: Plan, live: boolean) {
  const catalog = model.catalog
  const buttons = emptyButtons(model)
  const at = emptyAt(state, model)
  const title = catalog._tag === "Error" ? "The library did not open" : catalog._tag === "Empty" ? "The stage is yours" : ""
  const words = catalog._tag === "Error" ? catalog.message
    : catalog._tag === "Empty" ? "Bring your first games in through a plugin. Once you play, the game you come back to waits for you right here."
      : ""
  const width = p.emptyWide ? p.width * 0.44 : Math.min(p.width - 2 * p.padX, 900)
  const fit = fitCaps(title, width, p.title * 2.2, Math.min(p.title, p.height * 0.11), 20)
  return {
    empty: {
      shown: true,
      titleSize: fit.size,
      stack: p.stack,
      menu: pill("Menu", "menu", "menu", live && (state.onMenu || buttons.length === 0), { icon: "menu", quiet: true }),
      clock: model.clockLabel ?? "",
      ghosts: catalog._tag === "Empty",
    },
    title: title === "" ? [] : fit.lines,
    lines: breakLines(words, p.body, Math.min(width, p.body * 26)).filter(line => line !== ""),
    buttons: buttons.map((button, index): PillView => pill(button.label, "empty", button.id, live && !state.onMenu && index === at, { icon: button.icon, quiet: index > 0 })),
  }
}

/** The plan as main.slint's Screen. */
const screenOf = (p: Plan) => p

type Size = { readonly width: number; readonly height: number }

/**
 * What one window draws of the house, when the Stage and the Floor have a
 * window each (ADR 0086). Absent: the whole house, as on one display.
 */
interface Part {
  /** Focus is in the other window: the page is drawn, and nothing in it takes focus or a tap. */
  readonly elsewhere?: boolean
  /** No target drawn focused, but a tap still lands (the Floor while Play on the Stage holds focus). */
  readonly unfocused?: boolean
  /** The stage light under at least this veil: the Floor's light is the Stage's, dimmer. */
  readonly veil?: number
  /** Home's pick alone: the row, its shelves and the lobby are the Floor's. */
  readonly pickOnly?: boolean
  /** A list page's side content alone (the backup's QR code): its rows are the Floor's. */
  readonly sideOnly?: boolean
  /** No curtain, keyboard, sheet or notice: they are the other window's. */
  readonly layersOff?: boolean
  /** No page: the stage light alone. */
  readonly blank?: boolean
}

/** The Floor's stage light: the Stage's own, under this veil at least. */
export const FLOOR_VEIL = 0.62

/**
 * `view(state, model, window)`. On one window, the whole house as it has
 * always been drawn. With a window each (ADR 0086), window 0 draws the Stage
 * and window 1 the Floor; one focus moves across both.
 */
export function view(state: State, model: Model, window = 0) {
  if (!split(model)) return whole(state, model, state.size, {})
  return window === 0 ? stageWindow(state, model) : floorWindow(state, model)
}

/** The Stage's window: the lit game, the detail and Korri's status. */
function stageWindow(state: State, model: Model) {
  const size = state.size
  const top = topLayer(state, model)
  const what = shown(state, model)
  // Korri's status and Velvet's questions take the Stage, over the page they came from.
  if (top === "curtain") return whole(state, model, size, { pickOnly: true, sideOnly: true })
  const library = what === "home" || what === "set" || what === "detail"
  const lists = homeOf(state.lists, model.publishedAt)
  // The detail, with its own sheet or keyboard over it.
  if (what === "detail") return whole(state, model, size, {})
  if (what === "home" && (lists.pick === undefined || (state.depth === 0 && top === "screen"))) {
    return whole(state, model, size, { pickOnly: true })
  }
  // The Floor holds focus: the Stage shows the game in front of the person, or a page's
  // own side content (the backup's QR code), or the last lit game under a veil.
  const panel = library ? undefined : screenPanel(state, model)
  if (panel?.side !== undefined) return whole(state, model, size, { elsewhere: true, sideOnly: true, layersOff: true })
  const lead = focusedGame(state, model) ?? (state.reported === null ? lists.pick : gameById(state.lists, model, state.reported) ?? lists.pick)
  if (lead === undefined) return whole(state, model, size, { elsewhere: true, blank: true, layersOff: true })
  const cinema: State = { ...state, detail: { game: lead, at: 0 }, pages: [], sheet: undefined, keyboard: undefined }
  return whole(cinema, model, size, { elsewhere: true, layersOff: true, veil: library ? 0 : 0.86 })
}

/** The Floor's window: home's row and lobby, sets, list pages, sheets and the keyboard. */
function floorWindow(state: State, model: Model) {
  const size = state.floor ?? state.size
  const top = topLayer(state, model)
  const what = shown(state, model)
  const lists = homeOf(state.lists, model.publishedAt)
  if (what === "home" && lists.pick === undefined) return whole(state, model, size, { elsewhere: true, blank: true, layersOff: true, veil: FLOOR_VEIL })
  // The detail opens on the Stage: the Floor keeps the row or the set it came from.
  const behind: State = what === "detail" ? { ...state, detail: undefined } : state
  // Home on the Floor is its row and lobby: one level down, where the pick is out of sight.
  const floor: State = shown(behind, model) === "home" && behind.depth === 0 ? { ...behind, depth: 1 } : behind
  const onStage = top === "curtain" || what === "detail"
  return whole(floor, model, size, {
    elsewhere: onStage,
    unfocused: !onStage && what === "home" && state.depth === 0,
    layersOff: onStage,
    veil: FLOOR_VEIL,
  })
}

/** Every focus region a view draws a target in. */
function regionsOf(value: unknown, into: Set<string>): Set<string> {
  if (Array.isArray(value)) {
    for (const item of value) regionsOf(item, into)
  } else if (value !== null && typeof value === "object") {
    const entry = value as Record<string, unknown>
    if ("focused" in entry && typeof entry["focusRegion"] === "string") into.add(entry["focusRegion"] as string)
    for (const key in entry) if (key !== "focusPlan") regionsOf(entry[key], into)
  }
  return into
}

/** A window's exits lead only to regions it draws: the other window's are its own (contract 8.4). */
function exitsWithin(exits: Record<string, Record<string, { to: string; entry: string }>>, drawn: Set<string>) {
  const kept: Record<string, Record<string, { to: string; entry: string }>> = {}
  for (const region in exits) {
    if (!drawn.has(region)) continue
    const ways: Record<string, { to: string; entry: string }> = {}
    for (const way in exits[region]) if (drawn.has(exits[region]![way]!.to)) ways[way] = exits[region]![way]!
    if (Object.keys(ways).length > 0) kept[region] = ways
  }
  return kept
}

function whole(state: State, model: Model, size: Size, part: Part) {
  // What this view asks for in pictures: the stage first, then each part in its own order.
  const asked = pictures()
  const p = plan(size.width, size.height)
  const top = topLayer(state, model)
  const live = top === "screen" && part.elsewhere !== true && part.unfocused !== true
  // A launch's curtain is black and holds the game's logo: it asks for its picture first.
  const curtain = curtainView(asked, state, model, p)
  const what = shown(state, model)
  const panel = screenPanel(state, model)
  const lists = homeOf(state.lists, model.publishedAt)
  const library = what === "home" || what === "set" || what === "detail" || what === "overlay"
  const page = part.blank === true ? "none" : !library ? "list" : what === "detail" ? "detail" : what === "set" ? "set" : lists.pick === undefined ? "empty" : "home"
  // The stage: lit by the game in front of the person; dimmer the further from it.
  const lead = what === "detail" ? detailGame(state, model) : focusedGame(state, model) ?? (state.reported === null ? lists.pick : gameById(state.lists, model, state.reported) ?? lists.pick)
  const veil = Math.max(part.veil ?? 0, page === "list" ? 0.86 : page === "set" ? 0.8 : page === "home" && state.depth === 2 ? 0.78 : page === "home" && state.depth === 1 ? 0.3 : 0)
  // The keyboard and the curtain take the house: its lights go down further.
  const taken = top === "curtain" || top === "keyboard"
  const stage: StageView = page === "empty" || (page === "none" && lead === undefined) ? stageOf(asked, undefined, part.veil ?? 0, false) : stageOf(asked, lead, taken ? Math.max(veil, 0.84) : veil, state.drift && top === "screen" && (page === "home" || page === "detail"), page === "detail" || page === "home", page === "set" || page === "list" || (page === "home" && state.depth === 2), p.width, p.height)
  const sheet = state.sheet === undefined || part.layersOff === true ? undefined : sheetPanel(state.sheet.kind, state, model)
  const exits = top !== "screen" || part.elsewhere === true ? {}
    : page === "home" ? homeExits(state, model)
      : page === "empty" ? (emptyButtons(model).length > 0 ? { menu: { down: { to: "empty", entry: "remembered" } }, empty: { up: { to: "menu", entry: "first" } } } : {})
        : page === "set" && state.set !== undefined ? setExits(state.set, model)
          : page === "list" && panel !== undefined ? panelExits(panel) : {}
  const home = page === "home" ? homeView(asked, state, model, p, live) : undefined
  const set = page === "set" && state.set !== undefined ? setView(asked, state, state.set, model, p, live) : undefined
  const detail = page === "detail" ? detailView(asked, state, model, p, live) : undefined
  const empty = page === "empty" ? emptyView(state, model, p, live) : undefined
  const list = page === "list" && panel !== undefined ? pageView(asked, panel, state, p, live) : undefined
  const keyboard = part.layersOff === true ? { keyboard: NO_KEYBOARD, keys: [] } : keyboardView(state, top === "keyboard")
  const pickOnly = part.pickOnly === true
  const sideOnly = part.sideOnly === true
  const drawn = {
    screen: screenOf(p),
    still: state.still,
    touching: state.touched,
    // A finger's way back, where there is a level to go back to. A window whose focus is
    // in the other window draws its page, inert (`elsewhere`), under no layer of its own.
    layer: part.elsewhere === true || (part.layersOff === true && top !== "curtain") ? "screen" : top,
    elsewhere: part.elsewhere === true,
    page,
    stage,
    home: home === undefined ? OFF : pickOnly ? { ...home.home, depth: 0 } : home.home,
    homeCards: home?.cards ?? NONE,
    homeRow: pickOnly ? NONE : home?.row ?? NONE,
    shelves: pickOnly ? NONE : home?.shelves ?? NONE,
    shelfTiles: pickOnly ? NONE : home?.shelfTiles ?? NONE,
    // The focused shelf stands alone too, for the lobby's track (see `setFirstRow`).
    shelfAt: home?.home.shelfAt ?? 0,
    // And how deep home is: the page, the row's tiles and the lobby all move by it.
    homeDepth: pickOnly ? 0 : home?.home.depth ?? 0,
    set: set?.set ?? OFF,
    setTiles: set?.tiles ?? NONE,
    // The row a set's window starts at stands alone: a number the shell sets only when it
    // changes. Inside `set` it would be set again with every view, and the grid's
    // moving track would start its motion again (README, "What the interpreter taught us").
    setFirstRow: set?.set.firstRow ?? 0,
    setMarks: set?.marks ?? NONE,
    detail: detail?.detail ?? OFF,
    detailCards: detail?.cards ?? NONE,
    detailButtons: detail?.buttons ?? NONE,
    empty: empty?.empty ?? OFF,
    emptyTitle: empty?.title ?? NONE,
    emptyLines: empty?.lines ?? NONE,
    emptyButtons: empty?.buttons ?? NONE,
    list: list?.list ?? NO_PAGE,
    listRows: sideOnly ? NONE : list?.rows ?? NONE,
    listButtons: sideOnly ? NONE : list?.buttons ?? NONE,
    listPanelLines: list?.panelLines ?? NONE,
    listTiles: list?.tiles ?? NONE,
    // The code changes about every 20 s; top-level, so the shell sets it only then.
    listQr: list?.qr ?? "",
    listQrSize: list?.qrSize ?? 0,
    sheet: sheet === undefined ? NO_SHEET : { shown: true, title: sheet.title },
    sheetRows: sheet === undefined ? NONE : sheetRows(sheet, state, top === "sheet"),
    keyboard: keyboard.keyboard,
    keys: keyboard.keys,
    curtain: part.layersOff === true ? NO_CURTAIN : curtain.curtain,
    curtainKicker: part.layersOff === true ? NONE : curtain.kicker,
    curtainLines: part.layersOff === true ? NONE : curtain.lines,
    curtainSteps: part.layersOff === true ? NONE : curtain.steps,
    curtainButtons: part.layersOff === true ? NONE : curtain.buttons,
    notice: part.layersOff === true ? NO_NOTICE : noticeView(state, model),
    focusPlan: exits,
  }
  // With a window each, a window's exits lead only to what it draws itself.
  if (part.pickOnly === true || part.sideOnly === true || part.unfocused === true || part.veil !== undefined) {
    return { ...drawn, focusPlan: exitsWithin(exits, regionsOf(drawn, new Set())) }
  }
  return drawn
}

/** The gameplay overlay: Korri's controls over the running game, and Korri's status over those. */
export function overlayView(state: State, model: Model) {
  const p = plan(state.size.width, state.size.height)
  const top = topLayer(state, model)
  const overlay = model.presentation.kind === "gameplay-overlay" ? model.presentation : undefined
  const panel = overlay === undefined ? undefined : screenPanel(state, model)
  const focus = panel === undefined ? "" : panelFocus(panel, state.panels[panel.key]).focus
  const health = overlay?.streamHealth
  const keyboard = keyboardView(state, top === "keyboard")
  const curtain = curtainView(pictures(), state, model, p)
  // Value first, its name under it (owner, 2026-11-13: the overlay's head, version h2):
  // the number is what a glance at the stream's health looks for.
  const readouts = [
    ...(overlay?.batteryPercent === undefined ? [] : [{ value: `${Math.round(overlay.batteryPercent)}%`, label: "Battery" }]),
    ...(health?.rttMs === undefined ? [] : [{ value: `${Math.round(health.rttMs)} ms`, label: "Latency" }]),
    ...(health?.deliveredBitrateKbps === undefined ? [] : [{ value: health.deliveredBitrateKbps >= 1000 ? `${(health.deliveredBitrateKbps / 1000).toFixed(1)} Mbit/s` : `${Math.round(health.deliveredBitrateKbps)} kbit/s`, label: "Stream" }]),
    ...(health?.framesDropped === undefined ? [] : [{ value: String(health.framesDropped), label: health.framesDropped === 1 ? "Frame dropped" : "Frames dropped" }]),
    ...((overlay?.streamPlayers ?? []).length === 0 ? [] : [{ value: (overlay?.streamPlayers ?? []).join(", "), label: "Playing" }]),
  ]
  // The title shares its line with the clock at the right.
  const titleWidth = p.pauseW * 0.8 - (model.clockLabel === undefined || model.clockLabel === "" ? 0 : p.fact * 4.6)
  return {
    screen: screenOf(p),
    still: state.still,
    touching: state.touched,
    layer: top,
    titleLines: fitCaps(overlay?.title ?? "Paused", titleWidth, p.head * 2.2, p.head, 16).lines,
    titleSize: fitCaps(overlay?.title ?? "Paused", titleWidth, p.head * 2.2, p.head, 16).size,
    clock: model.clockLabel ?? "",
    readouts,
    rows: panel === undefined ? [] : panel.rows.map(row => rowView(row, "rows", focus, top === "screen")),
    leaving: state.overlay.leaving,
    keyboard: keyboard.keyboard,
    keys: keyboard.keys,
    curtain: curtain.curtain,
    curtainKicker: curtain.kicker,
    curtainLines: curtain.lines,
    curtainSteps: curtain.steps,
    curtainButtons: curtain.buttons,
    focusPlan: {},
  }
}

export { NO_MARQUEE, NO_PILL }
