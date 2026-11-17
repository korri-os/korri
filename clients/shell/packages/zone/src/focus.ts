/**
 * Focus regions and exits (contract 8.4), and the plan's decisions .slint
 * draws, laid over the view (src/view.ts) for one window (src/plan/screen.ts).
 *
 * - Every view entry with `focused` carries `focusRegion`; the target a
 *   region enters on `active` carries `focusActive: true`.
 * - `focusPlan` names each region's exits: up, down, left, right, each to a
 *   region with its entry rule. Inside a region a press goes to the target
 *   drawn nearest that way (src/press.ts reads the same plan).
 * - Each button carries its `line` in its row (the row wraps when its
 *   buttons do not fit), a dialog whether its buttons stand at caps-2 and
 *   the window of Korri's lines it shows (src/plan/dialog-words.ts), and
 *   `screen` the plan's grid and structure for ui/tokens.slint.
 *
 * The shell strips focusRegion, focusActive and focusPlan before it binds a
 * view to .slint; the rest is .slint's.
 */
import type { SurfaceModel } from "../../../../../contracts/generated/surface"
import { launchCursor, launchListEntry, launchPlan } from "./plan/launch-settings"
import { buttonRow, dialogButtons, tabsPlan, type ZoneScreen } from "./plan/screen"
import { dialogKey, dialogRoom, dialogWindow, dialogWords } from "./plan/dialog-words"
import { planSaveData } from "./plan/save-data"
import { topLayer, ZONE_TABS } from "./state/navigation"
import { zoneShown, type ZoneShown, type ZoneSurfaceState } from "./state/surface"

export type ZoneEntry = "remembered" | "active" | "first"
export type ZoneExit = { readonly to: string; readonly entry: ZoneEntry }
export type ZoneExits = Partial<Record<"up" | "down" | "left" | "right", ZoneExit>>
export type ZoneFocusPlan = Readonly<Record<string, ZoneExits>>

/** The list's width on a page with the sky band (ZoneSkyBand's ZoneRowList). */
const skyListWidth = (screen: ZoneScreen, well: boolean): number => screen.width - 16 * screen.a - (well ? 56 * screen.a : 0)

type Json = Record<string, unknown>
type Btnish = Json & { label: string }

/** Lay a row of buttons out at a width: each button's line. */
function lines<T extends Btnish>(buttons: readonly T[], width: number, cap: number, screen: ZoneScreen): T[] {
  const row = buttonRow(buttons.map(button => button.label), width, cap, screen.a, screen.look)
  return buttons.map((button, index) => ({ ...button, line: row.lines[index] ?? 0 }))
}

/**
 * Mark every entry with `focused` under `value` as standing in `region`.
 * The view's entries are made fresh by each view() call, so this writes
 * into them rather than copying the tree (QuickJS pays for every copy).
 */
/** The regions that hold a target in the view under way (regionOf notes each; focusView reads them for the exits). */
let REGIONS = new Set<string>()

function regionOf<T>(value: T, region: string, active?: (entry: Json) => boolean): T {
  if (Array.isArray(value)) {
    for (const item of value) regionOf(item, region, active)
    return value
  }
  if (value === null || typeof value !== "object" || "$image" in (value as Json)) return value
  const entry = value as Json
  for (const key in entry) {
    const inner = entry[key]
    if (inner !== null && typeof inner === "object") regionOf(inner, region, active)
  }
  if ("focused" in entry) {
    entry.focusRegion = region
    if (active?.(entry) === true) entry.focusActive = true
    if (isTarget(entry)) REGIONS.add(region)
  }
  return value
}

/** Whether an entry under `value` has the focus. */
function hasFocus(value: unknown): boolean {
  if (Array.isArray(value)) {
    for (const item of value) if (hasFocus(item)) return true
    return false
  }
  if (value === null || typeof value !== "object") return false
  const entry = value as Json
  if (entry.focused === true) return true
  for (const key in entry) {
    const inner = entry[key]
    if (inner !== null && typeof inner === "object" && hasFocus(inner)) return true
  }
  return false
}

/** `value` with no entry focused. */
function unfocused<T>(value: T): T {
  // Nothing under it holds the focus: the same value (no copy).
  if (!hasFocus(value)) return value
  if (Array.isArray(value)) return value.map(unfocused) as T
  if (value === null || typeof value !== "object" || "$image" in (value as Json)) return value
  const out: Json = {}
  for (const [key, inner] of Object.entries(value as Json)) out[key] = key === "focused" ? false : unfocused(inner)
  return out as T
}

/**
 * `value` with `focused` dropped from every entry under it: none of it is
 * drawn, so none of it takes the cursor. The shell keeps a field a view
 * leaves out, so .slint reads `focused` only where it draws the entry.
 */
function undrawn<T>(value: T): T {
  // In place: the view's entries are made fresh by each view() call.
  if (Array.isArray(value)) {
    for (const item of value) undrawn(item)
    return value
  }
  if (value === null || typeof value !== "object" || "$image" in (value as Json)) return value
  const entry = value as Json
  if ("focused" in entry) delete entry.focused
  for (const key in entry) {
    const inner = entry[key]
    if (inner !== null && typeof inner === "object") undrawn(inner)
  }
  return value
}

/** A list's rows with `focused` dropped from those that never take the cursor (headings, notes). */
export const targetRows = (rows: Json[]): Json[] =>
  rows.map(entry => {
    if (!("focused" in entry) || entry.focused === true || typeof entry.kind !== "string" || !NOT_TARGETS.has(entry.kind)) return entry
    const { focused: _, ...rest } = entry
    return rest
  })

/** An entry that can take the cursor: it has `focused`, words, and is not a list's heading or note. */
const NOT_TARGETS = new Set(["title", "caption", "note", "problem", "text", "tags", "word", "buttons"])
const isTarget = (entry: Json): boolean =>
  "focused" in entry && entry.label !== "" && !(typeof entry.kind === "string" && NOT_TARGETS.has(entry.kind))
    && !("shown" in entry && entry.shown === false)

/** A region holds a target when it has an entry that can take the cursor. */
const hasTargets = (value: unknown): boolean => {
  if (Array.isArray(value)) return value.some(hasTargets)
  if (value === null || typeof value !== "object" || "$image" in (value as Json)) return false
  const entry = value as Json
  if (isTarget(entry)) return true
  for (const key in entry) if (hasTargets(entry[key])) return true
  return false
}

/**
 * The view's keys that hold focus entries. The passes over focus (one
 * focus, moments, regions) walk only these: the rest (the cards, the facts,
 * the foot) never hold an entry with `focused`, and walking them cost most
 * of a view call in QuickJS.
 */
const FOCUS_KEYS = ["tabs", "cardMarks", "actions", "find", "get", "optsRows", "keyboard", "dialog", "busy", "saveCards", "retired", "slab", "seats", "next", "newSave", "rows", "keep"] as const

/** The layers .slint draws only while shown (main.slint, ZoneGames, ZoneStatusLayer: each under `if ….shown`). */
const HIDDEN_WHOLE = ["keyboard", "dialog", "busy", "hud", "launchCard", "tally"] as const
const NOT_SHOWN = Object.freeze({ shown: false })
const NOTHING = Object.freeze({})

/** The row regions of a page whose rows each take their own region (RUN WITH, GET GAMES). */
const rowRegion = (index: number) => `row-${index}`

export interface ZoneViewFocus {
  readonly page: ZoneShown
}

/**
 * The view with its regions, exits and the plan's decisions for .slint.
 * `overlay`: the view is PAUSED's Overlay, whose dialog has the whole window.
 */
export function focusView<V extends Json>(raw: V, state: ZoneSurfaceState, korri: SurfaceModel, screen: ZoneScreen, overlay = false): V & { focusPlan: ZoneFocusPlan; screen: Json } {
  // In place: surfaceView (and overlayView) make the view fresh for this call.
  const view: Json = raw
  REGIONS = new Set<string>()
  const page = zoneShown(state, korri)
  const nav = state.nav
  const layer = topLayer(nav)
  const a = screen.a
  const c1 = 10 * a

  // ---- The plan's decisions .slint draws ----
  const activeTab = Math.max(0, ZONE_TABS.indexOf(nav.tab))
  const tabs = tabsPlan(ZONE_TABS.map(tab => (raw.tabs as Btnish[])[ZONE_TABS.indexOf(tab)]?.label ?? ""), activeTab, screen)
  const bodyH = screen.height - screen.topH - screen.footH
  view.screen = {
    a,
    wArt: screen.wArt,
    hArt: screen.hArt,
    wide: screen.select.band === "wide",
    tall: screen.select.strips === "tall",
    stripLevel: ["inline", "tight", "keys", "stacked"].indexOf(screen.top.level),
    tabsWrap: tabs.wraps,
    saveBeside: planSaveData(screen.width, bodyH).slab === "beside",
  }
  view.actions = lines(raw.actions as Btnish[], screen.width, c1, screen)
  view.retired = lines(raw.retired as Btnish[], screen.width, c1, screen)
  const dialog = raw.dialog as Json & { buttons: Btnish[]; headline: string; korri: boolean; lines: string[]; title: string }
  const planned = dialogButtons(dialog.buttons.map(button => button.label), screen)
  // The headline and Korri's lines: the window of them that fits between the
  // headline and the buttons, from where up and down moved them for this
  // dialog's words. Each is one text, a line break between each line, drawn
  // by one ZoneCaps.
  const room = dialogRoom(screen, overlay, (raw.foot as { shown?: boolean } | undefined)?.shown === true)
  const words = dialogWords(dialog, room, screen)
  const shownLines = dialogWindow(words, state.dialogLine.words === dialogKey(dialog) ? state.dialogLine.line : 0)
  view.dialog = {
    ...dialog,
    headline: words.head.join("\n"),
    headCap: words.headCap,
    headBreaks: Math.max(0, words.head.length - 1),
    lines: shownLines.lines.length === 0 ? [] : [shownLines.lines.join("\n")],
    breaks: Math.max(0, shownLines.lines.length - 1),
    above: shownLines.above,
    below: shownLines.below,
    cap: planned.cap,
    buttons: dialog.buttons.map((button, index) => ({ ...button, line: planned.lines[index] ?? 0 })),
  }
  const busy = raw.busy as Json & { buttons: Btnish[] }
  view.busy = { ...busy, buttons: lines(busy.buttons, 160 * a, c1, screen) }
  const well = (raw.well as { shown?: boolean } | undefined)?.shown === true
  const listWidth = page === "onboarding" ? screen.width - 24 * a : skyListWidth(screen, well)
  const withLines = (rows: Json[], width: number) => rows.map(entry => {
    const buttons = lines((entry.buttons as Btnish[] | undefined) ?? [], width, c1, screen)
    // The held button's line: the list keeps it, not the whole row, in view.
    const held = buttons.find(button => button.focused === true)
    // The row the list keeps in view: the focused row, or the row of the focused button. Its own
    // field, so the list scrolls by the cursor, not by whichever entry is drawn focused.
    return { ...entry, buttons, holds: entry.holds === true || entry.focused === true, holdsLine: held === undefined ? 0 : held.line }
  })
  view.rows = withLines(raw.rows as Json[], listWidth)
  view.optsRows = withLines(raw.optsRows as Json[], screen.width - 24 * a)

  // ---- One focus: a layer that holds the cursor leaves the page under it unfocused ----
  const holder = ["dialog", "keyboard", "busy"].find(key => {
    const layer = view[key] as { shown?: boolean; buttons?: unknown[]; keys?: unknown[] } | undefined
    return layer?.shown === true && ((layer.buttons ?? layer.keys)?.length ?? 0) > 0 && hasFocus(layer)
  })
  if (holder !== undefined) {
    for (const key of FOCUS_KEYS) if (key !== holder && key in view) view[key] = unfocused(view[key])
  }

  // ---- A moment (the title card, the tally, the attract) covers the whole window: nothing under it holds the cursor ----
  const moment = (raw.launchCard as { shown?: boolean } | undefined)?.shown === true || (raw.tally as { shown?: boolean } | undefined)?.shown === true || raw.attract === true
  if (moment) for (const key of FOCUS_KEYS) if (key in view) view[key] = unfocused(view[key])

  // ---- Only what is drawn takes the cursor: what the page does not draw carries no `focused` ----
  // A key hidden here holds no focus entry: the regions pass skips it.
  const hidden = new Set<string>()
  const hide = (key: string) => {
    // The band's marks share OFF_MARK (src/view.ts): new entries, never written in place.
    view[key] = key === "cardMarks" ? (view[key] as Json[] | undefined)?.map(mark => ({ label: mark.label ?? "" })) : undrawn(view[key])
    hidden.add(key)
  }
  if (raw.tabsShown !== true) hide("tabs")
  if (page !== "games") {
    for (const key of ["actions", "cardMarks", "find", "get"]) hide(key)
  } else {
    // While FIND's keyboard is up, ZoneGames draws no band, FIND or GET GAMES under it.
    const typing = raw.typing === true
    if (raw.findShown !== true || typing) hide("find")
    if (raw.getShown !== true || typing) hide("get")
    if (typing) hide("cardMarks")
    // Cards out of ZoneCardBand's reach keep `focused`: they are targets not drawn yet (the band brings
    // each in as the cursor nears it), and each keeps its place among the band's entries.
  }
  const qr = view.qrShown === true
  if (page !== "savedata" || qr) {
    hide("saveCards")
    hide("retired")
  }
  if (page !== "savedata" || !qr) hide("slab")
  if (page !== "onboarding") {
    for (const key of ["seats", "next", "newSave"]) hide(key)
  } else {
    if ((view.next as Btnish).label === "") hide("next")
    if ((view.newSave as { shown?: boolean }).shown !== true) hide("newSave")
  }
  view.rows = targetRows(view.rows as Json[])
  view.optsRows = targetRows(view.optsRows as Json[])
  // LAUNCH SETTINGS' keep row, drawn only while a row is changed.
  const keep = view.keep as { shown?: boolean } | undefined
  if (keep !== undefined && keep.shown !== true) hide("keep")

  // ---- Regions ----
  if (!hidden.has("tabs")) view.tabs = regionOf(view.tabs, "tabs", entry => entry.active === true)
  const byIndex = (list: unknown, index: unknown) =>
    Array.isArray(list) ? list.map((entry, at) => (at === index ? { ...(entry as Json), focusActive: true } : entry)) : list
  // The band's marks come in their region (src/view.ts bandMarks): only the
  // focused card's is marked active, in place (it is the pool's, made fresh).
  const marks = (view.cardMarks as Json[] | undefined) ?? []
  if (!hidden.has("cardMarks") && marks.length > 0) {
    REGIONS.add("band")
    const active = marks[raw.focusedCard as number]
    if (active !== undefined && "focused" in active) active.focusActive = true
  }
  if (!hidden.has("actions")) view.actions = regionOf(view.actions, "actions")
  if (!hidden.has("find")) view.find = regionOf(view.find, "find")
  if (!hidden.has("get")) view.get = regionOf(view.get, "get")
  if (!hidden.has("optsRows")) view.optsRows = regionOf(view.optsRows, "opts")
  if (!hidden.has("keyboard")) view.keyboard = regionOf(view.keyboard, "keys")
  if (!hidden.has("dialog")) view.dialog = regionOf(view.dialog, "dialog")
  if (!hidden.has("busy")) view.busy = regionOf(view.busy, "busy")
  if (!hidden.has("saveCards")) view.saveCards = byIndex(regionOf(view.saveCards, "saves"), raw.saveFocus)
  if (!hidden.has("retired")) view.retired = regionOf(view.retired, "retired")
  if (!hidden.has("slab")) view.slab = regionOf(view.slab, "secret")
  if (!hidden.has("seats")) view.seats = regionOf(view.seats, "seats")
  if (!hidden.has("next")) view.next = regionOf(view.next, "next")
  if (!hidden.has("newSave")) view.newSave = regionOf(view.newSave, "save")
  const perRow = page === "runwith" || page === "store"
  // A row's region counts only the rows that hold targets, so a note that opens under the focused route does not rename the regions after it.
  if (perRow) {
    let held = 0
    view.rows = (view.rows as Json[]).map(entry => regionOf(entry, rowRegion(hasTargets(entry) ? held++ : held)))
  } else {
    // A list is a column of rows; LAUNCH SETTINGS' row of buttons is a region of its own.
    const rows = view.rows as Json[]
    const actions = page === "launch" ? rows.findIndex(entry => entry.kind === "buttons") : -1
    const items = rows.map((entry, index) => (index !== actions && entry.kind !== undefined && ["item", "range", "route"].includes(entry.kind as string) ? index : -1)).filter(index => index >= 0)
    const marked = rows.findIndex(entry => entry.focused === true || entry.focusActive === true)
    // The row the list enters on: the one it holds, else the one Zone goes to (the first; the last from below).
    let enters = marked >= 0 && items.includes(marked) ? marked : (page === "onboarding" ? items.at(-1) : items[0]) ?? -1
    const launch = korri.launchSettings
    if (page === "launch" && launch !== undefined && launch._tag !== "Closed" && marked < 0) {
      // LAUNCH SETTINGS: the row src/press.ts goes to from the zone the cursor is in (src/plan/launch-settings.ts).
      const plan = launchPlan(launch, korri.settingsStatus)
      const cursor = launchCursor(plan, state.launch.focused, state.launchAction)
      const entry = cursor.zone === "actions" || cursor.zone === "keep" ? launchListEntry(plan, state.launch.focused, cursor.zone) : undefined
      if (entry !== undefined) enters = rows.findIndex(candidate => candidate.focusId === `row:${entry.id}`)
    }
    // LAUNCH SETTINGS enters its buttons on the first that can be pressed.
    const enabled = actions >= 0 ? ((rows[actions]!.buttons as Json[]).find(button => button.disabled !== true) ?? (rows[actions]!.buttons as Json[])[0]) : undefined
    view.rows = rows.map((entry, index) =>
      index === actions ? regionOf(entry, "actions", button => button === enabled) : regionOf(entry, "list", () => index === enters))
  }
  if (!hidden.has("keep") && keep !== undefined) view.keep = regionOf(view.keep, "keep", () => true)

  // ---- Exits ----
  const regions = REGIONS
  const plan: Record<string, ZoneExits> = {}
  const presence = new Map<string, boolean>()
  const present = (key: string) => {
    let has = presence.get(key)
    if (has === undefined) presence.set(key, (has = hasTargets(view[key])))
    return has
  }
  const shown = (key: string) => (view[key] as { shown?: boolean } | undefined)?.shown === true
  const covered = shown("dialog") || shown("keyboard") || (shown("busy") && ((view.busy as { buttons?: unknown[] }).buttons?.length ?? 0) > 0)
  const gamesError = page === "games" && korri.catalog._tag === "Error"
  // GAMES' catalog error: RELOAD stands under the tabs, and the tabs stay reachable.
  if (gamesError && shown("dialog") && korri.status._tag === "Browsing" && layer === "none") {
    plan.dialog = { up: { to: "tabs", entry: "active" } }
    plan.tabs = { down: { to: "dialog", entry: "first" } }
  }
  // GAMES' own layers (GAME OPTS., the keyboard, the attract) hold the cursor in their region.
  const ownLayer = page === "games" && layer !== "none"
  if (!covered && !ownLayer) {
    switch (page) {
      case "games": {
        const band = present("cardMarks")
        const find = present("find")
        const get = present("get")
        const toBand: ZoneExit = { to: "band", entry: "active" }
        plan.tabs = {
          ...(find ? { right: { to: "find", entry: "first" } } : {}),
          ...(band ? { down: toBand } : get ? { down: { to: "get", entry: "first" } } : find ? { down: { to: "find", entry: "first" } } : {}),
        }
        if (get) plan.get = { up: { to: "tabs", entry: "active" }, ...(find ? { right: { to: "find", entry: "first" } } : {}), ...(band ? { down: toBand } : {}) }
        if (find) plan.find = { up: { to: "tabs", entry: "active" }, left: get ? { to: "get", entry: "first" } : { to: "tabs", entry: "active" }, ...(band ? { down: toBand } : {}) }
        if (band) {
          plan.band = {
            up: get && nav.card === 0 ? { to: "get", entry: "first" } : { to: "tabs", entry: "active" },
            ...(present("actions") ? { down: { to: "actions", entry: "first" } } : {}),
          }
        }
        if (present("actions")) plan.actions = { up: toBand }
        break
      }
      case "options":
        plan.tabs = present("rows") ? { down: { to: "list", entry: "active" } } : {}
        plan.list = { up: { to: "tabs", entry: "active" } }
        break
      case "savedata": {
        const secret = present("slab") || (view.qrShown === true)
        plan.tabs = secret ? { down: { to: "secret", entry: "first" } } : { down: { to: "saves", entry: "active" } }
        if (secret) plan.secret = { up: { to: "tabs", entry: "active" } }
        else {
          plan.saves = { up: { to: "tabs", entry: "active" }, ...(present("retired") ? { down: { to: "retired", entry: "first" } } : {}) }
          if (present("retired")) plan.retired = { up: { to: "saves", entry: "active" } }
        }
        break
      }
      case "runwith":
      case "store": {
        const rows = (view.rows as Json[]).map((entry, index) => (hasTargets(entry) ? index : -1)).filter(index => index >= 0)
        rows.forEach((_, at) => {
          const exits: ZoneExits = {}
          if (at > 0) exits.up = { to: rowRegion(at - 1), entry: "first" }
          if (at < rows.length - 1) exits.down = { to: rowRegion(at + 1), entry: "first" }
          plan[rowRegion(at)] = exits
        })
        break
      }
      case "launch": {
        // The zones top to bottom, as drawn: Korri's buttons (over the rows under a conflict or an error, else at the list's end), the list, the keep row.
        const rows = view.rows as Json[]
        const buttons = rows.findIndex(entry => entry.kind === "buttons")
        const first = rows.findIndex(entry => entry.kind === "item")
        const zones = [
          ...(buttons >= 0 && first >= 0 && buttons < first ? ["actions"] : []),
          "list",
          ...(buttons >= 0 && !(first >= 0 && buttons < first) ? ["actions"] : []),
          "keep",
        ].filter(zone => regions.has(zone))
        zones.forEach((zone, at) => {
          const above = zones[at - 1]
          const below = zones[at + 1]
          plan[zone] = {
            ...(above === undefined ? {} : { up: { to: above, entry: "active" } }),
            ...(below === undefined ? {} : { down: { to: below, entry: "active" } }),
          }
        })
        break
      }
      case "onboarding":
        if (present("seats") && present("next")) {
          plan.seats = { down: { to: "next", entry: "first" } }
          plan.next = { up: { to: "seats", entry: "first" } }
        }
        if (present("newSave") && present("next")) {
          plan.save = { right: { to: "next", entry: "first" }, down: { to: "next", entry: "first" } }
          plan.next = { left: { to: "save", entry: "first" }, up: { to: "save", entry: "first" } }
        }
        if (present("rows") && present("next")) {
          plan.list = { down: { to: "next", entry: "first" } }
          plan.next = { up: { to: "list", entry: "active" } }
        }
        break
      case "join":
      case "paused":
        break
    }
  }
  // Every exit names a region with targets in this view (regionOf noted them).
  const focusPlan: Record<string, ZoneExits> = {}
  for (const [region, exits] of Object.entries(plan)) {
    const kept = Object.fromEntries(Object.entries(exits).filter(([, exit]) => exit !== undefined && regions.has(exit.to)))
    if (regions.has(region)) focusPlan[region] = kept
  }
  view.focusPlan = focusPlan
  // A layer that is not shown is sent as only that: .slint reads none of its
  // other fields while it is hidden (every one stands under `if …shown`), and
  // each field costs the view call (its JSON) on every press.
  for (const key of HIDDEN_WHOLE) if ((view[key] as { shown?: boolean } | undefined)?.shown === false) view[key] = NOT_SHOWN
  // SAVE DATA's and first boot's parts, on a page that is neither (main.slint draws them only there).
  if (page !== "savedata" && page !== "onboarding" && "saveCards" in view) {
    view.saveCards = []
    view.retired = []
    view.slab = NOTHING
    view.seats = []
    view.next = NOTHING
    view.newSave = NOTHING
  }
  return view as V & { focusPlan: ZoneFocusPlan; screen: Json }
}

/** The tabs as the plan places them, for the focus graph. */
export const tabBoxes = (labels: readonly string[], active: number, screen: ZoneScreen) => tabsPlan(labels, active, screen).boxes
