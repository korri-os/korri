/**
 * Korri's account sign-ins (ADR 0083, ADR 0089): one card for any entry of
 * `model.accountSignIns`, whatever service it signs in to. Velvet reads no
 * meaning from an entry's id: Settings has a row for each, setup a step for
 * each one with an `offer`, and an entry takes the screen by itself while a
 * launch waits for it (`forGame`).
 *
 * The code is Korri's module grid, drawn in the "Pleats" style
 * (packages/velvet/docs/account-code.md): each vertical run of dark modules
 * is one capsule, the three finders are rounded squares, and the middle is
 * cleared for a small Ink badge with the service's logo, or the first letter
 * of its title when it has none. Ink on House light, with a quiet zone of
 * four modules; never inverted.
 *
 * Every string Korri publishes is drawn as it is; every action is a
 * RunAction with Korri's id.
 */
import type {
  SurfaceAccountSignIn as Entry,
  SurfaceAccountState,
  SurfaceAction,
  SurfaceModel as Model,
  SurfaceQrCode,
} from "../../../../../contracts/generated/surface"
import { onForget } from "./memo"
import { NONE, send, type Act, type ButtonSpec, type PanelSide, type RowSpec } from "./types"
import { caps } from "./words"

/** The light border around the code, in modules (ISO/IEC 18004 asks for four). */
export const QUIET = 4
/** The smallest grid a QR code has (version 1). A smaller one is not a code. */
const LEAST_SIZE = 21

const run = (actionId: string): Act => send({ _tag: "RunAction", actionId })

/** Whether Korri's grid is a code Velvet can draw: `size × size` characters of 0 and 1. */
export function drawable(qr: SurfaceQrCode): boolean {
  const size = qr.size
  return Number.isInteger(size) && size >= LEAST_SIZE && qr.modules.length === size * size && !/[^01]/.test(qr.modules)
}

/** The most of the code the cleared plate may take: past it a worn screen leaves error correction too little (the QR brief). */
const PLATE_MOST = 0.09

/**
 * The side of the cleared plate in the middle, in modules: the largest odd
 * number at most three tenths of the size, and at least 7 (the brief's rule;
 * it keeps the plate under 9 % of the code, so error correction has budget
 * left). The badge is two modules less, so one module of light rings it.
 * The smallest code (21 modules) cannot spare 7 x 7: it has no plate and no
 * badge (0), and scans.
 */
export function plateModules(size: number): number {
  const most = Math.floor(0.3 * size)
  const side = Math.max(7, most % 2 === 1 ? most : most - 1)
  return side * side > PLATE_MOST * size * size ? 0 : side
}

/** The badge's side in modules (0: no code). */
export const badgeModules = (qr: SurfaceQrCode): number => (drawable(qr) && plateModules(qr.size) > 0 ? plateModules(qr.size) - 2 : 0)

/**
 * The code's ink as path commands in a viewbox of `size + 2 * QUIET`
 * modules, for one Path with the even-odd rule: a capsule for each vertical
 * run of dark modules, then the three finders as rounded squares. A grid
 * Velvet cannot draw gives "".
 */
export function qrPath(qr: SurfaceQrCode): string {
  // Korri sends the same code with every publication until the service turns it.
  if (lastQr !== undefined && lastQr.size === qr.size && lastQr.modules === qr.modules) return lastQr.path
  const path = drawable(qr) ? pleats(qr) : ""
  lastQr = { size: qr.size, modules: qr.modules, path }
  return path
}

let lastQr: { size: number; modules: string; path: string } | undefined
onForget(() => {
  lastQr = undefined
})

/** A number for a path: at most two decimals, no trailing zeros. */
const n = (value: number): string => String(Math.round(value * 100) / 100)

function pleats(qr: SurfaceQrCode): string {
  const size = qr.size
  const plate = plateModules(size)
  const centre = (size - 1) / 2
  const half = (plate - 1) / 2
  const finder = (x: number, y: number): boolean => (x < 7 && y < 7) || (x >= size - 7 && y < 7) || (x < 7 && y >= size - 7)
  const cleared = (x: number, y: number): boolean => plate > 0 && Math.abs(x - centre) <= half && Math.abs(y - centre) <= half
  const dark = (x: number, y: number): boolean => qr.modules.charCodeAt(y * size + x) === 49 && !finder(x, y) && !cleared(x, y)
  const parts: string[] = []
  for (let x = 0; x < size; x++) {
    let y = 0
    while (y < size) {
      if (!dark(x, y)) {
        y++
        continue
      }
      const top = y
      while (y < size && dark(x, y)) y++
      // One capsule 0.78 wide with round ends of radius 0.39, from 0.04 under the run's
      // top to 0.04 over its foot: a run of one module is a short capsule, not a circle.
      parts.push(`M${n(x + QUIET + 0.11)} ${n(top + QUIET + 0.43)}a.39 .39 0 0 1 .78 0v${n(y - 1 - top + 0.14)}a.39 .39 0 0 1-.78 0z`)
    }
  }
  // The hole turns the other way: it is cut out under either fill rule.
  for (const [x, y] of [[0, 0], [size - 7, 0], [0, size - 7]] as const) {
    parts.push(rounded(x + QUIET, y + QUIET, 7, 2), hole(x + QUIET + 1, y + QUIET + 1, 5, 1), rounded(x + QUIET + 2, y + QUIET + 2, 3, 1))
  }
  return parts.join("")
}

/** A rounded square as one closed subpath, clockwise. */
function rounded(x: number, y: number, side: number, radius: number): string {
  const straight = side - 2 * radius
  const arc = (dx: number, dy: number) => `a${radius} ${radius} 0 0 1 ${dx} ${dy}`
  return `M${x + radius} ${y}h${straight}${arc(radius, radius)}v${straight}${arc(-radius, radius)}h${-straight}${arc(-radius, -radius)}v${-straight}${arc(radius, -radius)}z`
}

/** The same, anticlockwise. */
function hole(x: number, y: number, side: number, radius: number): string {
  const straight = side - 2 * radius
  const arc = (dx: number, dy: number) => `a${radius} ${radius} 0 0 0 ${dx} ${dy}`
  return `M${x + radius} ${y}${arc(-radius, radius)}v${straight}${arc(radius, radius)}h${straight}${arc(radius, -radius)}v${-straight}${arc(-radius, -radius)}z`
}

/** The badge's letter when the entry has no logo: the first letter of its title, in caps. */
export function monogram(title: string): string {
  const first = [...title.trim()][0]
  return first === undefined ? "" : caps(first)
}

// ---- Entries ----

export const entryById = (model: Model, id: string): Entry | undefined => model.accountSignIns?.find(entry => entry.id === id)

/** The entry a launch waits for: its code, or its sign-in at work, with the game it is for. */
export function waitingEntry(model: Model): Entry | undefined {
  return model.accountSignIns?.find(entry => (entry.state._tag === "Code" || entry.state._tag === "Working") && entry.state.forGame !== undefined)
}

/** The entries setup offers, in Korri's order. */
export const offered = (model: Model): Entry[] => (model.accountSignIns ?? []).filter(entry => entry.offer !== undefined && entry.offer !== "")

/** An entry's state in Settings, at rest: who is signed in, or what to do. */
export function stateWord(state: SurfaceAccountState): string {
  switch (state._tag) {
    case "SignedIn":
      return state.accountName
    case "SignedOut":
      return "Not signed in"
    case "Code":
    case "Working":
      return "Signing in"
    case "Problem":
      return "Needs attention"
  }
}

/** Korri's own line about the state, where it has one. */
export function stateLine(state: SurfaceAccountState): string | undefined {
  switch (state._tag) {
    case "SignedOut":
      return state.kicker
    case "Code":
    case "Working":
    case "Problem":
      return state.kicker
    case "SignedIn":
      return undefined
  }
}

/** Each of Korri's actions as a button: the first leads, the rest are quiet. */
export const accountButtons = (prefix: string, actions: readonly SurfaceAction[]): ButtonSpec[] =>
  actions.map((action, index) => ({
    id: `${prefix}${action.id}`, label: action.label, quiet: index > 0,
    ...(action.enabled ? {} : { dim: true }), ...(action.destructive === true ? { danger: true } : {}),
    press: action.enabled ? run(action.id) : NONE,
  }))

export interface AccountContent {
  readonly sub: string
  readonly rows: RowSpec[]
  readonly buttons: ButtonSpec[]
  readonly side?: PanelSide
  /** Back: Korri's way out while a sign-in runs, else the page's own. */
  readonly cancel?: Act
}

/**
 * The card's words, rows, buttons and code for an entry's state. `inSetup`:
 * the step's heading is the entry's `offer`, so the card does not say it
 * again.
 */
export function accountContent(entry: Entry, inSetup: boolean): AccountContent {
  const state = entry.state
  const prefix = "a:a:"
  const note = (id: string, label: string): RowSpec => ({ id, kind: "note", label })
  const offer = entry.offer !== undefined && entry.offer !== "" && !inSetup ? entry.offer : undefined
  const cancel = (actions: readonly SurfaceAction[]): { cancel?: Act } => {
    // A sign-in under way has one way out, Korri's; Back takes it only when it is the only one.
    const enabled = actions.filter(action => action.enabled)
    return enabled.length === 1 ? { cancel: run(enabled[0]!.id) } : {}
  }
  switch (state._tag) {
    case "SignedOut":
      return {
        sub: state.kicker ?? offer ?? "",
        rows: [note("a:how", "You approve the sign-in on your phone. No password is typed here.")],
        buttons: accountButtons(prefix, state.actions),
      }
    case "Code": {
      const ok = drawable(state.qr)
      return {
        sub: state.forGame === undefined ? "" : `${state.forGame} needs your ${entry.title} sign-in.`,
        rows: ok ? [note("a:fresh", "The code can change. Scan the one shown now.")] : [{ id: "a:bad", kind: "problem", label: "Korri sent a sign-in code this page cannot draw." }],
        buttons: accountButtons(prefix, state.actions),
        side: {
          kind: "qr", title: state.kicker, lines: state.detail === undefined ? [] : [state.detail], qr: state.qr,
          mark: monogram(entry.title), ...(entry.logo === undefined || entry.logo.url === "" ? {} : { logo: entry.logo }),
        },
        ...cancel(state.actions),
      }
    }
    case "Working":
      return {
        sub: state.forGame === undefined ? "" : `${state.forGame} starts when you are signed in.`,
        // A row that rests, with the busy mark: the page shows it is at work and has a focus.
        rows: [{ id: "a:working", kind: "row", label: state.kicker, mark: "busy", rests: true }],
        buttons: accountButtons(prefix, state.actions),
        ...cancel(state.actions),
      }
    case "SignedIn":
      return {
        sub: `Signed in as ${state.accountName}.`,
        rows: offer === undefined ? [] : [note("a:offer", offer)],
        buttons: accountButtons(prefix, state.actions),
      }
    case "Problem":
      return {
        sub: state.kicker,
        rows: [{ id: "a:problem", kind: "problem", label: state.reason }],
        buttons: accountButtons(prefix, state.actions),
      }
  }
}
