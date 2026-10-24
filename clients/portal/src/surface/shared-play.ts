/** Host-owned shared-play actions. Surfaces receive opaque ids, never RPCs. */
import type { Invite, StreamPlayEnded } from "@contracts/generated/korrid"
import type { SurfaceInvite, SurfaceStatus } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"
import { entryKey, type LaunchablesState } from "../launchables/state"

const STREAM = "invite:stream:"
const DISMISS = "invite:dismiss:"
const CANCEL = "stream-play:cancel"
const LEAVE = "stream-play:leave:"

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

export function surfaceInvitesFrom(invites: readonly Invite[], state: LaunchablesState): readonly SurfaceInvite[] {
  return invites.map(invite => ({
    id: invite.inviteId,
    personName: invite.personName,
    deviceLabel: invite.deviceLabel,
    gameTitle: invite.session.title ?? invite.session.gameId ?? "Game",
    ...(gameFacts(state, invite.session.gameId).coverArtUrl === undefined ? {} : { coverArtUrl: gameFacts(state, invite.session.gameId).coverArtUrl }),
    actions: [
      { id: STREAM + invite.inviteId, label: "Play on stream", enabled: true },
      { id: DISMISS + invite.inviteId, label: "Dismiss", enabled: true },
    ],
  }))
}

export function streamEndReason(ended: StreamPlayEnded): string {
  return ended.cause === "deviceUnreachable" ? `Lost ${ended.deviceLabel}`
    : `${ended.gameTitle ?? "The session"} ended on ${ended.deviceLabel}`
}

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
  return { _tag: "Ended", kicker: "Stream ended", reason: streamEndReason(ended),
    ...(gameFacts(state, ended.gameId).id === undefined ? {} : { gameId: gameFacts(state, ended.gameId).id }),
    ...(ended.gameTitle === undefined ? {} : { gameTitle: ended.gameTitle }),
  }
}

export function createSharedPlay(korrid: KorridClient, context: {
  readonly state: () => LaunchablesState
  readonly refreshSession: () => Promise<void>
}) {
  let snapshot: { readonly invites: readonly Invite[]; readonly status?: SurfaceStatus } = { invites: [] }
  const listeners = new Set<() => void>()
  let disposed = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let operation: { cancelled: boolean } | undefined
  let refreshId = 0
  const publish = (next: typeof snapshot) => {
    if (disposed || JSON.stringify(next) === JSON.stringify(snapshot)) return
    snapshot = next
    for (const listener of listeners) listener()
  }
  const refresh = async () => {
    const id = ++refreshId
    const result = await korrid.invitesList()
    if (disposed || id !== refreshId) return
    publish({ ...snapshot, invites: result._tag === "Ok" ? result.payload.invites : [] })
  }
  const busy = (kicker: string, cancellable = false) => publish({ invites: [], status: {
    _tag: "Busy", kicker, ...(cancellable ? { actions: [{ id: CANCEL, label: "Cancel", enabled: true }] } : {}),
  } })
  const problem = (message: string) => publish({ invites: [], status: {
    _tag: "Problem", kicker: "Stream unavailable", reason: message, canRetry: false,
  } })
  const play = async (invite: Invite) => {
    const active = { cancelled: false }
    operation = active
    busy(`Connecting to ${invite.deviceLabel}`, true)
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
    await context.refreshSession()
    publish({ invites: [] })
    await refresh()
  }
  const leave = async (launchId: string) => {
    operation = { cancelled: false }
    busy("Leaving stream")
    const outcome = await korrid.sessionLeave(launchId)
    operation = undefined
    if (disposed) return
    if (outcome._tag === "Err") { problem(outcome.payload.message); return }
    await context.refreshSession()
    publish({ invites: [] })
    await refresh()
  }
  return {
    getSnapshot: () => snapshot,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener) } },
    start: () => {
      disposed = false
      const poll = async () => {
        await refresh()
        if (!disposed) timer = setTimeout(() => void poll(), 1000)
      }
      void poll()
      return () => { disposed = true; clearTimeout(timer); ++refreshId }
    },
    act: (id: string): boolean => {
      if (id === CANCEL && operation !== undefined) { operation.cancelled = true; busy("Leaving stream"); return true }
      const paused = streamStatusFrom(context.state())
      if (paused?._tag === "Paused" && paused.actions[0]?.id === id && operation === undefined) {
        void leave(id.slice(LEAVE.length)); return true
      }
      const invite = snapshot.invites.find(invite => id === STREAM + invite.inviteId || id === DISMISS + invite.inviteId)
      if (invite === undefined) return false
      if (operation !== undefined || context.state()._tag !== "Ready") return true
      if (id === STREAM + invite.inviteId) void play(invite)
      else void korrid.invitesDismiss(invite.inviteId).then(async outcome => {
        if (outcome._tag === "Err") problem(outcome.payload.message)
        else await refresh()
      })
      return true
    },
    back: (): boolean => {
      if (operation !== undefined) { operation.cancelled = true; busy("Leaving stream"); return true }
      const paused = streamStatusFrom(context.state())
      if (paused?._tag !== "Paused") return false
      void leave(paused.actions[0]!.id.slice(LEAVE.length))
      return true
    },
    dismiss: (): boolean => {
      if (snapshot.status?._tag !== "Problem") return false
      publish({ invites: snapshot.invites }); return true
    },
  }
}
