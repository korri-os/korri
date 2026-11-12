/**
 * The curtain: what stands over any screen while Korri's status is not
 * Browsing, while Korri's identity work has something to say, or while
 * Velvet asks before something that cannot be undone. The stage dims and
 * the words stand on it; there is no card.
 *
 *   Busy        Korri's kicker and detail, a launch's steps (one segment a
 *               step, "Step i of n · label"), measured progress, and
 *               Korri's own actions. Back runs Korri's declineActionId; with
 *               none it is swallowed.
 *   Problem     Korri's kicker and reason against the game it names; Dismiss
 *               holds focus, Retry beside it when Korri can. Back dismisses.
 *   Confirm     exactly Korri's actions; focus starts on its decline.
 *   ChooseSave  one button per side (device, when, how long), then Korri's
 *               actions. Velvet never picks a side.
 *   Paused      Korri's actions while another device paused the stream.
 *   Ended       Korri's reason; Dismiss.
 *   Running     Korri's kicker over the game's name; nothing to press.
 *
 * Velvet never infers what an action means from its label or id; the one
 * action it singles out is the one Korri marks (`declineActionId`).
 */
import type { SurfaceAction, SurfaceModel as Model, SurfaceStatus, SurfaceSteps } from "../../../../../contracts/generated/surface"
import { breakLines, textWidth } from "./fit"
import { gameById } from "./lists"
import { shortKey } from "./pages"
import type { Plan } from "./plan"
import { question } from "./settings"
import { NONE, send, seq, type Act, type Game, type State } from "./types"
import { bytes, duration, saved } from "./words"

export interface CurtainButton {
  readonly label: string
  readonly note?: string
  readonly danger?: boolean
  readonly dim?: boolean
  readonly act: Act
}

/** Whether work is under way, and how far it is when Korri measures it. */
export type Progress =
  | { readonly kind: "none" }
  /** Under way, with no end Korri knows. */
  | { readonly kind: "indeterminate" }
  /** `fraction` of the way, 0 to 1. */
  | { readonly kind: "measured"; readonly fraction: number }

const NO_PROGRESS: Progress = { kind: "none" }
const UNDER_WAY: Progress = { kind: "indeterminate" }

export interface CurtainSpec {
  /** Names the layer: a new key puts focus on `first` and the words at their top. */
  readonly key: string
  readonly kind: "busy" | "problem" | "ask" | "running"
  readonly kicker: string
  /** The game it is about, in Korri's words or from the library. */
  readonly name?: string
  readonly game?: Game
  readonly words: readonly string[]
  readonly steps?: SurfaceSteps
  readonly progress: Progress
  readonly progressLine?: string
  readonly buttons: readonly CurtainButton[]
  readonly first: number
  /** Buttons one under the other, whatever their width (the sides of a save split). */
  readonly column?: boolean
  /** What Back does; "swallow": nothing, and nothing under the curtain hears it either. */
  readonly back: Act | "swallow"
}

const run = (actionId: string): Act => send({ _tag: "RunAction", actionId })

const actionButtons = (actions: readonly SurfaceAction[]): CurtainButton[] =>
  actions.map(action => ({
    label: action.label, ...(action.description === undefined ? {} : { note: action.description }),
    ...(action.destructive === true ? { danger: true } : {}), ...(action.enabled ? {} : { dim: true }),
    act: action.enabled ? run(action.id) : NONE,
  }))

/** The action Korri marks as Back's on its status (contract 7.2). */
export function declineOf(status: SurfaceStatus): string | undefined {
  return "declineActionId" in status && status.declineActionId !== undefined && status.declineActionId !== "" ? status.declineActionId : undefined
}

function firstOf(actions: readonly SurfaceAction[], decline: string | undefined, offset = 0): number {
  const at = actions.findIndex(action => action.id === decline && action.enabled)
  if (at >= 0) return offset + at
  const enabled = actions.findIndex(action => action.enabled)
  return offset + Math.max(0, enabled)
}

function statusCurtain(state: State, model: Model): CurtainSpec | undefined {
  const status = model.status
  const game = "gameId" in status ? gameById(state.lists, model, status.gameId) ?? (state.detail?.game.id === status.gameId ? state.detail?.game : undefined) : undefined
  const named = game === undefined ? {} : { game, name: game.title }
  const decline = declineOf(status)
  switch (status._tag) {
    case "Browsing":
      return undefined
    case "Busy": {
      const actions = status.actions ?? []
      const progress = status.progress
      return {
        key: `busy:${status.kicker}:${status.gameId ?? ""}`, kind: "busy", kicker: status.kicker, ...named,
        words: status.detail === undefined ? [] : [status.detail],
        ...(status.steps === undefined || status.steps.labels.length === 0 ? {} : { steps: status.steps }),
        // Steps say where the work is; with neither steps nor a measure the bar only says it is under way.
        progress: progress === undefined ? (status.steps === undefined ? UNDER_WAY : NO_PROGRESS)
          : progress.total === undefined || progress.total <= 0 ? UNDER_WAY
            : { kind: "measured", fraction: Math.max(0, Math.min(1, progress.done / progress.total)) },
        ...(progress === undefined ? {} : { progressLine: progress.total === undefined ? `${bytes(progress.done)} so far` : `${bytes(progress.done)} of ${bytes(progress.total)}` }),
        buttons: actionButtons(actions), first: firstOf(actions, decline),
        back: decline === undefined ? "swallow" : run(decline),
      }
    }
    case "Confirm":
      return {
        key: `confirm:${status.kicker}`, kind: "ask", kicker: status.kicker, ...named, words: status.detail === undefined ? [] : [status.detail], progress: NO_PROGRESS,
        buttons: actionButtons(status.actions), first: firstOf(status.actions, decline), back: decline === undefined ? "swallow" : run(decline),
      }
    case "ChooseSave": {
      const sides: CurtainButton[] = status.sides.map(side => ({ label: side.device, note: `${saved(side.endedAt, model.publishedAt)}, ${duration(side.durationSeconds)} session`, act: run(side.actionId) }))
      return {
        key: `saves:${status.gameId}`, kind: "ask", kicker: status.kicker, ...named, words: status.detail === undefined ? [] : [status.detail], progress: NO_PROGRESS,
        buttons: [...sides, ...actionButtons(status.actions)], first: firstOf(status.actions, decline, sides.length), column: true,
        back: decline === undefined ? "swallow" : run(decline),
      }
    }
    case "Paused":
      return {
        key: `paused:${status.kicker}`, kind: "ask", kicker: status.kicker, ...named, words: status.detail === undefined ? [] : [status.detail], progress: NO_PROGRESS,
        buttons: actionButtons(status.actions), first: firstOf(status.actions, decline), back: decline === undefined ? "swallow" : run(decline),
      }
    case "Running":
      return { key: `running:${status.gameId ?? ""}`, kind: "running", kicker: status.kicker, ...named, words: [], progress: NO_PROGRESS, buttons: [], first: 0, back: "swallow" }
    case "Ended": {
      const dismiss = send({ _tag: "Dismiss" })
      return {
        key: `ended:${status.reason}`, kind: "problem", kicker: status.kicker, ...(status.gameTitle === undefined ? named : { ...named, name: status.gameTitle }),
        words: [status.reason], progress: NO_PROGRESS, buttons: [{ label: "Dismiss", act: dismiss }], first: 0, back: dismiss,
      }
    }
    case "Problem": {
      const dismiss = send({ _tag: "Dismiss" })
      return {
        key: `problem:${status.kicker}:${status.reason}`, kind: "problem", kicker: status.kicker, ...(status.gameTitle === undefined ? named : { ...named, name: status.gameTitle }),
        words: [status.reason], progress: NO_PROGRESS,
        buttons: [{ label: "Dismiss", act: dismiss }, ...(status.canRetry ? [{ label: "Try again", act: send({ _tag: "Retry" }) }] : [])], first: 0, back: dismiss,
      }
    }
  }
}

const IDLE: Act = { t: "identity", flow: { f: "idle" } }

/** Korri's identity work, and the two answers a switch needs before it is sent. */
function identityCurtain(state: State, model: Model): CurtainSpec | undefined {
  const identity = model.identityManagement
  if (identity === undefined) return undefined
  const onPage = state.pages[state.pages.length - 1]?.p === "identity"
  const flow = state.identity
  if (flow.f === "disposition") {
    return {
      key: "identity:disposition", kind: "ask", kicker: "This device's records", progress: NO_PROGRESS,
      words: ["Transfer keeps this device's play history, saves and settings under the new identity.", "Delete removes them from this device."],
      buttons: [
        { label: "Cancel", act: IDLE },
        { label: "Transfer", act: { t: "identity", flow: { f: "trust", source: flow.source, disposition: "transfer" } } },
        { label: "Delete", danger: true, act: { t: "identity", flow: { f: "trust", source: flow.source, disposition: "delete" } } },
      ],
      first: 0, back: IDLE,
    }
  }
  if (flow.f === "trust") {
    const source = flow.source
    const request = source.kind === "backup"
      ? send({ _tag: "SwitchIdentityFromBackup", encryptedSecret: source.encryptedSecret, password: source.password, disposition: flow.disposition, trustLossConfirmed: true })
      : send({ _tag: "SwitchIdentityToNip46", bunkerUri: source.bunkerUri, disposition: flow.disposition, trustLossConfirmed: true })
    return {
      key: "identity:trust", kind: "ask", kicker: "Switch identity?", progress: NO_PROGRESS,
      words: ["Your other devices stop trusting this one until you approve it again."],
      buttons: [{ label: "Cancel", act: IDLE }, { label: "Switch", danger: true, act: seq(request, IDLE) }], first: 0, back: IDLE,
    }
  }
  if (!onPage) return undefined
  const status = identity.status
  const dismiss = seq(send({ _tag: "DismissIdentityStatus" }), IDLE)
  switch (status._tag) {
    case "Working":
      return { key: `identity:working:${status.operation}`, kind: "busy", kicker: status.operation, words: [], progress: UNDER_WAY, buttons: [], first: 0, back: "swallow" }
    case "Problem":
      return { key: `identity:problem:${status.message}`, kind: "problem", kicker: "That did not work", words: [status.message], progress: NO_PROGRESS, buttons: [{ label: "Dismiss", act: dismiss }], first: 0, back: dismiss }
    case "Switched":
      return { key: `identity:switched:${status.ownerPublicKey}`, kind: "ask", kicker: "Identity switched", words: ["This device now belongs to", shortKey(status.ownerPublicKey)], progress: NO_PROGRESS, buttons: [{ label: "Done", act: dismiss }], first: 0, back: dismiss }
    default:
      return undefined
  }
}

function askCurtain(state: State): CurtainSpec | undefined {
  const ask = state.ask
  if (ask === undefined) return undefined
  return {
    key: `ask:${ask.spec.title}:${ask.spec.buttons.length}`, kind: "ask", kicker: ask.spec.title, words: ask.spec.lines, progress: NO_PROGRESS,
    buttons: ask.spec.buttons.map(button => ({ label: button.label, ...(button.danger === true ? { danger: true } : {}), act: seq({ t: "ask", ask: CLOSED }, button.act) })),
    first: 0, back: { t: "ask", ask: CLOSED },
  }
}

/** The act that puts Velvet's question away: an ask with no buttons. */
export const CLOSED = { title: "", lines: [], buttons: [] } as const

let last: { state: State; model: Model; curtain: CurtainSpec | undefined } | undefined

/** The curtain that is up: Korri's status first, then Velvet's question, then Korri's identity work. */
export function curtainOf(state: State, model: Model): CurtainSpec | undefined {
  if (last !== undefined && last.state === state && last.model === model) return last.curtain
  // The gameplay overlay has its own page; Korri's status still stands over it.
  const curtain = statusCurtain(state, model) ?? askCurtain(state) ?? identityCurtain(state, model)
  last = { state, model, curtain }
  return curtain
}

export interface CurtainLayout {
  /** Every line of the words, broken to the curtain's width. */
  readonly lines: readonly string[]
  /** How many lines the window shows at once. */
  readonly room: number
  readonly stack: boolean
}

/** How the curtain's words and buttons stand in the plan's window. */
export function curtainLayout(spec: CurtainSpec, plan: Plan): CurtainLayout {
  const lines = spec.words.flatMap(paragraph => breakLines(paragraph, plan.body, plan.curtainW))
  const pill = (label: string) => plan.button * (textWidth(label, true) + 2.9)
  const row = spec.buttons.reduce((sum, button) => sum + pill(button.label), 0) + Math.max(0, spec.buttons.length - 1) * plan.button * 0.8
  const stack = spec.column === true || plan.narrow || row > plan.curtainW || spec.buttons.some(button => button.note !== undefined)
  // What the window has left for words: the kicker, the name, the bars and the buttons come first.
  const buttons = spec.buttons.length === 0 ? 0 : stack ? spec.buttons.length * plan.button * (spec.buttons.some(button => button.note !== undefined) ? 3.3 : 2.7) : plan.button * 2.7
  // The column as VelvetCurtain stands it: each part, and the gap between parts. The kicker
  // may take two lines, a problem its mark, and words that do not all fit the two lines
  // that say so. An estimate on the safe side: a line too few shows; a button under the
  // window's foot does not.
  const gap = plan.body * 0.7
  const fixedParts = [
    spec.name === undefined ? 0 : plan.body * 1.2,
    plan.kicker * 2 + (spec.kind === "problem" ? plan.body * 1.8 : 0),
    spec.steps === undefined ? 0 : plan.body * 2.4,
    spec.progress.kind === "none" ? 0 : plan.body * 2,
    buttons,
  ].filter(part => part > 0)
  const fixed = fixedParts.reduce((sum, part) => sum + part, 0) + gap * fixedParts.length + plan.padY * 2
  const line = plan.body * 1.35
  const whole = Math.floor((plan.height - fixed) / line)
  const room = Math.max(1, lines.length <= whole ? whole : Math.floor((plan.height - fixed - 2 * (plan.body * 1.2 + gap)) / line))
  return { lines, room, stack }
}

export { question }
