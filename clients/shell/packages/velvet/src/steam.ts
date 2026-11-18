/**
 * The person's Steam sign-in (ADR 0083): `model.steamAccount` as Velvet's
 * list page. Settings opens it, first boot shows it as a step, and it takes
 * the screen by itself while a launch waits for it (`forGame`).
 *
 * Korri draws nothing of the QR code's meaning: it sends the module grid,
 * and Velvet draws it as one path on a light plate with a quiet zone of four
 * modules. Every string Korri publishes is drawn as it is; every action is
 * a RunAction with Korri's id.
 */
import type { SurfaceAction, SurfaceModel as Model, SurfaceQrCode, SurfaceSteamAccount } from "../../../../../contracts/generated/surface"
import { onForget } from "./memo"
import { NONE, send, type Act, type ButtonSpec, type PanelSide, type RowSpec } from "./types"

/** The light border around the code, in modules (ISO/IEC 18004 asks for four). */
export const QUIET = 4

const run = (actionId: string): Act => send({ _tag: "RunAction", actionId })

/**
 * The code's dark modules as path commands in a viewbox of `size + 2 * QUIET`
 * modules: one rectangle per run of dark modules in a row. A grid whose
 * length is not `size * size`, or that holds anything but 0 and 1, draws
 * nothing.
 */
export function qrPath(qr: SurfaceQrCode): string {
  // Korri sends the same code with every publication until Steam turns it.
  if (lastQr !== undefined && lastQr.size === qr.size && lastQr.modules === qr.modules) return lastQr.path
  const path = drawQr(qr)
  lastQr = { size: qr.size, modules: qr.modules, path }
  return path
}

let lastQr: { size: number; modules: string; path: string } | undefined
onForget(() => {
  lastQr = undefined
})

function drawQr(qr: SurfaceQrCode): string {
  const size = qr.size
  if (!Number.isInteger(size) || size <= 0 || qr.modules.length !== size * size || /[^01]/.test(qr.modules)) return ""
  const parts: string[] = []
  for (let y = 0; y < size; y++) {
    let x = 0
    while (x < size) {
      if (qr.modules.charCodeAt(y * size + x) !== 49) {
        x++
        continue
      }
      const start = x
      while (x < size && qr.modules.charCodeAt(y * size + x) === 49) x++
      parts.push(`M${start + QUIET} ${y + QUIET}h${x - start}v1h${start - x}z`)
    }
  }
  return parts.join("")
}

/** What a launch waiting for the sign-in shows: the code, or the sign-in at work. */
export function steamTakesScreen(model: Model): boolean {
  const account = model.steamAccount
  return account !== undefined && (account._tag === "Code" || account._tag === "Working") && account.forGame !== undefined
}

/** The Steam row's value in Settings: who is signed in, or what to do. */
export function steamValue(account: SurfaceSteamAccount): string {
  switch (account._tag) {
    case "SignedIn":
      return account.accountName
    case "SignedOut":
      return "Not signed in"
    case "Code":
    case "Working":
      return "Signing in"
    case "Problem":
      return "Needs attention"
  }
}

const buttons = (actions: readonly SurfaceAction[]): ButtonSpec[] =>
  actions.map((action, index) => ({
    id: `st:a:${action.id}`, label: action.label, quiet: index > 0,
    ...(action.enabled ? {} : { dim: true }), ...(action.destructive === true ? { danger: true } : {}),
    press: action.enabled ? run(action.id) : NONE,
  }))

export interface SteamContent {
  readonly sub: string
  readonly rows: RowSpec[]
  readonly buttons: ButtonSpec[]
  readonly side?: PanelSide
  /** Back: Korri's Cancel while a sign-in runs, else the page's own. */
  readonly cancel?: Act
}

/** The page's words, rows, buttons and code for one state. */
export function steamContent(account: SurfaceSteamAccount): SteamContent {
  const note = (id: string, label: string): RowSpec => ({ id, kind: "note", label })
  const cancel = (actions: readonly SurfaceAction[]): Act | undefined => {
    // A sign-in under way has one way out, Korri's; Back takes it only when it is the only one.
    const enabled = actions.filter(action => action.enabled)
    return enabled.length === 1 ? run(enabled[0]!.id) : undefined
  }
  switch (account._tag) {
    case "SignedOut":
      return {
        sub: "Sign in once on this device to play your Steam games.",
        rows: [note("st:how", "You approve the sign-in in the Steam app on your phone. No password is typed here.")],
        buttons: buttons(account.actions),
      }
    case "Code": {
      const path = qrPath(account.qr)
      const lines = [account.kicker, ...(account.detail === undefined ? [] : [account.detail])]
      return {
        sub: account.forGame === undefined ? "" : `${account.forGame} needs your Steam sign-in.`,
        rows: path === "" ? [{ id: "st:bad", kind: "problem", label: "Korri sent a sign-in code this page cannot draw." }] : [note("st:fresh", "The code changes every few seconds. Scan the one shown now.")],
        buttons: buttons(account.actions),
        side: { kind: "qr", title: account.kicker, lines: lines.slice(1), qr: account.qr },
        ...(cancel(account.actions) === undefined ? {} : { cancel: cancel(account.actions)! }),
      }
    }
    case "Working":
      return {
        sub: account.forGame === undefined ? "" : `${account.forGame} starts when you are signed in.`,
        rows: [note("st:working", account.kicker)],
        buttons: buttons(account.actions),
        ...(cancel(account.actions) === undefined ? {} : { cancel: cancel(account.actions)! }),
      }
    case "SignedIn":
      return {
        sub: `Signed in as ${account.accountName}.`,
        rows: [note("st:in", "Your Steam games play on this device with this account.")],
        buttons: buttons(account.actions),
      }
    case "Problem":
      return {
        sub: account.kicker,
        rows: [{ id: "st:problem", kind: "problem", label: account.reason }],
        buttons: buttons(account.actions),
      }
  }
}
