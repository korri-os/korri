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
import { plan, type Plan } from "./plan"
import { qrDataUrl } from "./qr"
import type { PanelSpec, RowSpec, State } from "./types"
import { emptyAt, emptyButtons, panelFocus, panelStacks, takesFocus, topLayer } from "./update"
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
    dim: row.dim === true, danger: row.danger === true, share: row.share ?? 0,
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
      qr: side?.qr === undefined ? NO_IMAGE : { $image: qrDataUrl(side.qr) },
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
    panelLines: (side?.lines ?? []).flatMap(line => breakLines(line.length > 60 && !line.includes(" ") ? chunk(line, 28) : line, p.fact, p.panelW)),
    tiles: game === undefined ? [] : [gameCard(asked, game)],
  }
}

/** A long word with no break (a backup's secret) in groups, so it wraps where a person can keep their place. */
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
    ? marqueeOf(asked, spec.game!, { width: p.width * 0.6, logoH: Math.round(Math.min(220, Math.max(56, p.height * 0.2))), title: p.title, fact: p.fact, mark: true, badge: false }, model.publishedAt, [])
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

export function view(state: State, model: Model) {
  // What this view asks for in pictures: the stage first, then each part in its own order.
  const asked = pictures()
  const p = plan(state.size.width, state.size.height)
  const top = topLayer(state, model)
  const live = top === "screen"
  // A launch's curtain is black and holds the game's logo: it asks for its picture first.
  const curtain = curtainView(asked, state, model, p)
  const what = shown(state, model)
  const panel = screenPanel(state, model)
  const lists = homeOf(state.lists, model.publishedAt)
  const library = what === "home" || what === "set" || what === "detail" || what === "overlay"
  const page = !library ? "list" : what === "detail" ? "detail" : what === "set" ? "set" : lists.pick === undefined ? "empty" : "home"
  // The stage: lit by the game in front of the person; dimmer the further from it.
  const lead = what === "detail" ? detailGame(state, model) : focusedGame(state, model) ?? (state.reported === null ? lists.pick : gameById(state.lists, model, state.reported) ?? lists.pick)
  const veil = page === "list" ? 0.86 : page === "set" ? 0.8 : page === "home" && state.depth === 2 ? 0.78 : page === "home" && state.depth === 1 ? 0.3 : 0
  // The keyboard and the curtain take the house: its lights go down further.
  const taken = top === "curtain" || top === "keyboard"
  const stage: StageView = page === "empty" ? stageOf(asked, undefined, 0, false) : stageOf(asked, lead, taken ? Math.max(veil, 0.84) : veil, state.drift && top === "screen" && (page === "home" || page === "detail"), page === "detail" || page === "home", page === "set" || page === "list" || (page === "home" && state.depth === 2), p.width, p.height)
  const sheet = state.sheet === undefined ? undefined : sheetPanel(state.sheet.kind, state, model)
  const exits = top !== "screen" ? {}
    : page === "home" ? homeExits(state, model)
      : page === "empty" ? (emptyButtons(model).length > 0 ? { menu: { down: { to: "empty", entry: "remembered" } }, empty: { up: { to: "menu", entry: "first" } } } : {})
        : page === "set" && state.set !== undefined ? setExits(state.set, model)
          : page === "list" && panel !== undefined ? panelExits(panel) : {}
  const home = page === "home" ? homeView(asked, state, model, p, live) : undefined
  const set = page === "set" && state.set !== undefined ? setView(asked, state, state.set, model, p, live) : undefined
  const detail = page === "detail" ? detailView(asked, state, model, p, live) : undefined
  const empty = page === "empty" ? emptyView(state, model, p, live) : undefined
  const list = page === "list" && panel !== undefined ? pageView(asked, panel, state, p, live) : undefined
  const keyboard = keyboardView(state, top === "keyboard")
  return {
    screen: screenOf(p),
    still: state.still,
    touching: state.touched,
    // A finger's way back, where there is a level to go back to.
    wayBack: state.touched && (page === "set" || page === "detail" || page === "list" || (page === "home" && state.depth > 0)),
    layer: top,
    page,
    stage,
    home: home?.home ?? OFF,
    homeCards: home?.cards ?? NONE,
    homeRow: home?.row ?? NONE,
    shelves: home?.shelves ?? NONE,
    shelfTiles: home?.shelfTiles ?? NONE,
    // The focused shelf stands alone too, for the lobby's track (see `setFirstRow`).
    shelfAt: home?.home.shelfAt ?? 0,
    // And how deep home is: the page, the row's tiles and the lobby all move by it.
    homeDepth: home?.home.depth ?? 0,
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
    listRows: list?.rows ?? NONE,
    listButtons: list?.buttons ?? NONE,
    listPanelLines: list?.panelLines ?? NONE,
    listTiles: list?.tiles ?? NONE,
    sheet: sheet === undefined ? NO_SHEET : { shown: true, title: sheet.title },
    sheetRows: sheet === undefined ? NONE : sheetRows(sheet, state, top === "sheet"),
    keyboard: keyboard.keyboard,
    keys: keyboard.keys,
    curtain: curtain.curtain,
    curtainKicker: curtain.kicker,
    curtainLines: curtain.lines,
    curtainSteps: curtain.steps,
    curtainButtons: curtain.buttons,
    notice: noticeView(state, model),
    focusPlan: exits,
  }
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
  const facts = [
    ...(overlay?.batteryPercent === undefined ? [] : [{ icon: "note", text: `Battery ${Math.round(overlay.batteryPercent)}%`, strong: false }]),
    ...(health?.rttMs === undefined ? [] : [{ icon: "wifi", text: `${Math.round(health.rttMs)} ms`, strong: false }]),
    ...(health?.deliveredBitrateKbps === undefined ? [] : [{ icon: "down", text: health.deliveredBitrateKbps >= 1000 ? `${(health.deliveredBitrateKbps / 1000).toFixed(1)} Mbit/s` : `${Math.round(health.deliveredBitrateKbps)} kbit/s`, strong: false }]),
    ...(health?.framesDropped === undefined ? [] : [{ icon: "note", text: health.framesDropped === 1 ? "1 frame dropped" : `${health.framesDropped} frames dropped`, strong: false }]),
    ...((overlay?.streamPlayers ?? []).length === 0 ? [] : [{ icon: "person", text: (overlay?.streamPlayers ?? []).join(", "), strong: false }]),
  ]
  return {
    screen: screenOf(p),
    still: state.still,
    touching: state.touched,
    layer: top,
    titleLines: fitCaps(overlay?.title ?? "Paused", p.pauseW * 0.8, p.head * 2.2, p.head, 16).lines,
    titleSize: fitCaps(overlay?.title ?? "Paused", p.pauseW * 0.8, p.head * 2.2, p.head, 16).size,
    clock: model.clockLabel ?? "",
    facts,
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
