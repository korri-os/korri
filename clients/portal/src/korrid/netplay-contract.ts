/**
 * PROVISIONAL NETPLAY WIRE TYPES — REPLACE AT MERGE.
 *
 * The worker that owns `services/` adds these to korrid and exports them
 * through Typeshare into `contracts/generated/korrid.ts`. Until that branch
 * merges, this file is the one place the portal states them. At the merge:
 *
 *   1. Import `InviteNetplay`, the netplay `SessionStatus` field and the
 *      `app.invites.netplay` outcome from `@contracts/generated/korrid`.
 *   2. Delete this file and the two casts in `netplayRpcRequest` and
 *      `netplayOutcomeOf`; `callKorrid` then types the request itself.
 *
 * Sources: `/tmp/netplay-contract.md` item 6 (Invite.netplay, the RPC and
 * its outcome), and the coordinator's answer of 2026-10-26 for the parts the
 * contract left to `build` (option A): the `netplay` field of SessionStatus,
 * the reuse of `streamPlayEnded` for Ended, and the failure codes.
 */
import type {
  ActiveSession,
  Invite,
  RpcFailure,
  RpcRequest,
  SessionStatus,
} from "@contracts/generated/korrid"

/**
 * Whether this device can take part in an invite's session through netplay,
 * computed by this device's korrid. Absent when the holder's runner has no
 * `netplay` facts.
 */
export type InviteNetplay =
  /** The release and a matching core are both here. */
  | { _tag: "Ready" }
  /** The core is here; the release must be copied from the holder first. */
  | { _tag: "NeedsCopy"; bytes: number }
  /** No core here can play it in netplay; this plugin's newest release can. */
  | { _tag: "NeedsPlugin"; pluginId: string; title: string; bytes?: number }
  /** Netplay is not possible for a reason the person cannot fix here.
   * `reason` is a code (see `NetplayFailureCode`), never user copy. */
  | { _tag: "Unavailable"; reason: string }

export type NetplayInvite = Invite & { netplay?: InviteNetplay }

/** Where this device's netplay stands, as `app.session.status` reports it. */
export type NetplayPhase =
  | { _tag: "Copying"; done: number; total: number }
  | { _tag: "Connecting" }
  | { _tag: "Playing" }
  /** The holder left the session (froze it); the receiver waits. */
  | { _tag: "HolderLeft" }
  /** The connection dropped; korrid retries for about 10 s. */
  | { _tag: "Reconnecting" }

export interface NetplayObservation {
  launchId: string
  deviceLabel: string
  gameTitle?: string
  phase: NetplayPhase
}

export type NetplaySessionStatus = SessionStatus & { netplay?: NetplayObservation }

/** Same shape as `app.invites.streamPlay`'s outcome (contract item 6). */
export type InvitesNetplayOutcome =
  | { _tag: "Ok"; payload: ActiveSession }
  | { _tag: "Err"; payload: RpcFailure }

/** `RpcFailure.code` of `app.invites.netplay`, and `Unavailable.reason`. */
export const NetplayFailureCode = {
  NoNetplayRoute: "NoNetplayRoute",
  CopyFailed: "CopyFailed",
  /** The message carries the bytes the copy needs, as a decimal integer. */
  NotEnoughSpace: "NotEnoughSpace",
  CoreVersionMismatch: "CoreVersionMismatch",
} as const

export const inviteNetplayOf = (invite: Invite): InviteNetplay | undefined =>
  (invite as NetplayInvite).netplay

export const netplayObservationOf = (status: SessionStatus): NetplayObservation | undefined =>
  (status as NetplaySessionStatus).netplay

/** The request until `RpcRequest` carries it. */
export const netplayRpcRequest = (inviteId: string): RpcRequest =>
  ({ _tag: "app.invites.netplay", payload: { inviteId } }) as unknown as RpcRequest

/** The outcome of the response to `netplayRpcRequest`. */
export const netplayOutcomeOf = (response: unknown): InvitesNetplayOutcome =>
  (response as { outcome: InvitesNetplayOutcome }).outcome
