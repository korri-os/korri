import { expect, test } from "bun:test"
import type { Invite, ActiveSession } from "@contracts/generated/korrid"
import { StreamPlayEndCause } from "@contracts/generated/korrid"
import { createInMemoryKorridClient } from "../korrid/client"
import { LaunchablesState } from "../launchables/state"
import { createSharedPlay, streamStatusFrom, surfaceInvitesFrom } from "./shared-play"

const invite: Invite = { inviteId: "opaque", devicePublicKey: "far", deviceLabel: "Odin 2 Portal", personName: "Simon",
  session: { launchId: "exact", gameId: "turtles", title: "Turtles in Time", phase: "running" } }
const ready = () => LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [] } })
const settle = async () => { for (let i = 0; i < 10; ++i) await new Promise(resolve => setTimeout(resolve, 0)) }

test("invites carry opaque actions, the person and cover art of the matching game", () => {
  const games = [{ id: "turtles", title: "Turtles in Time", system: "SNES", coverArtUrl: "http://local/cover" }]
  const state = LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [] } }, undefined, { _tag: "Ok", payload: { games } })
  expect(surfaceInvitesFrom([invite], state)).toEqual([{ id: "opaque", personName: "Simon", gameTitle: "Turtles in Time", deviceLabel: "Odin 2 Portal", coverArtUrl: "http://local/cover",
    actions: [{ id: "invite:stream:opaque", label: "Play on stream", enabled: true }, { id: "invite:dismiss:opaque", label: "Dismiss", enabled: true }] }])
})

test("a selected frozen stream is Paused with exactly Leave, but a frozen starter session is not", () => {
  const status = { _tag: "Ok" as const, payload: { active: { ...invite.session, host: invite.deviceLabel, phase: "frozen" }, streamPlayLaunchId: "exact" } }
  const state = LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [] } }, status)
  expect(streamStatusFrom(state)).toEqual({ _tag: "Paused", kicker: "Paused on Odin 2 Portal", actions: [{ id: "stream-play:leave:exact", label: "Leave", enabled: true }] })
  const starter = LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [] } }, { _tag: "Ok", payload: { active: status.payload.active } })
  expect(streamStatusFrom(starter)).toBeUndefined()
})

test("Ended copy uses the holder and cause, never a daemon-supplied copy string", () => {
  const state = LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [] } }, { _tag: "Ok", payload: { streamPlayEnded: {
    launchId: "exact", deviceLabel: "Odin 2 Portal", gameTitle: "Turtles in Time", cause: StreamPlayEndCause.SessionEnded,
  } } })
  expect(streamStatusFrom(state)).toMatchObject({ _tag: "Ended", reason: "Turtles in Time ended on Odin 2 Portal" })
  if (state._tag !== "Ready" || state.streamPlayEnded === undefined) throw Error("not ready")
  expect(streamStatusFrom({ ...state, streamPlayEnded: { ...state.streamPlayEnded, cause: StreamPlayEndCause.DeviceUnreachable } })).toMatchObject({ reason: "Lost Odin 2 Portal" })
})

test("published invite actions start a stream and Leave stops only its viewer", async () => {
  let far: ActiveSession | undefined = { ...invite.session, phase: "frozen" }
  const client = createInMemoryKorridClient({ invites: [invite], streamSession: () => far })
  let state = ready()
  const shared = createSharedPlay(client, { state: () => state, refreshSession: async () => {
    state = LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [] } }, await client.sessionStatus())
  } })
  const stop = shared.start()
  await settle()
  expect(shared.act("invite:stream:opaque")).toBe(true)
  expect(shared.getSnapshot().status?._tag).toBe("Busy")
  await settle()
  expect(streamStatusFrom(state)?._tag).toBe("Paused")
  expect(shared.back()).toBe(true)
  await settle()
  expect((await client.sessionStatus())).toEqual({ _tag: "Ok", payload: {} })
  expect(far?.phase).toBe("frozen")
  stop()
})

test("a dismissed launch stays dismissed but its next launch can invite again", async () => {
  let current: readonly Invite[] = [invite]
  const client = createInMemoryKorridClient({ invites: () => current })
  expect(await client.invitesDismiss(invite.inviteId)).toEqual({ _tag: "Ok", payload: {} })
  expect(await client.invitesList()).toEqual({ _tag: "Ok", payload: { invites: [] } })
  current = [{ ...invite, inviteId: "next", session: { ...invite.session, launchId: "next" } }]
  expect(await client.invitesList()).toMatchObject({ _tag: "Ok", payload: { invites: [{ inviteId: "next" }] } })
})

test("Cancel waits for the exact viewer request then Leaves it without ending the holder", async () => {
  const base = createInMemoryKorridClient({ invites: [invite] })
  let release!: () => void
  const gate = new Promise<void>(resolve => { release = resolve })
  const client = { ...base, invitesStreamPlay: async (id: string) => {
    await gate
    return base.invitesStreamPlay(id)
  } }
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => {} })
  const stop = shared.start()
  await settle()
  shared.act("invite:stream:opaque")
  expect(shared.getSnapshot().status).toMatchObject({ _tag: "Busy", actions: [{ label: "Cancel" }] })
  shared.act("stream-play:cancel")
  release()
  await settle()
  expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  expect((await client.invitesList())).toMatchObject({ _tag: "Ok", payload: { invites: [{ inviteId: "opaque" }] } })
  stop()
})
