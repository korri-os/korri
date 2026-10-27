/** Host-owned shared-play actions. Surfaces receive opaque ids, never RPCs. */
import type { Invite, InviteNetplay, NetplayStatus, RpcFailure, StreamPlayEnded } from "@contracts/generated/korrid"
import type { SurfaceAction, SurfaceInvite, SurfaceStatus } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"
import { entryKey, type LaunchablesState } from "../launchables/state"
import { formatBytes } from "./acquisition-gate"
import {
  initialPluginFacts,
  inspectActionId,
  reviewActionId,
  reviewFor,
  reviewMessage,
  type PluginFacts,
  type PluginRepositoryClient,
  type ReleaseTarget,
} from "./plugin-repositories"
import { createPluginRepositoriesController } from "./plugin-repositories-controller"

const STREAM = "invite:stream:"
const NETPLAY = "invite:netplay:"
const DISMISS = "invite:dismiss:"
const CANCEL = "stream-play:cancel"
const LEAVE = "stream-play:leave:"
const NETPLAY_CANCEL = "netplay:cancel"
const NETPLAY_INSTALL = "netplay:install"
const NETPLAY_LEAVE = "netplay:leave:"

/** `RpcFailure.code` of `app.invites.netplay`, and `InviteNetplay.Unavailable.reason`
 * (services/korrid/src/netplay/receiver.rs, acquisition/netplay.rs). */
export const NetplayFailureCode = {
  NoNetplayRoute: "NoNetplayRoute",
  CopyFailed: "CopyFailed",
  NotEnoughSpace: "NotEnoughSpace",
  CoreVersionMismatch: "CoreVersionMismatch",
} as const

/** korrid's NotEnoughSpace message: "The copy needs 4700000000 bytes."
 * (services/korrid/src/acquisition/netplay.rs). */
const NEEDED_BYTES = /^The copy needs (\d+) bytes\.$/

/** The bytes a NotEnoughSpace message names, or nothing when it names none. */
export function neededBytes(message: string | undefined): number | undefined {
  const match = message === undefined ? null : NEEDED_BYTES.exec(message.trim())
  return match?.[1] === undefined ? undefined : Number(match[1])
}

function gameFacts(state: LaunchablesState, gameId: string | undefined): { id?: string; coverArtUrl?: string } {
  if (state._tag === "Loading" || gameId === undefined) return {}
  for (const entry of state.entries) {
    if (entry.kind === "now-playing") continue
    const copies = [entry.game, ...(entry.alternatives ?? []).map(copy => copy.game)]
    const game = copies.find(game => game.id === gameId)
    if (game !== undefined) return { id: entryKey(entry), ...("coverArtUrl" in game && typeof game.coverArtUrl === "string" ? { coverArtUrl: game.coverArtUrl } : {}) }
  }
  return {}
}

/**
 * The words of the netplay route korrid offers for an invite, or nothing
 * when it offers none: no `netplay` facts, or a reason the person can't fix.
 */
export function netplayLabel(netplay: InviteNetplay | undefined): string | undefined {
  switch (netplay?._tag) {
    case "Ready": return "Play here"
    case "NeedsCopy": return `Copy and play here (${formatBytes(netplay.bytes)})`
    case "NeedsPlugin": return `Install ${netplay.title} and play here`
    case "Unavailable":
    case undefined: return undefined
  }
}

export function surfaceInvitesFrom(invites: readonly Invite[], state: LaunchablesState): readonly SurfaceInvite[] {
  return invites.map(invite => {
    const netplay = netplayLabel(invite.netplay)
    return {
      id: invite.inviteId,
      personName: invite.personName,
      deviceLabel: invite.deviceLabel,
      gameTitle: invite.session.title ?? invite.session.gameId ?? "Game",
      ...(gameFacts(state, invite.session.gameId).coverArtUrl === undefined ? {} : { coverArtUrl: gameFacts(state, invite.session.gameId).coverArtUrl }),
      actions: [
        { id: STREAM + invite.inviteId, label: "Play on stream", enabled: true },
        ...(netplay === undefined ? [] : [{ id: NETPLAY + invite.inviteId, label: netplay, enabled: true }]),
        { id: DISMISS + invite.inviteId, label: "Dismiss", enabled: true },
      ],
    }
  })
}

export function streamEndReason(ended: StreamPlayEnded): string {
  return ended.cause === "deviceUnreachable" ? `Lost ${ended.deviceLabel}`
    : `${ended.gameTitle ?? "The session"} ended on ${ended.deviceLabel}`
}

/**
 * Paused and Ended of shared play this device takes part in. Ended covers
 * both kinds: korrid reports a netplay session that ended as it reports a
 * stream that ended (`streamPlayEnded`), so the words name neither.
 */
export function streamStatusFrom(state: LaunchablesState): SurfaceStatus | undefined {
  if (state._tag !== "Ready") return undefined
  const active = state.entries.find(entry => entry.kind === "now-playing" && entry.session.launchId === state.streamPlayLaunchId)
  if (active?.kind === "now-playing" && active.session.phase === "frozen") return {
    _tag: "Paused",
    kicker: `Paused on ${active.session.host ?? "the other device"}`,
    actions: [{ id: LEAVE + active.session.launchId, label: "Leave", enabled: true }],
    ...(gameFacts(state, active.session.gameId).id === undefined ? {} : { gameId: gameFacts(state, active.session.gameId).id }),
  }
  const ended = state.streamPlayEnded
  if (ended === undefined) return undefined
  return { _tag: "Ended", kicker: "Session ended", reason: streamEndReason(ended),
    ...(gameFacts(state, ended.gameId).id === undefined ? {} : { gameId: gameFacts(state, ended.gameId).id }),
    ...(ended.gameTitle === undefined ? {} : { gameTitle: ended.gameTitle }),
  }
}

const netplayCancel: SurfaceAction = { id: NETPLAY_CANCEL, label: "Cancel", enabled: true }

/** "42% of 512 MB": how far the copy is. */
export function copyDetail(done: number, total: number): string {
  const percent = total > 0 ? Math.min(100, Math.floor((done * 100) / total)) : 0
  return `${percent}% of ${formatBytes(total)}`
}

/**
 * What this device's netplay shows while korrid reports it. Playing shows
 * nothing: the game has the screen.
 */
export function netplayStatusFrom(observation: NetplayStatus, gameId?: string): SurfaceStatus | undefined {
  const device = observation.deviceLabel
  const owned = gameId === undefined ? {} : { gameId }
  const phase = observation.phase
  switch (phase._tag) {
    case "Copying": return {
      _tag: "Busy", kicker: `Copying ${observation.gameTitle ?? "the game"} from ${device}`,
      detail: copyDetail(phase.done, phase.total), progress: { done: phase.done, total: phase.total },
      actions: [netplayCancel], ...owned,
    }
    case "Connecting": return { _tag: "Busy", kicker: `Connecting to ${device}`, actions: [netplayCancel], ...owned }
    case "Reconnecting": return { _tag: "Busy", kicker: `Reconnecting to ${device}`, ...owned }
    case "HolderLeft": return {
      _tag: "Paused", kicker: `Paused on ${device}`,
      actions: [{ id: NETPLAY_LEAVE + observation.launchId, label: "Leave", enabled: true }], ...owned,
    }
    case "Playing": return undefined
  }
}

/** Who and what a netplay problem is about, for its words. */
export interface NetplayProblemFacts {
  readonly gameTitle: string
  readonly deviceLabel: string
  /** The core's plugin title, when this device just installed it. */
  readonly core?: string
}

const STREAM_STILL_WORKS = "Play on stream still works."

/** The words for a netplay failure code, from `app.invites.netplay` or an
 * invite's `Unavailable.reason`. Undefined for a code Korri has no words for. */
export function netplayReason(code: string, facts: NetplayProblemFacts, message?: string): string | undefined {
  switch (code) {
    case NetplayFailureCode.NoNetplayRoute:
      return `${facts.gameTitle} can't play here with ${facts.deviceLabel} right now. ${STREAM_STILL_WORKS}`
    case NetplayFailureCode.CopyFailed:
      return `${facts.gameTitle} did not copy from ${facts.deviceLabel}. Nothing was kept.`
    case NetplayFailureCode.NotEnoughSpace: {
      const bytes = neededBytes(message)
      return bytes === undefined
        ? `${facts.gameTitle} needs more free space on this device.`
        : `${facts.gameTitle} needs ${formatBytes(bytes)} free on this device.`
    }
    case NetplayFailureCode.CoreVersionMismatch:
      return `${facts.deviceLabel} runs a different version of ${facts.core ?? "the game's core"}. ${STREAM_STILL_WORKS}`
    default:
      return undefined
  }
}

const KICKERS: Readonly<Record<string, string>> = {
  [NetplayFailureCode.CopyFailed]: "Copy failed",
  [NetplayFailureCode.NotEnoughSpace]: "Not enough space",
}

export function netplayProblem(failure: RpcFailure, facts: NetplayProblemFacts): Extract<SurfaceStatus, { _tag: "Problem" }> {
  return {
    _tag: "Problem",
    kicker: KICKERS[failure.code] ?? "Netplay unavailable",
    reason: netplayReason(failure.code, facts, failure.message) ?? failure.message,
    canRetry: false,
  }
}

/** Newest first: "v10" after "v9", "2026.10.2" after "2026.9.30". */
const newerFirst = (a: string, b: string) => b.localeCompare(a, undefined, { numeric: true })

/**
 * The release of `pluginId` to install: the newest version any configured
 * repository offers from a trusted publisher. Plugin ids are global, so the
 * same id in two repositories is the same plugin.
 */
export function newestRelease(facts: PluginFacts, pluginId: string): ReleaseTarget | undefined {
  const offers: ReleaseTarget[] = []
  for (const [sourceUrl, catalog] of Object.entries(facts.catalogs)) {
    if (catalog._tag !== "Ready") continue
    for (const entry of catalog.entries) {
      if (entry.pluginId === pluginId && entry.publisherTrusted) offers.push({ sourceUrl, pluginId, releaseVersion: entry.releaseVersion })
    }
  }
  return offers.sort((a, b) => newerFirst(a.releaseVersion, b.releaseVersion))[0]
}

/* ---------------------------------------------------------------- model */

/** What a netplay start needs to show itself before korrid reports it. */
export interface NetplayStart {
  readonly inviteId: string
  readonly deviceLabel: string
  readonly gameTitle?: string
  /** The game as the surface names it, when this device's library holds it. */
  readonly gameId?: string
  /** Set when the release is copied first: the copy's size. */
  readonly copyBytes?: number
  /** The core's plugin title, when this device just installed it. */
  readonly core?: string
}

export type InstallStep = "Finding" | "Checking" | "Installing"

/**
 * Where shared play stands on this device: one case at a time. `op` names
 * the request a case waits for; a reply for another `op` is stale and
 * changes nothing. Cases that carry `netplay` hold korrid's last report of
 * this device's netplay (`SessionStatus.netplay`).
 */
export type SharedPlay =
  /** korrid's report has not been read since start, or since work ended. */
  | { readonly _tag: "Unread"; readonly gameId?: string }
  /** korrid reports no netplay, and nothing of the host's own is under way. */
  | { readonly _tag: "Idle" }
  /** korrid reports this device's netplay, whoever started it. */
  | { readonly _tag: "Observing"; readonly netplay: NetplayStatus; readonly gameId?: string }
  | { readonly _tag: "StreamStarting"; readonly op: number; readonly deviceLabel: string }
  /** `app.invites.netplay` is under way; korrid may already report it. */
  | { readonly _tag: "NetplayStarting"; readonly op: number; readonly start: NetplayStart; readonly netplay?: NetplayStatus }
  | { readonly _tag: "Installing"; readonly op: number; readonly invite: Invite; readonly title: string; readonly step: InstallStep }
  /** Settings' review waits for the person's answer. `facts` hold the
   * reviewed report, so the install passes exactly its approval. */
  | {
      readonly _tag: "Reviewing"
      readonly op: number
      readonly invite: Invite
      readonly title: string
      readonly target: ReleaseTarget
      readonly facts: PluginFacts
      readonly question: Extract<SurfaceStatus, { _tag: "Confirm" }>
    }
  /**
   * Cancel was pressed on a stream start. A far session cannot be
   * cancelled: the start request (`pending`) answers first, then its viewer
   * leaves.
   */
  | { readonly _tag: "Cancelling"; readonly kind: "stream"; readonly pending: number }
  /**
   * Cancel was pressed on a netplay. `pending` is the start request still
   * under way, if any: its reply ends the cancel; without one, korrid's
   * answer to the cancel does. The cancel names korrid's exact launch once
   * korrid reports it (`launchId`), and `sent` says app.session.cancel went
   * out for it.
   */
  | {
      readonly _tag: "Cancelling"
      readonly kind: "netplay"
      readonly pending?: number
      readonly launchId?: string
      readonly sent: boolean
    }
  | { readonly _tag: "Leaving"; readonly op: number; readonly kicker: string }
  | { readonly _tag: "Problem"; readonly status: Extract<SurfaceStatus, { _tag: "Problem" }> }

export type SharedPlayEvent =
  /** A session status read succeeded. */
  | { readonly _tag: "Read"; readonly netplay?: NetplayStatus }
  | { readonly _tag: "StreamRequested"; readonly op: number; readonly deviceLabel: string }
  | { readonly _tag: "NetplayRequested"; readonly op: number; readonly start: NetplayStart }
  | { readonly _tag: "InstallRequested"; readonly op: number; readonly invite: Invite; readonly title: string }
  | { readonly _tag: "InstallStepped"; readonly op: number; readonly step: InstallStep }
  | { readonly _tag: "ReviewReady"; readonly op: number; readonly target: ReleaseTarget; readonly facts: PluginFacts; readonly question: Extract<SurfaceStatus, { _tag: "Confirm" }> }
  | { readonly _tag: "Approved" }
  | { readonly _tag: "CancelPressed" }
  | { readonly _tag: "CancelSent" }
  | { readonly _tag: "LeaveRequested"; readonly op: number; readonly kicker: string }
  /** The work ended; korrid's next report decides. */
  | { readonly _tag: "Settled"; readonly gameId?: string }
  | { readonly _tag: "Failed"; readonly status: Extract<SurfaceStatus, { _tag: "Problem" }> }
  | { readonly _tag: "Dismissed" }

/** A netplay korrid reports that Cancel can still stop. */
const cancellable = (netplay: NetplayStatus | undefined): netplay is NetplayStatus =>
  netplay?.phase._tag === "Copying" || netplay?.phase._tag === "Connecting"

/** The cases with work of the host's own under way. */
export const sharedPlayBusy = (model: SharedPlay): boolean =>
  model._tag === "StreamStarting" || model._tag === "NetplayStarting" || model._tag === "Installing" ||
  model._tag === "Reviewing" || model._tag === "Cancelling" || model._tag === "Leaving"

/** Every transition. Pure: effects follow from the case it returns. */
export function sharedPlayStep(model: SharedPlay, event: SharedPlayEvent): SharedPlay {
  switch (event._tag) {
    case "Read":
      switch (model._tag) {
        case "Unread":
        case "Idle":
        case "Observing": {
          if (event.netplay === undefined) return { _tag: "Idle" }
          const gameId = model._tag === "Unread" ? model.gameId
            : model._tag === "Observing" && model.netplay.launchId === event.netplay.launchId ? model.gameId : undefined
          return { _tag: "Observing", netplay: event.netplay, ...(gameId === undefined ? {} : { gameId }) }
        }
        case "NetplayStarting": {
          const { netplay: _, ...rest } = model
          return event.netplay === undefined ? rest : { ...rest, netplay: event.netplay }
        }
        case "Cancelling":
          return model.kind === "netplay" && model.launchId === undefined && event.netplay !== undefined
            ? { ...model, launchId: event.netplay.launchId } : model
        default:
          return model
      }
    case "StreamRequested":
      return sharedPlayBusy(model) ? model : { _tag: "StreamStarting", op: event.op, deviceLabel: event.deviceLabel }
    case "NetplayRequested":
      return sharedPlayBusy(model) && !(model._tag === "Installing" && model.step === "Installing")
        ? model : { _tag: "NetplayStarting", op: event.op, start: event.start }
    case "InstallRequested":
      return sharedPlayBusy(model) ? model
        : { _tag: "Installing", op: event.op, invite: event.invite, title: event.title, step: "Finding" }
    case "InstallStepped":
      return model._tag === "Installing" && model.op === event.op ? { ...model, step: event.step } : model
    case "ReviewReady":
      return model._tag === "Installing" && model.op === event.op
        ? { _tag: "Reviewing", op: model.op, invite: model.invite, title: model.title, target: event.target, facts: event.facts, question: event.question }
        : model
    case "Approved":
      return model._tag === "Reviewing"
        ? { _tag: "Installing", op: model.op, invite: model.invite, title: model.title, step: "Installing" } : model
    case "CancelPressed":
      switch (model._tag) {
        case "Reviewing":
          return { _tag: "Unread" }
        case "Installing":
          // Installing has no Cancel: the plugin is half in place until it ends.
          return model.step === "Installing" ? model : { _tag: "Unread" }
        case "StreamStarting":
          return { _tag: "Cancelling", kind: "stream", pending: model.op }
        case "NetplayStarting":
          return { _tag: "Cancelling", kind: "netplay", pending: model.op, sent: false,
            ...(model.netplay === undefined ? {} : { launchId: model.netplay.launchId }) }
        case "Observing":
          // No request of ours is under way (it answered, or the portal
          // reloaded): korrid's report names the launch to cancel.
          return cancellable(model.netplay)
            ? { _tag: "Cancelling", kind: "netplay", launchId: model.netplay.launchId, sent: false } : model
        default:
          return model
      }
    case "CancelSent":
      return model._tag === "Cancelling" && model.kind === "netplay" ? { ...model, sent: true } : model
    case "LeaveRequested":
      return sharedPlayBusy(model) ? model : { _tag: "Leaving", op: event.op, kicker: event.kicker }
    case "Settled":
      return { _tag: "Unread", ...(event.gameId === undefined ? {} : { gameId: event.gameId }) }
    case "Failed":
      return { _tag: "Problem", status: event.status }
    case "Dismissed":
      return model._tag === "Problem" ? { _tag: "Unread" } : model
  }
}

/** What the model shows. Playing shows nothing: the game has the screen. */
export function sharedPlayStatus(model: SharedPlay): SurfaceStatus | undefined {
  switch (model._tag) {
    case "Unread":
    case "Idle":
      return undefined
    case "Observing":
      return netplayStatusFrom(model.netplay, model.gameId)
    case "StreamStarting":
      return { _tag: "Busy", kicker: `Connecting to ${model.deviceLabel}`, actions: [{ id: CANCEL, label: "Cancel", enabled: true }] }
    case "NetplayStarting": {
      if (model.netplay !== undefined) return netplayStatusFrom(model.netplay, model.start.gameId)
      const { start } = model
      return netplayStatusFrom({
        launchId: "", deviceLabel: start.deviceLabel,
        ...(start.gameTitle === undefined ? {} : { gameTitle: start.gameTitle }),
        phase: start.copyBytes === undefined ? { _tag: "Connecting" } : { _tag: "Copying", done: 0, total: start.copyBytes },
      }, start.gameId)
    }
    case "Installing":
      return model.step === "Installing" ? { _tag: "Busy", kicker: `Installing ${model.title}` }
        : { _tag: "Busy", kicker: `${model.step} ${model.title}`, actions: [netplayCancel] }
    case "Reviewing":
      return model.question
    case "Cancelling":
      return { _tag: "Busy", kicker: model.kind === "netplay" ? "Cancelling" : "Leaving stream" }
    case "Leaving":
      return { _tag: "Busy", kicker: model.kicker }
    case "Problem":
      return model.status
  }
}

/** Whether korrid's report is worth reading every second. */
const watching = (model: SharedPlay): boolean =>
  model._tag === "Unread" || model._tag === "Observing" || model._tag === "NetplayStarting" ||
  (model._tag === "Cancelling" && model.kind === "netplay")

const problemOf = (reason: string, kicker: string): Extract<SurfaceStatus, { _tag: "Problem" }> =>
  ({ _tag: "Problem", kicker, reason, canRetry: false })

/* ---------------------------------------------------------------- effects */

export function createSharedPlay(korrid: KorridClient, context: {
  readonly state: () => LaunchablesState
  readonly refreshSession: () => Promise<void>
  /** A plugin was installed: the library re-reads what it can play. */
  readonly pluginsChanged?: () => void
}) {
  let snapshot: { readonly invites: readonly Invite[]; readonly status?: SurfaceStatus } = { invites: [] }
  const listeners = new Set<() => void>()
  let disposed = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let refreshId = 0
  let nextOp = 0
  let invites: readonly Invite[] = []
  let model: SharedPlay = { _tag: "Unread" }
  const publish = () => {
    const status = sharedPlayStatus(model)
    const next = { invites, ...(status === undefined ? {} : { status }) }
    if (disposed || JSON.stringify(next) === JSON.stringify(snapshot)) return
    snapshot = next
    for (const listener of listeners) listener()
  }
  /** One transition, then the effect its case asks for. */
  const dispatch = (event: SharedPlayEvent) => {
    model = sharedPlayStep(model, event)
    publish()
    // A netplay cancel goes to korrid's exact launch as soon as it is known.
    if (model._tag === "Cancelling" && model.kind === "netplay" && model.launchId !== undefined && !model.sent) {
      const launchId = model.launchId
      const waiting = model.pending
      dispatch({ _tag: "CancelSent" })
      void korrid.sessionCancel({ expectedLaunchId: launchId }).then(async () => {
        // With a request still under way, its reply ends the cancel.
        if (disposed || waiting !== undefined || model._tag !== "Cancelling") return
        await settle()
      })
    }
  }
  const refresh = async () => {
    const id = ++refreshId
    const result = await korrid.invitesList()
    if (disposed || id !== refreshId) return
    invites = result._tag === "Ok" ? result.payload.invites : []
    publish()
  }
  /** korrid's report of this device's netplay. A failed read changes nothing. */
  const observe = async () => {
    const status = await korrid.sessionStatus()
    if (disposed || status._tag !== "Ok") return
    dispatch({ _tag: "Read", ...(status.payload.netplay === undefined ? {} : { netplay: status.payload.netplay }) })
  }
  /** The work ended: korrid's report and the invites decide what shows. */
  const settle = async (gameId?: string) => {
    dispatch({ _tag: "Settled", ...(gameId === undefined ? {} : { gameId }) })
    await context.refreshSession()
    if (disposed) return
    await observe()
    await refresh()
  }
  const fail = (reason: string, kicker: string) => dispatch({ _tag: "Failed", status: problemOf(reason, kicker) })
  /**
   * The reply to a start request, for the case that sent it. Cancelled: a
   * launch that started anyway is left, so the holder plays on.
   */
  const answered = async (op: number, outcome: Awaited<ReturnType<KorridClient["invitesStreamPlay"]>>,
    failed: (failure: RpcFailure) => void, kicker: string, gameId?: string) => {
    if (disposed) return
    const current = model
    if (current._tag === "Cancelling" && current.pending === op) {
      if (outcome._tag === "Ok") {
        const left = await korrid.sessionLeave(outcome.payload.launchId)
        if (disposed) return
        if (left._tag === "Err") { fail(left.payload.message, kicker); return }
      }
      await settle()
      return
    }
    if (!("op" in current) || current.op !== op) return
    if (outcome._tag === "Err") { failed(outcome.payload); return }
    await settle(gameId)
  }
  const play = async (invite: Invite) => {
    const op = ++nextOp
    dispatch({ _tag: "StreamRequested", op, deviceLabel: invite.deviceLabel })
    // Cancel cannot cancel a far session. Once the daemon's request settles,
    // Leave retires only this device's exact viewer and retains the far seats.
    const outcome = await korrid.invitesStreamPlay(invite.inviteId)
    await answered(op, outcome, failure => fail(failure.message, "Stream unavailable"), "Stream unavailable")
  }
  const facts = (invite: Invite, core?: string): NetplayProblemFacts => ({
    gameTitle: invite.session.title ?? invite.session.gameId ?? "The game",
    deviceLabel: invite.deviceLabel,
    ...(core === undefined ? {} : { core }),
  })
  /**
   * Copy when needed, then netplay: korrid does both in one request and
   * reports where it stands through the session status, read every second
   * meanwhile.
   */
  const netplay = async (invite: Invite, core?: string) => {
    const op = ++nextOp
    const gameId = gameFacts(context.state(), invite.session.gameId).id
    const route = invite.netplay
    dispatch({ _tag: "NetplayRequested", op, start: {
      inviteId: invite.inviteId,
      deviceLabel: invite.deviceLabel,
      ...(invite.session.title === undefined ? {} : { gameTitle: invite.session.title }),
      ...(gameId === undefined ? {} : { gameId }),
      ...(route?._tag === "NeedsCopy" ? { copyBytes: route.bytes } : {}),
      ...(core === undefined ? {} : { core }),
    } })
    const outcome = await korrid.invitesNetplay(invite.inviteId)
    await answered(op, outcome, failure => dispatch({ _tag: "Failed", status: netplayProblem(failure, facts(invite, core)) }),
      "Netplay unavailable", gameId)
  }
  /** Settings' own controller, on facts this flow holds for one step. */
  const controllerStep = (seed: PluginFacts, actionId: string | undefined) =>
    new Promise<{ readonly facts: PluginFacts; readonly failure?: string }>(resolve => {
      let pluginFacts = seed
      let locked = false
      const controller = createPluginRepositoriesController(korrid satisfies PluginRepositoryClient, {
        facts: () => pluginFacts,
        publish: next => { pluginFacts = next },
        status: next => {
          if (next._tag === "Saving") return
          resolve({ facts: pluginFacts, ...(next._tag === "Problem" ? { failure: next.message } : {}) })
        },
        busy: { get: () => locked, set: value => { locked = value } },
        changed: () => context.pluginsChanged?.(),
        alive: () => !disposed,
      })
      if (actionId === undefined) void controller.refresh().then(() => resolve({ facts: pluginFacts }))
      else controller.run(actionId)
    })
  const still = (op: number, step: InstallStep) =>
    !disposed && model._tag === "Installing" && model.op === op && model.step === step
  /**
   * Install the plugin korrid names, then netplay: Settings' own inspect and
   * approve flow. The person reads the same report Settings shows before
   * anything installs; the install waits in Reviewing for their answer.
   */
  const install = async (invite: Invite, pluginId: string, title: string) => {
    const op = ++nextOp
    dispatch({ _tag: "InstallRequested", op, invite, title })
    const found = await controllerStep(initialPluginFacts, undefined)
    if (!still(op, "Finding")) return
    const target = newestRelease(found.facts, pluginId)
    if (target === undefined) { fail(`No plugin repository on this device offers ${title}.`, "Install failed"); return }
    dispatch({ _tag: "InstallStepped", op, step: "Checking" })
    const checked = await controllerStep(found.facts, inspectActionId(target))
    if (!still(op, "Checking")) return
    const review = reviewFor(checked.facts, target)
    if (checked.failure !== undefined || review === undefined) { fail(checked.failure ?? `${title} could not be checked.`, "Install failed"); return }
    const gameId = gameFacts(context.state(), invite.session.gameId).id
    dispatch({ _tag: "ReviewReady", op, target, facts: checked.facts, question: {
      _tag: "Confirm",
      kicker: `Install ${review.report.title ?? title}?`,
      detail: reviewMessage(review.report),
      actions: [{ id: NETPLAY_INSTALL, label: "Install and play here", enabled: true }, netplayCancel],
      // A stray press must never approve a plugin or its permissions.
      declineActionId: NETPLAY_CANCEL,
      ...(gameId === undefined ? {} : { gameId }),
    } })
  }
  /** The person said yes to the review in `reviewing`. */
  const approve = async (reviewing: Extract<SharedPlay, { _tag: "Reviewing" }>) => {
    const { op, invite, title, target, facts: reviewed } = reviewing
    dispatch({ _tag: "Approved" })
    const installed = await controllerStep(reviewed, reviewActionId(target))
    if (!still(op, "Installing")) return
    if (installed.failure !== undefined) { fail(installed.failure, "Install failed"); return }
    // korrid decides the route again with the new core; a version that
    // differs from the holder's leaves stream play only.
    await refresh()
    if (!still(op, "Installing")) return
    const again = invites.find(candidate => candidate.inviteId === invite.inviteId)
    if (again === undefined) { await settle(); return }
    const route = again.netplay
    if (route?._tag === "Unavailable") {
      dispatch({ _tag: "Failed", status: netplayProblem({ code: route.reason, message: route.reason }, facts(again, title)) })
      return
    }
    await netplay(again, title)
  }
  const leave = async (launchId: string, kicker: string, failed: string) => {
    const op = ++nextOp
    dispatch({ _tag: "LeaveRequested", op, kicker })
    const outcome = await korrid.sessionLeave(launchId)
    if (disposed || model._tag !== "Leaving" || model.op !== op) return
    if (outcome._tag === "Err") { fail(outcome.payload.message, failed); return }
    await settle()
  }
  /** Cancel or Back on the work in progress; true when it was the host's. */
  const cancel = (): boolean => {
    const before = model
    dispatch({ _tag: "CancelPressed" })
    return model !== before || sharedPlayBusy(before)
  }
  const heldBy = (): NetplayStatus | undefined =>
    model._tag === "Observing" && model.netplay.phase._tag === "HolderLeft" ? model.netplay : undefined
  return {
    getSnapshot: () => snapshot,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener) } },
    /** The current case, for tests and diagnostics. */
    getModel: (): SharedPlay => model,
    start: () => {
      disposed = false
      const poll = async () => {
        await refresh()
        if (!disposed && watching(model)) await observe()
        if (!disposed) timer = setTimeout(() => void poll(), 1000)
      }
      void poll()
      return () => { disposed = true; clearTimeout(timer); ++refreshId }
    },
    act: (id: string): boolean => {
      if (id === CANCEL || id === NETPLAY_CANCEL) return cancel()
      if (id === NETPLAY_INSTALL) {
        if (model._tag !== "Reviewing") return false
        void approve(model)
        return true
      }
      const held = heldBy()
      if (held !== undefined && id === NETPLAY_LEAVE + held.launchId) {
        void leave(held.launchId, "Leaving", "Netplay unavailable"); return true
      }
      const paused = streamStatusFrom(context.state())
      if (paused?._tag === "Paused" && paused.actions[0]?.id === id && !sharedPlayBusy(model)) {
        void leave(id.slice(LEAVE.length), "Leaving stream", "Stream unavailable"); return true
      }
      const invite = snapshot.invites.find(invite => [STREAM, NETPLAY, DISMISS].some(prefix => id === prefix + invite.inviteId))
      if (invite === undefined) return false
      if (sharedPlayBusy(model) || context.state()._tag !== "Ready") return true
      if (id === STREAM + invite.inviteId) void play(invite)
      else if (id === NETPLAY + invite.inviteId) {
        const route = invite.netplay
        if (route?._tag === "NeedsPlugin") void install(invite, route.pluginId, route.title)
        else if (route?._tag === "Ready" || route?._tag === "NeedsCopy") void netplay(invite)
      } else void korrid.invitesDismiss(invite.inviteId).then(async outcome => {
        if (outcome._tag === "Err") fail(outcome.payload.message, "Stream unavailable")
        else await refresh()
      })
      return true
    },
    back: (): boolean => {
      if (cancel()) return true
      const held = heldBy()
      if (held !== undefined) { void leave(held.launchId, "Leaving", "Netplay unavailable"); return true }
      // A drop is korrid's to retry; Back waits with it.
      if (model._tag === "Observing" && model.netplay.phase._tag === "Reconnecting") return true
      const paused = streamStatusFrom(context.state())
      if (paused?._tag !== "Paused") return false
      void leave(paused.actions[0]!.id.slice(LEAVE.length), "Leaving stream", "Stream unavailable")
      return true
    },
    dismiss: (): boolean => {
      if (model._tag !== "Problem") return false
      dispatch({ _tag: "Dismissed" })
      return true
    },
  }
}
