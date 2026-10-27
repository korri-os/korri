/** Host-owned shared-play actions. Surfaces receive opaque ids, never RPCs. */
import type { Invite, RpcFailure, StreamPlayEnded } from "@contracts/generated/korrid"
import type { SurfaceAction, SurfaceInvite, SurfaceStatus } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"
import {
  NetplayFailureCode,
  inviteNetplayOf,
  netplayObservationOf,
  type InviteNetplay,
  type NetplayObservation,
} from "../korrid/netplay-contract"
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
    const netplay = netplayLabel(inviteNetplayOf(invite))
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
export function netplayStatusFrom(observation: NetplayObservation, gameId?: string): SurfaceStatus | undefined {
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
      const bytes = message !== undefined && /^\d+$/.test(message.trim()) ? Number(message.trim()) : undefined
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

export function netplayProblem(failure: RpcFailure, facts: NetplayProblemFacts): SurfaceStatus {
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

type Operation = {
  readonly kind: "stream" | "leave" | "netplay" | "install"
  cancelled: boolean
  /** Netplay: korrid has been asked to cancel the launch it reported. */
  cancelSent?: boolean
  /** Install: the review waits for the person's answer. */
  answer?: (install: boolean) => void
}

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
  let operation: Operation | undefined
  let refreshId = 0
  let invites: readonly Invite[] = []
  /** What korrid last reported of this device's netplay. Read at start, so a
   * reload finds a netplay in progress, then while one goes on. */
  let observation: NetplayObservation | undefined
  let observed = false
  /** The game the netplay in progress belongs to, as the surface names it. */
  let netplayGameId: string | undefined
  /** A question or failure of the host's own; outranks korrid's report. */
  let override: SurfaceStatus | undefined
  /** What a netplay start shows until korrid reports it. */
  let placeholder: SurfaceStatus | undefined
  const compose = () => {
    // Once korrid reports the netplay, its report decides; Playing shows nothing.
    const status = override ?? (observation === undefined ? placeholder : netplayStatusFrom(observation, netplayGameId))
    const next = { invites, ...(status === undefined ? {} : { status }) }
    if (disposed || JSON.stringify(next) === JSON.stringify(snapshot)) return
    snapshot = next
    for (const listener of listeners) listener()
  }
  const refresh = async () => {
    const id = ++refreshId
    const result = await korrid.invitesList()
    if (disposed || id !== refreshId) return
    invites = result._tag === "Ok" ? result.payload.invites : []
    compose()
  }
  /** korrid's report of this device's netplay. A failed read keeps the last. */
  const observe = async () => {
    const status = await korrid.sessionStatus()
    if (disposed) return
    observed = true
    if (status._tag === "Ok") observation = netplayObservationOf(status.payload)
    if (observation === undefined && operation?.kind !== "netplay") netplayGameId = undefined
    const active = operation
    if (active?.kind === "netplay" && active.cancelled && active.cancelSent !== true && observation !== undefined) {
      active.cancelSent = true
      void korrid.sessionCancel({ expectedLaunchId: observation.launchId })
    }
    compose()
  }
  const watching = () => !observed || observation !== undefined || operation?.kind === "netplay"
  const show = (status: SurfaceStatus | undefined) => { override = status; compose() }
  const busy = (kicker: string, actions?: readonly SurfaceAction[]) =>
    show({ _tag: "Busy", kicker, ...(actions === undefined ? {} : { actions }) })
  const problem = (message: string, kicker = "Stream unavailable") => show({
    _tag: "Problem", kicker, reason: message, canRetry: false,
  })
  /** The work ended: korrid's report and the invites decide what shows. */
  const settle = async () => {
    override = undefined
    placeholder = undefined
    await context.refreshSession()
    if (watching()) await observe()
    compose()
    await refresh()
  }
  const play = async (invite: Invite) => {
    const active: Operation = { kind: "stream", cancelled: false }
    operation = active
    busy(`Connecting to ${invite.deviceLabel}`, [{ id: CANCEL, label: "Cancel", enabled: true }])
    const outcome = await korrid.invitesStreamPlay(invite.inviteId)
    // Cancel cannot cancel a far session. Once the daemon's request settles,
    // Leave retires only this device's exact viewer and retains the far seats.
    if (outcome._tag === "Ok" && active.cancelled) {
      const left = await korrid.sessionLeave(outcome.payload.launchId)
      if (left._tag === "Err") { operation = undefined; problem(left.payload.message); return }
    }
    operation = undefined
    if (disposed) return
    if (outcome._tag === "Err" && !active.cancelled) { problem(outcome.payload.message); return }
    await settle()
  }
  const facts = (invite: Invite, core?: string): NetplayProblemFacts => ({
    gameTitle: invite.session.title ?? invite.session.gameId ?? "The game",
    deviceLabel: invite.deviceLabel,
    ...(core === undefined ? {} : { core }),
  })
  /**
   * Copy when needed, then netplay: korrid does both in one request and
   * reports where it stands through the session status, read every second
   * meanwhile. Cancel asks korrid to cancel the launch it reported; a launch
   * that started anyway is left, so the holder plays on.
   */
  const netplay = async (invite: Invite, core?: string) => {
    const active: Operation = { kind: "netplay", cancelled: false }
    operation = active
    netplayGameId = gameFacts(context.state(), invite.session.gameId).id
    override = undefined
    const route = inviteNetplayOf(invite)
    placeholder = route?._tag === "NeedsCopy"
      ? netplayStatusFrom({ launchId: "", deviceLabel: invite.deviceLabel, phase: { _tag: "Copying", done: 0, total: route.bytes },
        ...(invite.session.title === undefined ? {} : { gameTitle: invite.session.title }) }, netplayGameId)
      : netplayStatusFrom({ launchId: "", deviceLabel: invite.deviceLabel, phase: { _tag: "Connecting" } }, netplayGameId)
    compose()
    const outcome = await korrid.invitesNetplay(invite.inviteId)
    if (outcome._tag === "Ok" && active.cancelled) {
      const left = await korrid.sessionLeave(outcome.payload.launchId)
      if (left._tag === "Err") { operation = undefined; placeholder = undefined; problem(left.payload.message, "Netplay unavailable"); return }
    }
    operation = undefined
    if (disposed) return
    if (outcome._tag === "Err" && !active.cancelled) {
      placeholder = undefined
      observation = undefined
      show(netplayProblem(outcome.payload, facts(invite, core)))
      return
    }
    await settle()
  }
  /**
   * Install the plugin korrid names, then netplay: Settings' own inspect and
   * approve flow, run by its controller on this flow's facts. The person
   * reads the same report Settings shows before anything installs.
   */
  const install = async (invite: Invite, pluginId: string, title: string) => {
    const active: Operation = { kind: "install", cancelled: false }
    operation = active
    let pluginFacts: PluginFacts = initialPluginFacts
    let locked = false
    let settled: ((failure: string | undefined) => void) | undefined
    const controller = createPluginRepositoriesController(korrid satisfies PluginRepositoryClient, {
      facts: () => pluginFacts,
      publish: next => { pluginFacts = next },
      status: next => {
        if (next._tag === "Saving") return
        settled?.(next._tag === "Problem" ? next.message : undefined)
      },
      busy: { get: () => locked, set: value => { locked = value } },
      changed: () => context.pluginsChanged?.(),
      alive: () => !disposed,
    })
    const step = (actionId: string) => new Promise<string | undefined>(resolve => {
      settled = failure => { settled = undefined; resolve(failure) }
      controller.run(actionId)
    })
    const stopped = () => disposed || active.cancelled
    const fail = (reason: string) => {
      operation = undefined
      show({ _tag: "Problem", kicker: "Install failed", reason, canRetry: false })
    }
    const cancel = [netplayCancel]
    busy(`Finding ${title}`, cancel)
    await controller.refresh()
    if (stopped()) return
    const target = newestRelease(pluginFacts, pluginId)
    if (target === undefined) { fail(`No plugin repository on this device offers ${title}.`); return }
    busy(`Checking ${title}`, cancel)
    const inspected = await step(inspectActionId(target))
    if (stopped()) return
    const review = reviewFor(pluginFacts, target)
    if (inspected !== undefined || review === undefined) { fail(inspected ?? `${title} could not be checked.`); return }
    const yes = await new Promise<boolean>(resolve => {
      active.answer = resolve
      show({
        _tag: "Confirm",
        kicker: `Install ${review.report.title ?? title}?`,
        detail: reviewMessage(review.report),
        actions: [{ id: NETPLAY_INSTALL, label: "Install and play here", enabled: true }, netplayCancel],
        // A stray press must never approve a plugin or its permissions.
        declineActionId: NETPLAY_CANCEL,
        ...(gameFacts(context.state(), invite.session.gameId).id === undefined ? {} : { gameId: gameFacts(context.state(), invite.session.gameId).id }),
      })
    })
    active.answer = undefined
    if (!yes || disposed) return
    busy(`Installing ${title}`)
    const installed = await step(reviewActionId(target))
    if (disposed) return
    if (installed !== undefined) { fail(installed); return }
    // korrid decides the route again with the new core; a version that
    // differs from the holder's leaves stream play only.
    await refresh()
    if (disposed) return
    const again = invites.find(candidate => candidate.inviteId === invite.inviteId)
    if (again === undefined) { operation = undefined; await settle(); return }
    const route = inviteNetplayOf(again)
    if (route?._tag === "Unavailable") {
      operation = undefined
      show(netplayProblem({ code: route.reason, message: route.reason }, facts(again, title)))
      return
    }
    await netplay(again, title)
  }
  const leave = async (launchId: string, kicker: string, failed: string) => {
    operation = { kind: "leave", cancelled: false }
    busy(kicker)
    const outcome = await korrid.sessionLeave(launchId)
    operation = undefined
    if (disposed) return
    if (outcome._tag === "Err") { problem(outcome.payload.message, failed); return }
    await settle()
  }
  /** Stop the work in progress, as far as it can be stopped. */
  const cancel = (): boolean => {
    const active = operation
    if (active === undefined || active.kind === "leave") return false
    if (active.answer !== undefined) {
      active.answer(false)
      operation = undefined
      show(undefined)
      return true
    }
    // Installing has no Cancel: the plugin is half in place until it ends.
    if (active.kind === "install" && override?._tag === "Busy" && override.actions === undefined) return true
    active.cancelled = true
    if (active.kind === "install") { operation = undefined; show(undefined); return true }
    if (active.kind === "netplay") {
      if (observation !== undefined && active.cancelSent !== true) {
        active.cancelSent = true
        void korrid.sessionCancel({ expectedLaunchId: observation.launchId })
      }
      busy("Cancelling")
      return true
    }
    busy("Leaving stream")
    return true
  }
  const netplayPaused = () => observation?.phase._tag === "HolderLeft" ? observation : undefined
  return {
    getSnapshot: () => snapshot,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener) } },
    start: () => {
      disposed = false
      const poll = async () => {
        await refresh()
        if (!disposed && watching()) await observe()
        if (!disposed) timer = setTimeout(() => void poll(), 1000)
      }
      void poll()
      return () => { disposed = true; clearTimeout(timer); ++refreshId }
    },
    act: (id: string): boolean => {
      if (id === CANCEL || id === NETPLAY_CANCEL) return cancel() || operation !== undefined
      if (id === NETPLAY_INSTALL) {
        const answer = operation?.answer
        answer?.(true)
        return answer !== undefined
      }
      const held = netplayPaused()
      if (held !== undefined && id === NETPLAY_LEAVE + held.launchId && operation === undefined) {
        void leave(held.launchId, "Leaving", "Netplay unavailable"); return true
      }
      const paused = streamStatusFrom(context.state())
      if (paused?._tag === "Paused" && paused.actions[0]?.id === id && operation === undefined) {
        void leave(id.slice(LEAVE.length), "Leaving stream", "Stream unavailable"); return true
      }
      const invite = snapshot.invites.find(invite => [STREAM, NETPLAY, DISMISS].some(prefix => id === prefix + invite.inviteId))
      if (invite === undefined) return false
      if (operation !== undefined || context.state()._tag !== "Ready") return true
      if (id === STREAM + invite.inviteId) void play(invite)
      else if (id === NETPLAY + invite.inviteId) {
        const route = inviteNetplayOf(invite)
        if (route?._tag === "NeedsPlugin") void install(invite, route.pluginId, route.title)
        else if (route?._tag === "Ready" || route?._tag === "NeedsCopy") void netplay(invite)
      } else void korrid.invitesDismiss(invite.inviteId).then(async outcome => {
        if (outcome._tag === "Err") problem(outcome.payload.message)
        else await refresh()
      })
      return true
    },
    back: (): boolean => {
      if (operation !== undefined) return cancel() || true
      const held = netplayPaused()
      if (held !== undefined) { void leave(held.launchId, "Leaving", "Netplay unavailable"); return true }
      // A drop is korrid's to retry; Back waits with it.
      if (observation?.phase._tag === "Reconnecting") return true
      const paused = streamStatusFrom(context.state())
      if (paused?._tag !== "Paused") return false
      void leave(paused.actions[0]!.id.slice(LEAVE.length), "Leaving stream", "Stream unavailable")
      return true
    },
    dismiss: (): boolean => {
      if (override?._tag !== "Problem") return false
      show(undefined); return true
    },
  }
}
