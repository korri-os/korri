/**
 * Netplay on the receiving device: the route an invite offers, the states
 * korrid reports while it copies, connects, waits and reconnects, every
 * problem, and the plugin install that comes first when the core is missing.
 */
import { expect, spyOn, test } from "bun:test"
import { SessionFreezerState, type Invite } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient, type InMemoryPluginHostConfig } from "../korrid/client"
import type { InviteNetplay, NetplayInvite, NetplayObservation } from "../korrid/netplay-contract"
import { LaunchablesState } from "../launchables/state"
import { initialPluginFacts } from "./plugin-repositories"
import {
  createSharedPlay,
  netplayProblem,
  netplayStatusFrom,
  newestRelease,
  surfaceInvitesFrom,
} from "./shared-play"

const base: Invite = { inviteId: "opaque", devicePublicKey: "far", deviceLabel: "RG35XX SP", personName: "Simon",
  session: { launchId: "exact", gameId: "turtles", title: "Turtles in Time", phase: "running" } }
const withRoute = (netplay?: InviteNetplay): NetplayInvite => netplay === undefined ? base : { ...base, netplay }
const ready = () => LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [] } })
const settle = async () => { for (let i = 0; i < 10; ++i) await new Promise(resolve => setTimeout(resolve, 0)) }
const gate = () => {
  let open!: () => void
  const closed = new Promise<void>(resolve => { open = resolve })
  return { closed, open: () => open() }
}
const labels = (invite: NetplayInvite) => surfaceInvitesFrom([invite], ready())[0]!.actions.map(action => [action.id, action.label])

test("each netplay route is one action between stream play and Dismiss, with its own words", () => {
  expect(labels(withRoute({ _tag: "Ready" }))).toEqual([
    ["invite:stream:opaque", "Play on stream"], ["invite:netplay:opaque", "Play here"], ["invite:dismiss:opaque", "Dismiss"],
  ])
  expect(labels(withRoute({ _tag: "NeedsCopy", bytes: 4_194_304 }))[1]).toEqual(["invite:netplay:opaque", "Copy and play here (4.2 MB)"])
  expect(labels(withRoute({ _tag: "NeedsCopy", bytes: 612_000_000 }))[1]).toEqual(["invite:netplay:opaque", "Copy and play here (612 MB)"])
  expect(labels(withRoute({ _tag: "NeedsPlugin", pluginId: "@korri:snes9x2010", title: "Snes9x 2010" }))[1])
    .toEqual(["invite:netplay:opaque", "Install Snes9x 2010 and play here"])
})

test("no netplay action when korrid offers none or the reason can't be fixed here", () => {
  const streamOnly = [["invite:stream:opaque", "Play on stream"], ["invite:dismiss:opaque", "Dismiss"]]
  expect(labels(withRoute())).toEqual(streamOnly)
  for (const reason of ["CoreVersionMismatch", "NotEnoughSpace", "NoNetplayRoute"]) {
    expect(labels(withRoute({ _tag: "Unavailable", reason }))).toEqual(streamOnly)
  }
})

const observed = (phase: NetplayObservation["phase"]): NetplayObservation =>
  ({ launchId: "mine", deviceLabel: "RG35XX SP", gameTitle: "Turtles in Time", phase })

test("korrid's report becomes Busy with a bar and Cancel, Paused with Leave, or Reconnecting with nothing to press", () => {
  expect(netplayStatusFrom(observed({ _tag: "Copying", done: 1_048_576, total: 4_194_304 }), "entry")).toEqual({
    _tag: "Busy", kicker: "Copying Turtles in Time from RG35XX SP", detail: "25% of 4.2 MB",
    progress: { done: 1_048_576, total: 4_194_304 }, actions: [{ id: "netplay:cancel", label: "Cancel", enabled: true }], gameId: "entry",
  })
  expect(netplayStatusFrom(observed({ _tag: "Connecting" }))).toEqual({
    _tag: "Busy", kicker: "Connecting to RG35XX SP", actions: [{ id: "netplay:cancel", label: "Cancel", enabled: true }],
  })
  expect(netplayStatusFrom(observed({ _tag: "HolderLeft" }))).toEqual({
    _tag: "Paused", kicker: "Paused on RG35XX SP", actions: [{ id: "netplay:leave:mine", label: "Leave", enabled: true }],
  })
  expect(netplayStatusFrom(observed({ _tag: "Reconnecting" }))).toEqual({ _tag: "Busy", kicker: "Reconnecting to RG35XX SP" })
  expect(netplayStatusFrom(observed({ _tag: "Playing" }))).toBeUndefined()
})

test("every problem names the game, the device and what is left; codes never reach the person", () => {
  const facts = { gameTitle: "Turtles in Time", deviceLabel: "RG35XX SP", core: "Snes9x 2010" }
  expect(netplayProblem({ code: "NoNetplayRoute", message: "x" }, facts)).toEqual({ _tag: "Problem", kicker: "Netplay unavailable",
    reason: "Turtles in Time can't play here with RG35XX SP right now. Play on stream still works.", canRetry: false })
  expect(netplayProblem({ code: "CoreVersionMismatch", message: "x" }, facts)).toMatchObject({ kicker: "Netplay unavailable",
    reason: "RG35XX SP runs a different version of Snes9x 2010. Play on stream still works." })
  expect(netplayProblem({ code: "CopyFailed", message: "x" }, facts)).toMatchObject({ kicker: "Copy failed",
    reason: "Turtles in Time did not copy from RG35XX SP. Nothing was kept." })
  expect(netplayProblem({ code: "NotEnoughSpace", message: "4700000000" }, facts)).toMatchObject({ kicker: "Not enough space",
    reason: "Turtles in Time needs 4.7 GB free on this device." })
  expect(netplayProblem({ code: "NotEnoughSpace", message: "no room" }, facts)).toMatchObject({
    reason: "Turtles in Time needs more free space on this device." })
  expect(netplayProblem({ code: "BrainUnreachable", message: "korrid did not answer" }, facts)).toMatchObject({
    kicker: "Netplay unavailable", reason: "korrid did not answer" })
})

test("Play here: Connecting until korrid reports, then its report; Playing gives the screen back", async () => {
  let report: NetplayObservation | undefined
  const answer = gate()
  const base = createInMemoryKorridClient({ invites: [withRoute({ _tag: "Ready" })], netplaySession: () => report })
  const client = { ...base, invitesNetplay: async (id: string) => { await answer.closed; return base.invitesNetplay(id) } }
  const call = spyOn(client, "invitesNetplay")
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => {} })
  const stop = shared.start()
  await settle()
  expect(shared.act("invite:netplay:opaque")).toBe(true)
  expect(call).toHaveBeenCalledWith("opaque")
  expect(shared.getSnapshot().status).toMatchObject({ _tag: "Busy", kicker: "Connecting to RG35XX SP", actions: [{ id: "netplay:cancel" }] })
  report = observed({ _tag: "Connecting" })
  await new Promise(resolve => setTimeout(resolve, 1100))
  expect(shared.getSnapshot().status).toMatchObject({ _tag: "Busy", kicker: "Connecting to RG35XX SP" })
  report = observed({ _tag: "Playing" })
  answer.open()
  await settle()
  expect(shared.getSnapshot().status).toBeUndefined()
  stop()
})

test("Copy and play here: the bar starts at once, follows korrid's bytes, and Cancel cancels the exact launch", async () => {
  let report: NetplayObservation | undefined
  const answer = gate()
  const base = createInMemoryKorridClient({ invites: [withRoute({ _tag: "NeedsCopy", bytes: 4_000_000 })], netplaySession: () => report })
  const client = { ...base, invitesNetplay: async () => {
    await answer.closed
    return { _tag: "Err" as const, payload: { code: "Cancelled", message: "The copy was cancelled" } }
  } }
  const cancel = spyOn(client, "sessionCancel")
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => {} })
  const stop = shared.start()
  await settle()
  shared.act("invite:netplay:opaque")
  expect(shared.getSnapshot().status).toMatchObject({ _tag: "Busy", kicker: "Copying Turtles in Time from RG35XX SP",
    progress: { done: 0, total: 4_000_000 }, detail: "0% of 4 MB" })
  report = observed({ _tag: "Copying", done: 3_000_000, total: 4_000_000 })
  await new Promise(resolve => setTimeout(resolve, 1100))
  expect(shared.getSnapshot().status).toMatchObject({ progress: { done: 3_000_000, total: 4_000_000 }, detail: "75% of 4 MB" })
  expect(shared.act("netplay:cancel")).toBe(true)
  expect(cancel).toHaveBeenCalledWith({ expectedLaunchId: "mine" })
  expect(shared.getSnapshot().status).toEqual({ _tag: "Busy", kicker: "Cancelling" })
  report = undefined
  answer.open()
  await settle()
  // A cancelled copy is no problem to report: the invite is back.
  expect(shared.getSnapshot().status).toBeUndefined()
  expect(shared.getSnapshot().invites.map(invite => invite.inviteId)).toEqual(["opaque"])
  stop()
})

test("a refused netplay is a Problem in plain words, and dismissing it brings the invite back", async () => {
  const base = createInMemoryKorridClient({ invites: [withRoute({ _tag: "NeedsCopy", bytes: 4_700_000_000 })] })
  const client = { ...base, invitesNetplay: async () => ({ _tag: "Err" as const, payload: { code: "NotEnoughSpace", message: "4700000000" } }) }
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => {} })
  const stop = shared.start()
  await settle()
  shared.act("invite:netplay:opaque")
  await settle()
  expect(shared.getSnapshot().status).toEqual({ _tag: "Problem", kicker: "Not enough space",
    reason: "Turtles in Time needs 4.7 GB free on this device.", canRetry: false })
  expect(shared.dismiss()).toBe(true)
  expect(shared.getSnapshot().status).toBeUndefined()
  stop()
})

test("the holder left: Paused with Leave, found again after a reload; Leave and Back leave the exact launch", async () => {
  let report: NetplayObservation | undefined = observed({ _tag: "HolderLeft" })
  const client = createInMemoryKorridClient({ netplaySession: () => report })
  // korrid ends this device's copy and keeps its seat.
  const leave = spyOn(client, "sessionLeave").mockImplementation(async launchId => ({ _tag: "Ok", payload: {
    launchId, state: SessionFreezerState.Frozen, changed: true } }))
  // A portal that starts while netplay waits finds it at its first read.
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => { report = undefined } })
  const stop = shared.start()
  await settle()
  expect(shared.getSnapshot().status).toEqual({ _tag: "Paused", kicker: "Paused on RG35XX SP",
    actions: [{ id: "netplay:leave:mine", label: "Leave", enabled: true }] })
  expect(shared.act("netplay:leave:mine")).toBe(true)
  await settle()
  expect(leave).toHaveBeenCalledWith("mine")
  expect(shared.getSnapshot().status).toBeUndefined()
  stop()

  report = observed({ _tag: "HolderLeft" })
  const again = createSharedPlay(client, { state: ready, refreshSession: async () => { report = undefined } })
  const stopAgain = again.start()
  await settle()
  expect(again.back()).toBe(true)
  await settle()
  expect(leave).toHaveBeenCalledTimes(2)
  stopAgain()
})

test("Reconnecting holds Back: korrid retries for about 10 s and nothing is pressed", async () => {
  const client = createInMemoryKorridClient({ netplaySession: () => observed({ _tag: "Reconnecting" }) })
  const leave = spyOn(client, "sessionLeave")
  const cancel = spyOn(client, "sessionCancel")
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => {} })
  const stop = shared.start()
  await settle()
  expect(shared.getSnapshot().status).toEqual({ _tag: "Busy", kicker: "Reconnecting to RG35XX SP" })
  expect(shared.back()).toBe(true)
  expect(leave).not.toHaveBeenCalled()
  expect(cancel).not.toHaveBeenCalled()
  stop()
})

const SNES9X = "@korri:snes9x2010"
const OFFICIAL = "https://official.example/catalog.json"
const COMMUNITY = "https://community.example/catalog.json"
const pluginHost: InMemoryPluginHostConfig = {
  official: OFFICIAL,
  userAdded: [COMMUNITY],
  repositories: [
    { url: OFFICIAL, entries: [
      { pluginId: SNES9X, title: "Snes9x 2010", releaseVersion: "2026.9.30", publisherTrusted: true },
      { pluginId: SNES9X, title: "Snes9x 2010", releaseVersion: "2026.10.2", publisherTrusted: true },
    ] },
    { url: COMMUNITY, entries: [{ pluginId: SNES9X, title: "Snes9x 2010", releaseVersion: "2027.1.1", publisherTrusted: false }] },
  ],
  installs: [],
  inspections: { [SNES9X]: { id: SNES9X, title: "Snes9x 2010", warning: "", ports: { allowedTCPPorts: [55435], allowedUDPPorts: [] }, nativeUnits: [], brings: [] } },
}
const needsPlugin: InviteNetplay = { _tag: "NeedsPlugin", pluginId: SNES9X, title: "Snes9x 2010" }

test("the newest trusted release of the plugin is the one installed", () => {
  const facts = { ...initialPluginFacts, catalogs: {
    [OFFICIAL]: { _tag: "Ready" as const, entries: pluginHost.repositories[0]!.entries },
    [COMMUNITY]: { _tag: "Ready" as const, entries: pluginHost.repositories[1]!.entries },
  } }
  expect(newestRelease(facts, SNES9X)).toEqual({ sourceUrl: OFFICIAL, pluginId: SNES9X, releaseVersion: "2026.10.2" })
  expect(newestRelease(facts, "@korri:absent")).toBeUndefined()
})

async function installFlow(after: InviteNetplay) {
  let route: InviteNetplay = needsPlugin
  const client = createInMemoryKorridClient({ pluginHost, invites: () => [withRoute(route)] })
  const install = spyOn(client, "installPlugin")
  const enable = spyOn(client, "setPluginEnabled")
  const netplay = spyOn(client, "invitesNetplay")
  let changed = 0
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => {}, pluginsChanged: () => {
    ++changed
    route = after
  } })
  const stop = shared.start()
  await settle()
  expect(shared.act("invite:netplay:opaque")).toBe(true)
  expect(shared.getSnapshot().status).toMatchObject({ _tag: "Busy", kicker: "Finding Snes9x 2010", actions: [{ id: "netplay:cancel" }] })
  await settle()
  expect(install).not.toHaveBeenCalled()
  return { client, shared, stop, install, enable, netplay, changed: () => changed }
}

test("Install and play here: Settings' review first, then the install, then netplay", async () => {
  const flow = await installFlow({ _tag: "Ready" })
  expect(flow.shared.getSnapshot().status).toEqual({
    _tag: "Confirm", kicker: "Install Snes9x 2010?", detail: "Opens network ports: TCP 55435.",
    actions: [{ id: "netplay:install", label: "Install and play here", enabled: true }, { id: "netplay:cancel", label: "Cancel", enabled: true }],
    // A stray press must never approve the install: the cursor starts on Cancel.
    declineActionId: "netplay:cancel",
  })
  const shown: unknown[] = []
  flow.shared.subscribe(() => shown.push(flow.shared.getSnapshot().status))
  expect(flow.shared.act("netplay:install")).toBe(true)
  await Promise.resolve()
  expect(flow.shared.getSnapshot().status).toEqual({ _tag: "Busy", kicker: "Installing Snes9x 2010" })
  // Installing has nothing to cancel.
  expect(flow.shared.back()).toBe(true)
  await settle()
  expect(shown[0]).toEqual({ _tag: "Busy", kicker: "Installing Snes9x 2010" })
  expect(flow.install).toHaveBeenCalledWith(OFFICIAL, SNES9X, "2026.10.2", expect.any(String))
  expect(flow.enable).toHaveBeenCalledWith(SNES9X, true)
  expect(flow.changed()).toBe(1)
  expect(flow.netplay).toHaveBeenCalledWith("opaque")
  flow.stop()
})

test("a core whose version differs from the holder's is a Problem after the install; stream play stays", async () => {
  const flow = await installFlow({ _tag: "Unavailable", reason: "CoreVersionMismatch" })
  flow.shared.act("netplay:install")
  await settle()
  expect(flow.netplay).not.toHaveBeenCalled()
  expect(flow.shared.getSnapshot().status).toEqual({ _tag: "Problem", kicker: "Netplay unavailable",
    reason: "RG35XX SP runs a different version of Snes9x 2010. Play on stream still works.", canRetry: false })
  flow.shared.dismiss()
  expect(surfaceInvitesFrom(flow.shared.getSnapshot().invites, ready())[0]!.actions.map(action => action.label)).toEqual(["Play on stream", "Dismiss"])
  flow.stop()
})

test("Cancel at the review installs nothing and brings the invite back", async () => {
  const flow = await installFlow({ _tag: "Ready" })
  expect(flow.shared.back()).toBe(true)
  await settle()
  expect(flow.shared.getSnapshot().status).toBeUndefined()
  expect(flow.install).not.toHaveBeenCalled()
  expect(flow.netplay).not.toHaveBeenCalled()
  flow.stop()
})

test("no repository offers the plugin: a Problem that says so", async () => {
  const client = createInMemoryKorridClient({ pluginHost: { ...pluginHost, repositories: [] },
    invites: [withRoute(needsPlugin)] })
  const shared = createSharedPlay(client, { state: ready, refreshSession: async () => {} })
  const stop = shared.start()
  await settle()
  shared.act("invite:netplay:opaque")
  await settle()
  expect(shared.getSnapshot().status).toEqual({ _tag: "Problem", kicker: "Install failed",
    reason: "No plugin repository on this device offers Snes9x 2010.", canRetry: false })
  stop()
})

test("the HTTP client asks its own korrid for app.invites.netplay with the opaque invite", async () => {
  const requests: unknown[] = []
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
    const body = await request.json() as { _tag: string }
    requests.push(body)
    return Response.json({ _tag: body._tag, outcome: { _tag: "Ok", payload: { launchId: "mine" } } })
  } })
  try {
    const client = createHttpKorridClient(`http://127.0.0.1:${server.port}`, "test")
    expect(await client.invitesNetplay("opaque")).toEqual({ _tag: "Ok", payload: { launchId: "mine" } })
    expect(requests).toEqual([{ _tag: "app.invites.netplay", payload: { inviteId: "opaque" } }])
  } finally { await server.stop(true) }
})
