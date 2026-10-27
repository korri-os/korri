/**
 * Shared play's model: one case at a time, and every transition between
 * them, driven with plain values through `sharedPlayStep`.
 */
import { expect, spyOn, test } from "bun:test"
import type { Invite, NetplayStatus } from "@contracts/generated/korrid"
import { createInMemoryKorridClient } from "../korrid/client"
import { LaunchablesState } from "../launchables/state"
import { initialPluginFacts } from "./plugin-repositories"
import {
  createSharedPlay,
  sharedPlayBusy,
  sharedPlayStatus,
  sharedPlayStep,
  type SharedPlay,
  type SharedPlayEvent,
} from "./shared-play"

const invite: Invite = { inviteId: "opaque", devicePublicKey: "far", deviceLabel: "RG35XX SP", personName: "Simon",
  session: { launchId: "exact", gameId: "turtles", title: "Turtles in Time", phase: "running" } }
const report = (phase: NetplayStatus["phase"], launchId = "mine"): NetplayStatus =>
  ({ launchId, deviceLabel: "RG35XX SP", gameTitle: "Turtles in Time", phase })
const run = (model: SharedPlay, ...events: SharedPlayEvent[]) => events.reduce(sharedPlayStep, model)
const start = { inviteId: "opaque", deviceLabel: "RG35XX SP", gameTitle: "Turtles in Time" }
const target = { sourceUrl: "https://official.example/catalog.json", pluginId: "@korri:snes9x2010", releaseVersion: "1" }
const question = { _tag: "Confirm" as const, kicker: "Install Snes9x 2010?", actions: [], declineActionId: "netplay:cancel" }
const reviewing: SharedPlay = { _tag: "Reviewing", op: 3, invite, title: "Snes9x 2010", target, facts: initialPluginFacts, question }

test("a read decides between Idle and Observing, and keeps the game only for the same launch", () => {
  expect(run({ _tag: "Unread" }, { _tag: "Read" })).toEqual({ _tag: "Idle" })
  expect(run({ _tag: "Unread", gameId: "entry" }, { _tag: "Read", netplay: report({ _tag: "Connecting" }) }))
    .toEqual({ _tag: "Observing", netplay: report({ _tag: "Connecting" }), gameId: "entry" })
  const observing: SharedPlay = { _tag: "Observing", netplay: report({ _tag: "Connecting" }), gameId: "entry" }
  expect(run(observing, { _tag: "Read", netplay: report({ _tag: "Playing" }) }))
    .toEqual({ _tag: "Observing", netplay: report({ _tag: "Playing" }), gameId: "entry" })
  expect(run(observing, { _tag: "Read", netplay: report({ _tag: "Playing" }, "other") }))
    .toEqual({ _tag: "Observing", netplay: report({ _tag: "Playing" }, "other") })
  expect(run(observing, { _tag: "Read" })).toEqual({ _tag: "Idle" })
})

test("a read during a start or a cancel keeps that case and takes korrid's launch", () => {
  const starting = run({ _tag: "Idle" }, { _tag: "NetplayRequested", op: 1, start })
  expect(starting).toEqual({ _tag: "NetplayStarting", op: 1, start })
  expect(run(starting, { _tag: "Read", netplay: report({ _tag: "Copying", done: 1, total: 2 }) }))
    .toEqual({ _tag: "NetplayStarting", op: 1, start, netplay: report({ _tag: "Copying", done: 1, total: 2 }) })
  const cancelling = run(starting, { _tag: "CancelPressed" })
  expect(cancelling).toEqual({ _tag: "Cancelling", kind: "netplay", pending: 1, sent: false })
  const named = run(cancelling, { _tag: "Read", netplay: report({ _tag: "Connecting" }) })
  expect(named).toEqual({ _tag: "Cancelling", kind: "netplay", pending: 1, launchId: "mine", sent: false })
  expect(run(named, { _tag: "CancelSent" }, { _tag: "Read", netplay: report({ _tag: "Connecting" }, "later") }))
    .toEqual({ _tag: "Cancelling", kind: "netplay", pending: 1, launchId: "mine", sent: true })
  // A read never ends a problem or a question.
  const problem: SharedPlay = { _tag: "Problem", status: { _tag: "Problem", kicker: "x", reason: "y", canRetry: false } }
  expect(run(problem, { _tag: "Read", netplay: report({ _tag: "Connecting" }) })).toBe(problem)
  expect(run(reviewing, { _tag: "Read" })).toBe(reviewing)
})

test("Cancel on korrid's own report: Copying and Connecting cancel its exact launch; the rest are not the host's", () => {
  for (const phase of [{ _tag: "Copying" as const, done: 1, total: 2 }, { _tag: "Connecting" as const }]) {
    expect(run({ _tag: "Observing", netplay: report(phase) }, { _tag: "CancelPressed" }))
      .toEqual({ _tag: "Cancelling", kind: "netplay", launchId: "mine", sent: false })
  }
  for (const phase of [{ _tag: "Playing" as const }, { _tag: "HolderLeft" as const }, { _tag: "Reconnecting" as const }]) {
    const observing: SharedPlay = { _tag: "Observing", netplay: report(phase) }
    expect(run(observing, { _tag: "CancelPressed" })).toBe(observing)
  }
  expect(run({ _tag: "Idle" }, { _tag: "CancelPressed" })).toEqual({ _tag: "Idle" })
})

test("Cancel on the host's own work: a stream start waits for its reply; an install stops, except while installing", () => {
  expect(run({ _tag: "Idle" }, { _tag: "StreamRequested", op: 2, deviceLabel: "RG35XX SP" }, { _tag: "CancelPressed" }))
    .toEqual({ _tag: "Cancelling", kind: "stream", pending: 2 })
  const finding = run({ _tag: "Idle" }, { _tag: "InstallRequested", op: 3, invite, title: "Snes9x 2010" })
  expect(finding).toEqual({ _tag: "Installing", op: 3, invite, title: "Snes9x 2010", step: "Finding" })
  expect(run(finding, { _tag: "CancelPressed" })).toEqual({ _tag: "Unread" })
  expect(run(finding, { _tag: "InstallStepped", op: 3, step: "Checking" }, { _tag: "CancelPressed" })).toEqual({ _tag: "Unread" })
  expect(run(reviewing, { _tag: "CancelPressed" })).toEqual({ _tag: "Unread" })
  const installing = run(reviewing, { _tag: "Approved" })
  expect(installing).toEqual({ _tag: "Installing", op: 3, invite, title: "Snes9x 2010", step: "Installing" })
  expect(run(installing, { _tag: "CancelPressed" })).toBe(installing)
})

test("the install walks Finding, Checking, Reviewing and Installing, and ignores a stale reply", () => {
  const finding = run({ _tag: "Idle" }, { _tag: "InstallRequested", op: 3, invite, title: "Snes9x 2010" })
  expect(run(finding, { _tag: "InstallStepped", op: 9, step: "Checking" })).toBe(finding)
  expect(run(finding, { _tag: "ReviewReady", op: 9, target, facts: initialPluginFacts, question })).toBe(finding)
  expect(run(finding, { _tag: "InstallStepped", op: 3, step: "Checking" }, { _tag: "ReviewReady", op: 3, target, facts: initialPluginFacts, question }))
    .toEqual(reviewing)
  // After the install, netplay starts from the Installing case.
  expect(run(reviewing, { _tag: "Approved" }, { _tag: "NetplayRequested", op: 4, start }))
    .toEqual({ _tag: "NetplayStarting", op: 4, start })
})

test("one piece of work at a time: a request while busy changes nothing", () => {
  const busy: SharedPlay[] = [
    { _tag: "StreamStarting", op: 1, deviceLabel: "RG35XX SP" },
    { _tag: "NetplayStarting", op: 1, start },
    { _tag: "Installing", op: 1, invite, title: "Snes9x 2010", step: "Checking" },
    reviewing,
    { _tag: "Cancelling", kind: "netplay", sent: false },
    { _tag: "Leaving", op: 1, kicker: "Leaving" },
  ]
  for (const model of busy) {
    expect(sharedPlayBusy(model)).toBe(true)
    expect(run(model, { _tag: "StreamRequested", op: 7, deviceLabel: "x" })).toBe(model)
    expect(run(model, { _tag: "NetplayRequested", op: 7, start })).toBe(model)
    expect(run(model, { _tag: "InstallRequested", op: 7, invite, title: "x" })).toBe(model)
    expect(run(model, { _tag: "LeaveRequested", op: 7, kicker: "x" })).toBe(model)
  }
  for (const model of [{ _tag: "Unread" }, { _tag: "Idle" }, { _tag: "Observing", netplay: report({ _tag: "HolderLeft" }) }] as SharedPlay[]) {
    expect(sharedPlayBusy(model)).toBe(false)
  }
})

test("the work ends in Unread, or in a Problem that Dismiss ends", () => {
  expect(run({ _tag: "NetplayStarting", op: 1, start }, { _tag: "Settled", gameId: "entry" })).toEqual({ _tag: "Unread", gameId: "entry" })
  const failed = run({ _tag: "NetplayStarting", op: 1, start },
    { _tag: "Failed", status: { _tag: "Problem", kicker: "Copy failed", reason: "r", canRetry: false } })
  expect(failed._tag).toBe("Problem")
  expect(run(failed, { _tag: "Dismissed" })).toEqual({ _tag: "Unread" })
  expect(run({ _tag: "Idle" }, { _tag: "Dismissed" })).toEqual({ _tag: "Idle" })
})

test("each case shows exactly one status", () => {
  expect(sharedPlayStatus({ _tag: "Unread" })).toBeUndefined()
  expect(sharedPlayStatus({ _tag: "Idle" })).toBeUndefined()
  expect(sharedPlayStatus({ _tag: "NetplayStarting", op: 1, start: { ...start, copyBytes: 4_000_000 } }))
    .toMatchObject({ _tag: "Busy", kicker: "Copying Turtles in Time from RG35XX SP", progress: { done: 0, total: 4_000_000 } })
  expect(sharedPlayStatus({ _tag: "NetplayStarting", op: 1, start, netplay: report({ _tag: "Playing" }) })).toBeUndefined()
  expect(sharedPlayStatus({ _tag: "Installing", op: 1, invite, title: "Snes9x 2010", step: "Checking" }))
    .toEqual({ _tag: "Busy", kicker: "Checking Snes9x 2010", actions: [{ id: "netplay:cancel", label: "Cancel", enabled: true }] })
  expect(sharedPlayStatus(reviewing)).toBe(question)
  expect(sharedPlayStatus({ _tag: "Cancelling", kind: "stream", pending: 1 })).toEqual({ _tag: "Busy", kicker: "Leaving stream" })
})

test("a cancel pressed before korrid reports goes to the launch korrid reports next, once", async () => {
  let netplay: NetplayStatus | undefined
  let release!: () => void
  const answered = new Promise<void>(resolve => { release = resolve })
  const base = createInMemoryKorridClient({ invites: [{ ...invite, netplay: { _tag: "Ready" } }], netplaySession: () => netplay })
  const client = { ...base, invitesNetplay: async (id: string) => { await answered; return base.invitesNetplay(id) } }
  const cancel = spyOn(client, "sessionCancel")
  const ready = () => LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [] } })
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => {} })
  const stop = shared.start()
  for (let i = 0; i < 10; ++i) await new Promise(resolve => setTimeout(resolve, 0))
  shared.act("invite:netplay:opaque")
  expect(shared.act("netplay:cancel")).toBe(true)
  expect(cancel).not.toHaveBeenCalled()
  netplay = report({ _tag: "Connecting" })
  await new Promise(resolve => setTimeout(resolve, 2100))
  expect(cancel).toHaveBeenCalledTimes(1)
  expect(cancel).toHaveBeenCalledWith({ expectedLaunchId: "mine" })
  release()
  stop()
})
