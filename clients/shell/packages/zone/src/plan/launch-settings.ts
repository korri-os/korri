/**
 * What the LAUNCH SETTINGS page shows for Korri's launch settings editor
 * (ADR 0066): one list. Each setting stands once, under its setting section,
 * with the value the next launch would use, the same for a game that runs
 * here and a game that another device runs.
 *
 *     the head     the game's title, and where the game runs in Korri's
 *                  words ("Runs here", "Runs on aka")
 *     the list     Korri's notes and message, then every group with its
 *                  rows. A changed row shows its earlier value. A row Korri
 *                  gives no way to change is dimmed and says why while the
 *                  cursor is on it
 *     the actions  Korri's actions (Reload, Retry, Close) as one row of
 *                  buttons: under the message while Korri reports a conflict
 *                  or an error, at the list's end otherwise
 *     the keep row present only while a row is changed: where the changes
 *                  are kept. It stands under the list, always in view
 *
 * The page reuses OPTIONS' rows, keyboard and dialog, so it hands OPTIONS'
 * plan the groups and the keep row as settings groups; the state model
 * (src/state/surface.ts) runs OPTIONS' update on the same slices, and the two
 * can never disagree about the rows.
 *
 * Everything here is a pure function of Korri's model and the window
 * (src/plan/screen.ts): which words fit their box, where the earlier value
 * stands, how large the title is.
 */
import type { SurfaceAction, SurfaceLaunchSettings, SurfaceModel } from "../../../../../../contracts/generated/surface"
import { measureWords } from "./measure"
import { optionsPlan, type ZoneOptionsGroup, type ZoneOptionsRow, type ZoneOptionsSlices } from "./options"
import type { ZoneScreen } from "./screen"

export type ZoneLaunchOpen = Exclude<SurfaceLaunchSettings, { readonly _tag: "Closed" }>
export type ZoneLaunchState = ZoneLaunchOpen["_tag"]

export const launchOpen = (korri: Pick<SurfaceModel, "launchSettings">): boolean =>
  korri.launchSettings !== undefined && korri.launchSettings._tag !== "Closed"

/** A part of the page the cursor can stand in, top to bottom. */
export type ZoneLaunchZone = "actions" | "list" | "keep"

export interface ZoneLaunchPlan {
  /** OPTIONS' slices: the groups, then the keep row as a group of its own. */
  readonly slices: ZoneOptionsSlices
  /** The rows show while Korri is ready, and beside an error: the rows of this device still work then. */
  readonly rows: boolean
  /** The list's groups, as OPTIONS' plan makes them (a DISMISS row under a row Korri has a problem with). */
  readonly groups: readonly ZoneOptionsGroup[]
  /** Every row of the list, top to bottom: the order up and down walk. */
  readonly list: readonly ZoneOptionsRow[]
  /** The earlier value in Korri's words, by the id of each row changed and not kept yet. */
  readonly was: ReadonlyMap<string, string>
  /** Where the changes are kept. Present only while a row is changed. */
  readonly keep?: ZoneOptionsRow
  /** Korri's actions, Close among them. */
  readonly actions: readonly SurfaceAction[]
  /** The actions stand under Korri's message, over the rows. */
  readonly actionsFirst: boolean
  /** The zones that hold a target, top to bottom. */
  readonly zones: readonly ZoneLaunchZone[]
  /** The action that closes the editor, for B. */
  readonly closeActionId?: string
}

/** The keep row's group, by its place among the slices' groups: the last. */
const KEEP_TITLE = ""

export function launchPlan(launch: ZoneLaunchOpen, status: SurfaceModel["settingsStatus"]): ZoneLaunchPlan {
  const close = launch.actions.find(action => action.id.endsWith(":cancel"))
  const rows = launch._tag === "Ready" || launch._tag === "Error"
  const was = new Map(rows ? (launch.changes ?? []).map(change => [change.id, change.was] as const) : [])
  const shown = rows ? (launch.groups ?? []).filter(group => group.items.length > 0) : []
  const kept = rows && was.size > 0 ? launch.keep : undefined
  const slices: ZoneOptionsSlices = {
    settings: [...shown, ...(kept === undefined ? [] : [{ title: KEEP_TITLE, items: [kept] }])],
    settingsStatus: status,
    actions: [],
  }
  const options = optionsPlan(slices)
  const groups = kept === undefined ? options.groups : options.groups.slice(0, -1)
  const list = groups.flatMap(group => group.rows)
  const keep = kept === undefined ? undefined : options.groups.at(-1)?.rows[0]
  const actionsFirst = launch._tag === "Conflict" || launch._tag === "Error"
  const zones: ZoneLaunchZone[] = []
  if (actionsFirst && launch.actions.length > 0) zones.push("actions")
  if (list.length > 0) zones.push("list")
  if (!actionsFirst && launch.actions.length > 0) zones.push("actions")
  if (keep !== undefined) zones.push("keep")
  return {
    slices,
    rows,
    groups,
    list,
    was,
    ...(keep === undefined ? {} : { keep }),
    actions: launch.actions,
    actionsFirst,
    zones,
    ...(close === undefined ? {} : { closeActionId: close.id }),
  }
}

/** The slices for OPTIONS' update while the editor is open; none while it is not. */
export function launchSlices(korri: Pick<SurfaceModel, "launchSettings" | "settingsStatus">): ZoneOptionsSlices | undefined {
  const launch = korri.launchSettings
  return launch === undefined || launch._tag === "Closed" ? undefined : launchPlan(launch, korri.settingsStatus).slices
}

// ---- The cursor ----

/** Where the cursor stands: on an action, on a row of the list, on the keep row, or nowhere. */
export type ZoneLaunchCursor =
  | { readonly zone: "actions"; readonly actionId: string }
  | { readonly zone: "list"; readonly rowId: string }
  | { readonly zone: "keep"; readonly rowId: string }
  | { readonly zone: "none" }

/** The action the buttons enter on: the first that can be pressed. */
export const launchEntryAction = (plan: ZoneLaunchPlan): SurfaceAction | undefined =>
  plan.actions.find(action => action.enabled) ?? plan.actions[0]

/**
 * The cursor, from the row and the action the state holds. With neither, it
 * starts on the first row; with no rows, or while Korri reports a conflict
 * or an error (the message asks for an action), on the first action that
 * can be pressed.
 */
export function launchCursor(plan: ZoneLaunchPlan, focused: string | undefined, action: string | undefined): ZoneLaunchCursor {
  if (action !== undefined && plan.actions.some(candidate => candidate.id === action)) return { zone: "actions", actionId: action }
  if (focused !== undefined && plan.keep?.id === focused) return { zone: "keep", rowId: focused }
  if (focused !== undefined && plan.list.some(row => row.id === focused)) return { zone: "list", rowId: focused }
  const first = plan.list[0]
  const entry = plan.actions.find(candidate => candidate.enabled)
  if (entry !== undefined && (plan.actionsFirst || first === undefined)) return { zone: "actions", actionId: entry.id }
  if (first !== undefined) return { zone: "list", rowId: first.id }
  return { zone: "none" }
}

/**
 * The row the list is entered on from another zone: the row it holds, else
 * its first row from above and its last row from below.
 */
export function launchListEntry(plan: ZoneLaunchPlan, held: string | undefined, from: ZoneLaunchZone): ZoneOptionsRow | undefined {
  const kept = plan.list.find(row => row.id === held)
  if (kept !== undefined) return kept
  return plan.zones.indexOf(from) < plan.zones.indexOf("list") ? plan.list[0] : plan.list.at(-1)
}

// ---- What fits (ui/molecules/ZoneOptionRow.slint, ui/organisms/ZoneSkyBand.slint) ----

/** The list's label share of a row (pages/ZoneLaunchSettings.slint). */
export const ZONE_LAUNCH_LABEL_SHARE = 0.55
/** The room between a changed value and its earlier value, in art px. */
export const ZONE_LAUNCH_WAS_GAP = 4
/** The title's cap in art px: its plan size, and the least it shrinks to before it is cut. */
export const ZONE_LAUNCH_TITLE_CAP = 20
export const ZONE_LAUNCH_TITLE_CAP_LEAST = 10
/** The head's room beside its title's panel: the band's margins and the gap to the tags, in art px. */
export const ZONE_LAUNCH_HEAD_MARGIN = 16
/** The title's panel past its words at each end, in art px: it leans by its height, the cap and one art px over and under. */
export const launchTitlePad = (cap: number): number => cap + 2
/** A lone tag's room beside its words (ZoneTagRow: one art px at each end), in art px. */
export const ZONE_LAUNCH_TAG_PAD = 2

/** The room a choice's triangles keep clear at each end of its value, in art px (ZoneOptionRow's `arrows-room`). */
export const ZONE_LAUNCH_ARROWS_ROOM = 4

export interface ZoneLaunchBoxes {
  /** The room a label's words have, in px. */
  readonly label: number
  /** The room a value's words have, in px. */
  readonly value: number
}

/** A row's two boxes at this window, as ZoneOptionRow lays them out in ZoneSkyBand's list. */
export function launchBoxes(screen: ZoneScreen, choice: boolean): ZoneLaunchBoxes {
  const a = screen.a
  const width = screen.width - 16 * a
  const label = Math.floor((width * ZONE_LAUNCH_LABEL_SHARE) / a) * a
  const value = width - label - 4 * a
  return {
    label: Math.max(0, label - 13 * a - 6 * a),
    value: Math.max(0, value - 26 * a - (choice ? 2 * ZONE_LAUNCH_ARROWS_ROOM * a : 0)),
  }
}

/** The earlier value as the row says it: "was 4x". */
export const wasWords = (was: string): string => `was ${was}`

export interface ZoneLaunchRowFit {
  /** The earlier value stands beside the value; otherwise the focused row's note says it. */
  readonly wasBeside: boolean
  /** The label's words are cut: the focused row's note says them whole. */
  readonly labelCut: boolean
  /** The value's words are cut: the focused row's note says them whole. */
  readonly valueCut: boolean
}

/**
 * What of a row fits at this window. The earlier value is a readout: it
 * yields its place beside the value before the value is cut, and moves to
 * the focused row's note.
 */
export function launchRowFit(label: string, labelFact: boolean, value: string, was: string | undefined, choice: boolean, screen: ZoneScreen): ZoneLaunchRowFit {
  const boxes = launchBoxes(screen, choice)
  const cap = 10 * screen.a
  const valueWidth = measureWords(value, cap, "fact", screen.look)
  const wasWidth = was === undefined ? 0 : measureWords(wasWords(was), cap, "fact", screen.look) + ZONE_LAUNCH_WAS_GAP * screen.a
  return {
    wasBeside: was !== undefined && valueWidth + wasWidth <= boxes.value,
    labelCut: measureWords(label, cap, labelFact ? "fact" : "caps", screen.look) > boxes.label,
    valueCut: valueWidth > boxes.value,
  }
}

export interface ZoneLaunchHead {
  /** The title's cap in px. */
  readonly cap: number
  /** The tag stands on its own line under the title. */
  readonly stacked: boolean
}

/**
 * The head: the title beside its tag. A title too long for its room takes
 * a line of its own over the tag when the window is tall (height pays);
 * otherwise, and then too, it yields its size down to the least before it
 * yields a letter.
 */
export function launchHead(title: string, tag: string | undefined, screen: ZoneScreen): ZoneLaunchHead {
  const a = screen.a
  const tagWidth = tag === undefined ? 0 : measureWords(tag, 10 * a, "caps", screen.look) + ZONE_LAUNCH_TAG_PAD * a
  const fits = (cap: number, room: number) => measureWords(title, cap * a, "korri", screen.look) <= room - 2 * launchTitlePad(cap) * a
  const beside = screen.width - ZONE_LAUNCH_HEAD_MARGIN * a - tagWidth
  const alone = screen.width - ZONE_LAUNCH_HEAD_MARGIN * a
  const largest = (room: number) => {
    for (let cap = ZONE_LAUNCH_TITLE_CAP; cap > ZONE_LAUNCH_TITLE_CAP_LEAST; cap--) if (fits(cap, room)) return cap
    return ZONE_LAUNCH_TITLE_CAP_LEAST
  }
  if (tag === undefined || fits(ZONE_LAUNCH_TITLE_CAP, beside)) return { cap: ZONE_LAUNCH_TITLE_CAP * a, stacked: false }
  if (screen.select.strips === "tall") return { cap: largest(alone) * a, stacked: true }
  return { cap: largest(beside) * a, stacked: false }
}
