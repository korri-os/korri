import { afterEach, expect, spyOn, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import type { ActiveSession, Invite, NetplayStatus } from "@contracts/generated/korrid"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots = new Set<Root>()
afterEach(async () => {
  await act(async () => { for (const root of roots) root.unmount(); roots.clear() })
  document.body.innerHTML = ""
})
async function waitFor(condition: () => boolean) {
  for (let i = 0; i < 250; ++i) {
    if (condition()) return
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 10)) })
  }
  throw Error("condition did not become true")
}
const invite: Invite = { inviteId: "opaque", devicePublicKey: "far", deviceLabel: "Odin 2 Portal", personName: "Simon",
  session: { launchId: "exact", gameId: "turtles", title: "Turtles in Time", phase: "running" } }
async function mount(client: KorridClient) {
  await client.finishOnboarding()
  let model!: SurfaceModel
  let host!: SurfaceHost
  let backs = 0
  let off: (() => void) | undefined
  const bus = createInputBus()
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.add(root)
  await act(async () => root.render(<SurfaceRoot bus={bus} korrid={client} surface={{
    id: "shared-play-test", title: "Shared play", presentations: ["catalog"],
    render: props => { model = props.model; host = props.host; off ??= host.input.on("back", () => ++backs); return null },
  }} />))
  await waitFor(() => model !== undefined && model.catalog._tag !== "Loading")
  return { model: () => model, host: () => host, bus, backs: () => backs,
    async unmount() { off?.(); await act(async () => root.unmount()); roots.delete(root); container.remove() } }
}

test("the portal maps invite actions to RPCs and intercepts Back while Paused, even after reload", async () => {
  const far: ActiveSession = { ...invite.session, phase: "frozen" }
  const client = createInMemoryKorridClient({ games: [], invites: [invite], streamSession: () => far })
  const play = spyOn(client, "invitesStreamPlay")
  const leave = spyOn(client, "sessionLeave")
  const first = await mount(client)
  await waitFor(() => first.model().invites?.length === 1)
  expect(first.model().invites?.[0]?.personName).toBe("Simon")
  await act(async () => first.host().runAction(first.model().invites![0]!.actions[0]!.id))
  await waitFor(() => first.model().status._tag === "Paused")
  expect(play).toHaveBeenCalledWith("opaque")
  await first.unmount()
  const reloaded = await mount(client)
  expect(reloaded.model().status).toMatchObject({ _tag: "Paused", actions: [{ label: "Leave" }] })
  await act(async () => reloaded.bus.emit({ type: "back" }))
  await waitFor(() => reloaded.model().status._tag === "Browsing")
  expect(leave).toHaveBeenCalledWith("exact")
  expect(reloaded.backs()).toBe(0)
  expect(far.phase).toBe("frozen")
})

test("a frozen remote starter session keeps its existing behaviour and receives Back", async () => {
  const client = createInMemoryKorridClient({ games: [], activeSession: { ...invite.session, host: invite.deviceLabel, phase: "frozen" } })
  const view = await mount(client)
  expect(view.model().status._tag).toBe("Browsing")
  await act(async () => view.bus.emit({ type: "back" }))
  expect(view.backs()).toBe(1)
})

test("Ended is observed, names the game and holder, and stays hidden after dismiss and reload", async () => {
  let far: ActiveSession | undefined = { ...invite.session }
  const client = createInMemoryKorridClient({ games: [], activeSession: { ...far, host: invite.deviceLabel }, streamPlayLaunchId: "exact", streamSession: () => far })
  const view = await mount(client)
  far = undefined
  await waitFor(() => view.model().status._tag === "Ended")
  expect(view.model().status).toMatchObject({ reason: "Turtles in Time ended on Odin 2 Portal" })
  await act(async () => view.host().dismiss())
  expect(view.model().status._tag).toBe("Browsing")
  await act(async () => view.host().reload())
  await waitFor(() => view.model().catalog._tag !== "Loading")
  expect(view.model().status._tag).toBe("Browsing")
})

test("the portal publishes the netplay route, shows korrid's copy progress, and Back cancels it", async () => {
  let report: NetplayStatus | undefined
  let release!: () => void
  const answered = new Promise<void>(resolve => { release = resolve })
  const base = createInMemoryKorridClient({ games: [], netplaySession: () => report,
    invites: [{ ...invite, netplay: { _tag: "NeedsCopy", bytes: 4_000_000 } }] })
  const client = { ...base, invitesNetplay: async (id: string) => { await answered; return base.invitesNetplay(id) } }
  const cancel = spyOn(client, "sessionCancel")
  const view = await mount(client)
  await waitFor(() => view.model().invites?.length === 1)
  expect(view.model().invites![0]!.actions.map(action => action.label)).toEqual(["Play on stream", "Copy and play here (4 MB)", "Dismiss"])
  await act(async () => view.host().runAction(view.model().invites![0]!.actions[1]!.id))
  report = { launchId: "mine", deviceLabel: invite.deviceLabel, gameTitle: "Turtles in Time", phase: { _tag: "Copying", done: 1_000_000, total: 4_000_000 } }
  await waitFor(() => view.model().status._tag === "Busy" && "progress" in view.model().status
    && (view.model().status as { progress?: { done: number } }).progress?.done === 1_000_000)
  expect(view.model().status).toMatchObject({ kicker: "Copying Turtles in Time from Odin 2 Portal", detail: "25% of 4 MB" })
  expect(view.model().invites).toEqual([])
  await act(async () => view.bus.emit({ type: "back" }))
  expect(cancel).toHaveBeenCalledWith({ expectedLaunchId: "mine" })
  expect(view.backs()).toBe(0)
  report = undefined
  release()
  await view.unmount()
})
