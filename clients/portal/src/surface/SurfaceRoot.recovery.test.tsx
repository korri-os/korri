import { afterEach, expect, spyOn, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { FocusOwnership, InitialHandoff, type Game } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { ShiftSurface } from "@korri/shift"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"
import { runnerRoutes } from "./fixtures/runner-routes"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots = new Set<Root>()
afterEach(async () => {
  await act(async () => { for (const root of roots) root.unmount(); roots.clear() })
  document.body.innerHTML = ""
})
const gate = () => {
  let resolve!: () => void
  const promise = new Promise<void>(done => { resolve = done })
  return { promise, resolve }
}
const invoke = async (fn: () => void) => { await act(async () => fn()) }
const waitFor = async (fn: () => boolean) => {
  for (let i = 0; i < 200; i++) {
    if (fn()) return
    await act(async () => { await new Promise(done => setTimeout(done, 10)) })
  }
  throw new Error("condition did not become true")
}
const game = (selected: boolean): Game => ({
  id: "wl4", title: "Wario Land 4", supportsRunnerSelection: selected,
  source: { label: "This device", isLocal: true },
})
const seed = (selected: boolean) => ({
  games: [game(selected)],
  gameRoutes: [{ ...runnerRoutes, gameRunner: undefined, systemRunners: {}, routes: runnerRoutes.routes.slice(0, 1) }],
})
async function mount(korrid: KorridClient, options: { presentation?: "pico" | "shift"; initialGameId?: string } = {}) {
  let model!: SurfaceModel
  let host!: SurfaceHost
  const bus = createInputBus()
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.add(root)
  await act(async () => root.render(<SurfaceRoot bus={bus} korrid={korrid} surface={{
    id: "recovery", title: "Recovery", presentations: ["catalog"],
    render: props => {
      model = props.model; host = props.host
      return options.presentation === "shift" ? <ShiftSurface {...props} />
        : <PicoSurface {...props} initialView={{ _tag: "Detail", gameId: options.initialGameId ?? "game:wl4" }} />
    },
  }} />))
  await waitFor(() => model.catalog._tag !== "Loading")
  return {
    container, bus, model: () => model, host: () => host,
    async launch() {
      await invoke(() => host.launchGame("game:wl4"))
      await waitFor(() => model.status._tag === "Busy")
    },
    async unmount() { await invoke(() => root.unmount()); roots.delete(root); container.remove() },
    currentId() { return model.catalog._tag === "Ready" ? model.catalog.games.find(item => item.resumable)?.id.replace("now-playing:", "") : undefined },
  }
}
const unreachable = { _tag: "Err" as const, payload: { code: "BrainUnreachable", message: "connection lost" } }

for (const selected of [false, true]) {
  const path = selected ? "selected runner" : "configured command"
  test(`${path}: fresh mount recovers exact initial waiting, not a second launch`, async () => {
    const korrid = createInMemoryKorridClient({ ...seed(selected), sessionFocusDelayMs: 60_000 })
    const reserves = spyOn(korrid, "sessionReserve")
    const starts = spyOn(korrid, "sessionStart")
    const cancels = spyOn(korrid, "sessionCancel")
    const first = await mount(korrid)
    await first.launch()
    const id = first.currentId()!
    await first.unmount()
    const recovered = await mount(korrid)
    expect(recovered.model().status._tag).toBe("Busy")
    expect(recovered.container.querySelector(".pico-launch-stage")).not.toBeNull()
    await invoke(() => recovered.host().runAction("cancel-launch"))
    await waitFor(() => recovered.model().status._tag === "Browsing")
    expect(cancels).toHaveBeenCalledWith({ expectedLaunchId: id })
    expect(reserves).toHaveBeenCalledTimes(1)
    expect(starts).toHaveBeenCalledTimes(1)
    reserves.mockRestore(); starts.mockRestore(); cancels.mockRestore()
  })
  test(`${path}: fresh mount retains a reserved identity before any live unit`, async () => {
    const reserve = gate()
    const korrid = createInMemoryKorridClient({ ...seed(selected), sessionReserveGate: reserve.promise })
    const starts = spyOn(korrid, "sessionStart")
    const first = await mount(korrid)
    await first.launch()
    await first.unmount()
    const recovered = await mount(korrid)
    expect(recovered.model().status._tag).toBe("Busy")
    await invoke(() => recovered.host().runAction("cancel-launch"))
    await waitFor(() => recovered.model().status._tag === "Browsing")
    await invoke(reserve.resolve)
    expect(starts).not.toHaveBeenCalled()
    expect(await korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
    starts.mockRestore()
  })
  test(`${path}: failed undispatched Cancel permits an explicit exact retry after reconnect`, async () => {
    const implementation = createInMemoryKorridClient({ ...seed(selected), sessionFocusDelayMs: 60_000 })
    let attempts = 0
    let outage = false
    const client: KorridClient = { ...implementation, async sessionCancel(request) {
      attempts++
      if (attempts === 1) { outage = true; return unreachable }
      return implementation.sessionCancel(request)
    }, async sessionStatus(timeout) { return outage ? unreachable : implementation.sessionStatus(timeout) } }
    const view = await mount(client)
    await view.launch()
    const id = view.currentId()!
    await invoke(() => view.bus.emit({ type: "back" }))
    expect(await client.sessionStatus()).toEqual(unreachable)
    outage = false
    await invoke(() => view.host().reload())
    await waitFor(() => { const status = view.model().status; return status._tag === "Busy" && !!status.actions?.some(action => action.enabled) })
    expect((await implementation.sessionStatus())).toMatchObject({ _tag: "Ok", payload: { active: { launchId: id } } })
    await invoke(() => view.host().runAction("cancel-launch"))
    await waitFor(() => view.model().status._tag === "Browsing")
    expect(attempts).toBe(2)
  })
}

test("the public status outage knob actually returns BrainUnreachable", async () => {
  const client = createInMemoryKorridClient({ sessionStatusUnavailable: () => true })
  expect(await client.sessionStatus()).toMatchObject({ _tag: "Err", payload: { code: "BrainUnreachable" } })
})

for (const initialHandoff of [InitialHandoff.Observed, InitialHandoff.Recovered, undefined]) {
  test(`fresh mount does not infer startup from current focus: ${initialHandoff}`, async () => {
    const korrid = createInMemoryKorridClient({ ...seed(false), activeSession: {
      gameId: "wl4", launchId: "existing", phase: "running", focusOwnership: FocusOwnership.Other, initialHandoff,
    } })
    const view = await mount(korrid)
    expect(view.model().status._tag).toBe("Browsing")
    expect(view.currentId()).toBe("existing")
  })
}

for (const selected of [false, true]) {
  const path = selected ? "selected" : "command"
  for (const cancel of [false, true]) {
    test(`${path}: remount during claimed preparation ${cancel ? "cancels before commit" : "follows late commit to exact handoff"}`, async () => {
      const preparation = gate()
      let focus = FocusOwnership.Other
      const client = createInMemoryKorridClient({ ...seed(selected), sessionPreparationGate: preparation.promise,
        sessionFocusOwnership: () => focus })
      const reserves = spyOn(client, "sessionReserve")
      const starts = spyOn(client, "sessionStart")
      const first = await mount(client)
      await first.launch()
      const pending = await client.sessionStatus()
      expect(pending).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ phase: "preparing", session: { launchId: "reserved:1:wl4" } }] } })
      expect(pending._tag === "Ok" && pending.payload.active).toBeUndefined()
      await first.unmount()
      const recovered = await mount(client)
      expect(recovered.model().status).toMatchObject({ _tag: "Busy", actions: [{ enabled: true }] })
      expect(recovered.container.querySelector(".pico-launch-stage")).not.toBeNull()
      if (cancel) {
        await invoke(() => recovered.bus.emit({ type: "back" }))
        await waitFor(() => recovered.model().status._tag === "Browsing")
      }
      await invoke(preparation.resolve)
      if (cancel) expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
      else {
        await invoke(() => recovered.host().reload())
        expect(recovered.model().status._tag).toBe("Busy")
        expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { launchId: "reserved:1:wl4", initialHandoff: "waiting" } } })
        focus = FocusOwnership.Launch
        await waitFor(() => recovered.model().status._tag === "Browsing")
      }
      expect(reserves).toHaveBeenCalledTimes(1); expect(starts).toHaveBeenCalledTimes(1)
      reserves.mockRestore(); starts.mockRestore()
    })
  }
  for (const resolution of ["handoff", "cancel", "exit"] as const) {
    test(`${path}: lost start response cannot retire preparing work; resolves by ${resolution}`, async () => {
      const preparation = gate()
      let focus = FocusOwnership.Other
      const implementation = createInMemoryKorridClient({ ...seed(selected), sessionPreparationGate: preparation.promise,
        sessionFocusOwnership: () => focus })
      let work!: ReturnType<KorridClient["sessionStart"]>
      let starts = 0
      const client: KorridClient = { ...implementation, async sessionStart(request) {
        starts++; work = implementation.sessionStart(request); return unreachable
      } }
      const view = await mount(client)
      await view.launch()
      expect({ ...view.model().status }).toMatchObject({ _tag: "Busy", detail: expect.stringContaining("connection lost") })
      for (let i = 0; i < 3; i++) {
        expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ phase: "preparing" }] } })
        await invoke(() => view.host().reload())
        expect(view.model().status._tag).toBe("Busy")
      }
      if (resolution === "cancel") {
        await invoke(() => view.host().runAction("cancel-launch"))
        await waitFor(() => view.model().status._tag === "Browsing")
      }
      await invoke(preparation.resolve)
      await work
      if (resolution === "cancel") expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
      else {
        await invoke(() => view.host().reload())
        expect(view.model().status._tag).toBe("Busy")
        if (resolution === "handoff") focus = FocusOwnership.Launch
        else await implementation.sessionStop("reserved:1:wl4")
        await waitFor(() => view.model().status._tag !== "Busy")
        expect(view.currentId()).toBe(resolution === "handoff" ? "reserved:1:wl4" : undefined)
      }
      expect(starts).toBe(1)
    })
  }
  test(`${path}: reservation-only uncertain Cancel retries exactly; repeated in-flight Back is suppressed`, async () => {
    const preparation = gate()
    const cancellation = gate()
    const implementation = createInMemoryKorridClient({ ...seed(selected), sessionPreparationGate: preparation.promise })
    const targets: string[] = []
    const client: KorridClient = { ...implementation, async sessionCancel(request) {
      targets.push(request.expectedLaunchId)
      if (targets.length === 1) { await cancellation.promise; return unreachable }
      return implementation.sessionCancel(request)
    } }
    const view = await mount(client)
    let backs = 0
    view.host().input.on("back", () => backs++)
    await view.launch()
    await invoke(() => { view.bus.emit({ type: "back" }); view.bus.emit({ type: "back" }); view.host().runAction("cancel-launch") })
    expect(targets).toEqual(["reserved:1:wl4"])
    expect(view.model().status).toMatchObject({ _tag: "Busy", actions: [{ enabled: false }] })
    expect(backs).toBe(0)
    await invoke(cancellation.resolve)
    expect(view.model().status).toMatchObject({ _tag: "Busy", actions: [{ label: "Retry Cancel", enabled: true }] })
    await invoke(() => view.host().reload())
    expect(view.model().status._tag).toBe("Busy")
    await invoke(() => view.host().runAction("cancel-launch"))
    await waitFor(() => view.model().status._tag === "Browsing")
    await invoke(preparation.resolve)
    expect(await implementation.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
    expect(targets).toEqual(["reserved:1:wl4", "reserved:1:wl4"])
  })
  test(`${path}: lost successful Cancel response can be retried without targeting replacement B`, async () => {
    const implementation = createInMemoryKorridClient({ ...seed(selected), sessionFocusDelayMs: 60_000 })
    const targets: string[] = []
    let outage = false
    const client: KorridClient = { ...implementation, async sessionCancel(request) {
      targets.push(request.expectedLaunchId)
      const outcome = await implementation.sessionCancel(request)
      if (targets.length === 1) { outage = true; return unreachable }
      return outcome
    }, async sessionStatus(timeout) { return outage ? unreachable : implementation.sessionStatus(timeout) } }
    const view = await mount(client)
    await view.launch()
    await invoke(() => view.host().runAction("cancel-launch"))
    expect(view.model().status).toMatchObject({ _tag: "Busy", actions: [{ enabled: true }] })
    const reserved = await implementation.sessionReserve({ gameId: "wl4" })
    if (reserved._tag !== "Ok") throw new Error("reserve failed")
    await implementation.sessionStart({ gameId: "wl4", expectedLaunchId: reserved.payload.launchId })
    // Explicit stale exact retry is safe even before connectivity observations recover.
    await invoke(() => view.host().runAction("cancel-launch"))
    expect(targets).toEqual(["reserved:1:wl4", "reserved:1:wl4"])
    outage = false
    await invoke(() => view.host().reload())
    await waitFor(() => view.currentId() === reserved.payload.launchId)
    expect(await implementation.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { launchId: reserved.payload.launchId } } })
    expect(view.model().status._tag).toBe("Busy")
  })
  test(`${path}: Pending Cancel then failed cleanup reports HostRecoveryBlocked and allows exact recovery`, async () => {
    const commit = gate()
    const implementation = createInMemoryKorridClient({ ...seed(selected), sessionCommitGate: commit.promise,
      sessionFocusDelayMs: 60_000, sessionCleanupFailure: { code: "HostRecoveryBlocked", message: "Exact cleanup failed" } })
    const cancels = spyOn(implementation, "sessionCancel")
    const view = await mount(implementation)
    await view.launch()
    expect(await implementation.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ phase: "committing" }] } })
    await invoke(() => view.bus.emit({ type: "back" }))
    expect(await implementation.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ phase: "cancelling" }] } })
    await invoke(commit.resolve)
    expect({ ...view.model().status }).toMatchObject({ _tag: "Busy", detail: expect.stringContaining("Exact cleanup failed"), actions: [{ enabled: true }] })
    await invoke(() => view.host().runAction("cancel-launch"))
    await waitFor(() => view.model().status._tag !== "Busy")
    expect(cancels).toHaveBeenCalledTimes(2)
    expect(await implementation.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
    cancels.mockRestore()
  })
}

test("multiple pending identities offer exact manual choices, then a newer single candidate recovers", async () => {
  const client = createInMemoryKorridClient(seed(false))
  const first = await client.sessionReserve({ gameId: "wl4" })
  const second = await client.sessionReserve({ gameId: "wl4" })
  if (first._tag !== "Ok" || second._tag !== "Ok") throw new Error("reserve failed")
  const starts = spyOn(client, "sessionStart")
  const cancels = spyOn(client, "sessionCancel")
  const view = await mount(client)
  expect(view.model().status).toMatchObject({ _tag: "Busy", kicker: "2 launches are starting", actions: [
    { id: `cancel-pending:${first.payload.launchId}`, enabled: true },
    { id: `cancel-pending:${second.payload.launchId}`, enabled: true },
  ] })
  await invoke(() => { view.host().launchGame("game:wl4"); view.host().runAction("cancel-launch"); view.bus.emit({ type: "back" }) })
  expect(starts).not.toHaveBeenCalled(); expect(cancels).not.toHaveBeenCalled()
  await client.sessionCancel({ expectedLaunchId: second.payload.launchId })
  await waitFor(() => { const status = view.model().status; return status._tag === "Busy" && status.actions?.length === 1 && status.actions[0]?.id === "cancel-launch" })
  await invoke(() => view.host().runAction("cancel-launch"))
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(cancels).toHaveBeenLastCalledWith({ expectedLaunchId: first.payload.launchId })
  starts.mockRestore(); cancels.mockRestore()
})

test("fresh active waiting plus unrelated pending offers manual choices, not an implicit priority", async () => {
  const client = createInMemoryKorridClient({ ...seed(false), sessionFocusDelayMs: 60_000 })
  const first = await client.sessionReserve({ gameId: "wl4" })
  if (first._tag !== "Ok") throw new Error("reserve failed")
  await client.sessionStart({ gameId: "wl4", expectedLaunchId: first.payload.launchId })
  const second = await client.sessionReserve({ gameId: "wl4" })
  if (second._tag !== "Ok") throw new Error("reserve failed")
  const view = await mount(client)
  expect(view.model().status).toMatchObject({ _tag: "Busy", kicker: "2 launches are starting" })
  await client.sessionCancel({ expectedLaunchId: second.payload.launchId })
  await waitFor(() => { const status = view.model().status; return status._tag === "Busy" && status.actions?.length === 1 && status.actions[0]?.id === "cancel-launch" })
  await invoke(() => view.host().runAction("cancel-launch"))
  await waitFor(() => view.model().status._tag === "Browsing")
})

for (const phase of ["frozen", "focus-failed"]) {
  test(`fresh ${phase} recovery retains exact session without new destructive Cancel`, async () => {
    const client = createInMemoryKorridClient({ ...seed(false), activeSession: {
      gameId: "wl4", launchId: "exact", phase, initialHandoff: InitialHandoff.Waiting,
    } })
    const cancels = spyOn(client, "sessionCancel")
    const view = await mount(client)
    expect(view.model().status._tag).toBe("Browsing")
    expect(view.currentId()).toBe("exact")
    await invoke(() => view.host().runAction("cancel-launch"))
    expect(cancels).not.toHaveBeenCalled(); cancels.mockRestore()
  })
}

test("daemon latches initial handoff; fresh mount after real later focus loss stays Browsing", async () => {
  let focus = FocusOwnership.Other
  const client = createInMemoryKorridClient({ ...seed(false), sessionFocusOwnership: () => focus })
  const first = await mount(client)
  await first.launch()
  focus = FocusOwnership.Launch
  await waitFor(() => first.model().status._tag === "Browsing")
  focus = FocusOwnership.Other
  expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { initialHandoff: "observed", focusOwnership: "other" } } })
  await first.unmount()
  const recovered = await mount(client)
  expect(recovered.model().status._tag).toBe("Browsing")
  const cancels = spyOn(client, "sessionCancel")
  await invoke(() => recovered.host().runAction("cancel-launch"))
  expect(cancels).not.toHaveBeenCalled(); cancels.mockRestore()
})

test("pending identities with native observation failure do not prove idle or handoff", async () => {
  const preparation = gate()
  let unavailable = false
  const failure = { code: "HostRecoveryBlocked", message: "native observation failed" }
  const client = createInMemoryKorridClient({ ...seed(false), sessionPreparationGate: preparation.promise,
    sessionObservationFailure: () => unavailable ? failure : undefined, sessionFocusDelayMs: 60_000 })
  const first = await mount(client)
  await first.launch()
  unavailable = true
  await first.unmount()
  const recovered = await mount(client)
  expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { observationFailure: failure, pendingLaunches: [{ phase: "preparing" }] } })
  expect({ ...recovered.model().status }).toMatchObject({ _tag: "Busy", detail: expect.stringContaining("native observation failed") })
  await invoke(() => recovered.host().reload())
  expect(recovered.model().status._tag).toBe("Busy")
  unavailable = false
  await invoke(() => recovered.host().runAction("cancel-launch"))
  await waitFor(() => recovered.model().status._tag === "Browsing")
  await invoke(preparation.resolve)
  expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
})

test("unavailable initial local status blocks unsafe reserve/start until real reconnect", async () => {
  let unavailable = true
  const client = createInMemoryKorridClient({ ...seed(false), sessionStatusUnavailable: () => unavailable })
  const reserve = spyOn(client, "sessionReserve")
  const view = await mount(client)
  expect({ ...view.model().status }).toMatchObject({ _tag: "Problem", canRetry: true, reason: expect.stringContaining("BrainUnreachable") })
  await invoke(() => view.host().launchGame("game:wl4"))
  expect(reserve).not.toHaveBeenCalled()
  unavailable = false
  await waitFor(() => view.model().status._tag === "Browsing")
  reserve.mockRestore()
})

for (const selected of [false, true]) {
  test(`${selected ? "selected" : "command"}: an old status snapshot A cannot retire or overwrite startup B`, async () => {
    let focus = FocusOwnership.Launch
    const implementation = createInMemoryKorridClient({ ...seed(selected), sessionFocusOwnership: () => focus })
    const delayed = gate()
    let holdNext = false
    let captured = false
    const client: KorridClient = { ...implementation, async sessionStatus(timeout) {
      const result = await implementation.sessionStatus(timeout)
      if (holdNext) { holdNext = false; captured = true; await delayed.promise }
      return result
    } }
    const view = await mount(client)
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.currentId() !== undefined && view.model().status._tag === "Browsing")
    const oldId = view.currentId()!
    holdNext = true
    await waitFor(() => captured)
    await invoke(() => view.host().runGameAction(`now-playing:${oldId}`, "stop"))
    await waitFor(() => view.currentId() === undefined && view.model().status._tag === "Browsing")
    focus = FocusOwnership.Other
    await view.launch()
    const newId = view.currentId()!
    expect(newId).not.toBe(oldId)
    await invoke(delayed.resolve)
    expect(view.model().status._tag).toBe("Busy")
    expect(view.currentId()).toBe(newId)
  })
  for (const replace of [false, true]) {
    test(`${selected ? "selected" : "command"}: delayed successful Return reply cannot resurrect A after ${replace ? "replacement B" : "natural End"}`, async () => {
      let focus = FocusOwnership.Launch
      const implementation = createInMemoryKorridClient({ ...seed(selected), sessionFocusOwnership: () => focus })
      const delayed = gate()
      let thawed = false
      const client: KorridClient = { ...implementation, async sessionThaw(id) {
        const result = await implementation.sessionThaw(id)
        thawed = true
        await delayed.promise
        return result
      } }
      const starts = spyOn(client, "sessionStart")
      const view = await mount(client)
      await invoke(() => view.host().launchGame("game:wl4"))
      await waitFor(() => view.currentId() !== undefined && view.model().status._tag === "Browsing")
      const oldId = view.currentId()!
      await implementation.sessionFreeze(oldId)
      await invoke(() => view.host().reload())
      await invoke(() => view.host().runGameAction(`now-playing:${oldId}`, "resume"))
      await waitFor(() => thawed)
      expect(view.model().status._tag).toBe("Busy")
      await implementation.sessionStop(oldId)
      await waitFor(() => view.currentId() === undefined && view.model().status._tag === "Browsing")
      if (replace) { focus = FocusOwnership.Other; await view.launch() }
      const expectedId = view.currentId()
      await invoke(delayed.resolve)
      expect(view.currentId()).toBe(expectedId)
      expect(view.currentId()).not.toBe(oldId)
      expect(view.model().status._tag).toBe(replace ? "Busy" : "Browsing")
      expect(starts).toHaveBeenCalledTimes(replace ? 2 : 1)
      starts.mockRestore()
    })
  }
}

test("same-frame Back after explicit runner choice is Cancel, never underlying Back", async () => {
  const preparation = gate()
  const client = createInMemoryKorridClient({ ...seed(true), gameRoutes: [{ ...runnerRoutes, gameRunner: undefined, systemRunners: {} }],
    sessionPreparationGate: preparation.promise })
  const view = await mount(client)
  let backs = 0
  view.host().input.on("back", () => backs++)
  await invoke(() => view.host().launchGame("game:wl4"))
  await waitFor(() => view.model().runnerChoice?._tag === "Ready")
  const choice = view.model().runnerChoice
  if (!choice || choice._tag === "Closed") throw new Error("missing choice")
  const action = choice.routes[0]!.actions.find(action => action.label === "Launch once")!
  await invoke(() => { view.host().runAction(action.id); view.bus.emit({ type: "back" }) })
  expect(backs).toBe(0)
  expect(view.model().runnerChoice?._tag).toBe("Closed")
  await waitFor(() => view.model().status._tag === "Browsing")
  await invoke(preparation.resolve)
  expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
})

for (const selected of [false, true]) {
  test(`${selected ? "selected" : "command"}: unavailable status preserves exact Return/End but blocks every replacement path`, async () => {
    const other = { ...game(selected), id: "other", title: "Other game" }
    let failure: { code: string; message: string } | undefined
    const implementation = createInMemoryKorridClient({ ...seed(selected), games: [game(selected), other],
      gameRoutes: [...seed(selected).gameRoutes, { ...runnerRoutes, gameId: other.id }],
      sessionObservationFailure: () => failure })
    const starts = spyOn(implementation, "sessionStart")
    const reserves = spyOn(implementation, "sessionReserve")
    const returns = spyOn(implementation, "sessionThaw")
    const ends = spyOn(implementation, "sessionStop")
    const routes = spyOn(implementation, "gameRoutes")
    const view = await mount(implementation)
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.model().status._tag === "Browsing" && view.currentId() !== undefined)
    const id = view.currentId()!
    await implementation.sessionFreeze(id)
    await invoke(() => view.host().reload())
    const pending = await implementation.sessionReserve({ gameId: "other" })
    if (pending._tag !== "Ok") throw new Error("reserve failed")
    failure = { code: "HostRecoveryBlocked", message: "native status unavailable" }
    const failedStatus = await implementation.sessionStatus()
    expect(failedStatus).toMatchObject({ _tag: "Ok", payload: { observationFailure: failure, pendingLaunches: [{ session: pending.payload }] } })
    await invoke(() => view.host().reload())
    expect(view.currentId()).toBe(id)
    expect({ ...view.model().status }).toMatchObject({ _tag: "Problem", reason: expect.stringContaining("HostRecoveryBlocked"), canRetry: true })
    const reserveCount = reserves.mock.calls.length
    const routeCount = routes.mock.calls.length
    await invoke(() => {
      view.host().launchGame("game:other")
      view.host().runGameAction("game:other", "runners")
      view.host().runAction("runner:launch:0")
    })
    expect(reserves).toHaveBeenCalledTimes(reserveCount)
    expect(starts).toHaveBeenCalledTimes(1)
    expect(routes).toHaveBeenCalledTimes(routeCount)
    // Failed native observation does not consume the daemon's exact Home intent.
    failure = undefined
    expect(await implementation.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { overlay: { launchId: id, phase: "frozen" } } })
    failure = { code: "HostRecoveryBlocked", message: "native status unavailable" }
    await invoke(() => view.host().runGameAction(`now-playing:${id}`, "resume"))
    expect(returns).toHaveBeenCalledWith(id)
    expect(view.currentId()).toBe(id)
    expect(view.model().status._tag).toBe("Problem")
    await invoke(() => view.host().runGameAction(`now-playing:${id}`, "stop"))
    expect(ends).toHaveBeenCalledWith(id)
    expect(view.model().status._tag).toBe("Problem")
    failure = undefined
    await invoke(() => view.host().reload())
    await waitFor(() => view.model().status._tag === "Busy")
    await invoke(() => view.host().runAction("cancel-launch"))
    await waitFor(() => view.model().status._tag === "Browsing")
    expect(starts).toHaveBeenCalledTimes(1)
    starts.mockRestore(); reserves.mockRestore(); returns.mockRestore(); ends.mockRestore(); routes.mockRestore()
  })
}

test("warnings from a delayed start reply survive prior exact handoff but not replacement", async () => {
  const ack = gate()
  const routes = [{ ...runnerRoutes, gameRunner: undefined, systemRunners: {}, routes: runnerRoutes.routes.slice(0, 1).map(route => ({
    ...route, warnings: [{ setting: "video_driver", runnerId: route.runnerId, build: route.runnerBuild, message: "Pinned runtime setting warning" }],
  })) }]
  const client = createInMemoryKorridClient({ ...seed(true), gameRoutes: routes, sessionStartGate: ack.promise })
  const view = await mount(client)
  await invoke(() => view.host().launchGame("game:wl4"))
  await waitFor(() => view.model().status._tag === "Browsing" && view.currentId() !== undefined)
  const id = view.currentId()
  await invoke(ack.resolve)
  await waitFor(() => view.model().runnerChoice?._tag === "Warnings")
  expect(view.currentId()).toBe(id)
  const choice = view.model().runnerChoice
  expect(choice && choice._tag !== "Closed" && choice.warnings.join(" ")).toContain("Pinned runtime setting warning")
})

test("status recovery failure during an in-flight Cancel never advertises an enabled duplicate", async () => {
  const cancel = gate()
  let failure: { code: string; message: string } | undefined
  const implementation = createInMemoryKorridClient({ ...seed(false), sessionFocusDelayMs: 60_000,
    sessionObservationFailure: () => failure })
  let cancels = 0
  const client: KorridClient = { ...implementation, async sessionCancel(request) {
    cancels++; await cancel.promise; return implementation.sessionCancel(request)
  } }
  const view = await mount(client)
  await view.launch()
  await invoke(() => view.host().runAction("cancel-launch"))
  failure = { code: "HostRecoveryBlocked", message: "native failure" }
  await invoke(() => view.host().reload())
  expect(view.model().status).toMatchObject({ _tag: "Busy", actions: [{ enabled: false }] })
  await invoke(() => { view.host().runAction("cancel-launch"); view.bus.emit({ type: "back" }) })
  expect(cancels).toBe(1)
  failure = undefined
  await invoke(cancel.resolve)
  await waitFor(() => view.model().status._tag === "Browsing")
})

for (const presentation of ["pico", "shift"] as const) {
  for (const selected of [false, true]) {
    test(`${presentation} ${selected ? "selected" : "command"}: startup/remount names the actual catalog entry, not another focused game`, async () => {
      const local = { ...game(selected), host: "device-label" }
      const other = { ...game(false), id: "other", title: "Other game", playStats: {
        lastPlayed: "2026-10-01T00:00:00Z", playCount: 1, totalPlaytimeSeconds: 1,
      } }
      const peer = { ...game(false), host: "peer", title: "Unrelated peer game", source: { label: "Peer", isLocal: false } }
      const client = createInMemoryKorridClient({ ...seed(selected), games: [other, peer, local], sessionFocusDelayMs: 60_000 })
      const reserves = spyOn(client, "sessionReserve")
      const starts = spyOn(client, "sessionStart")
      const cancels = spyOn(client, "sessionCancel")
      const options = { presentation, initialGameId: "game:other" }
      const first = await mount(client, options)
      if (presentation === "shift") {
        const otherButton = first.container.querySelector<HTMLButtonElement>('button[data-shift-game-id="game:other"]')
        expect(otherButton).not.toBeNull()
        await invoke(() => otherButton!.focus())
        expect(document.activeElement).toBe(otherButton)
      } else expect(first.container.querySelector(".pico-game-detail")?.textContent).toContain("Other game")
      await invoke(() => first.host().launchGame("game:device-label:wl4"))
      await waitFor(() => first.model().status._tag === "Busy")
      expect(first.model().status).toMatchObject({ _tag: "Busy", gameId: "game:device-label:wl4" })
      const id = first.currentId()!
      await first.unmount()
      const recovered = await mount(client, options)
      expect(recovered.model().status).toMatchObject({ _tag: "Busy", gameId: "game:device-label:wl4" })
      if (presentation === "pico") {
        expect(recovered.container.querySelector('.pico-launch-stage-cart [aria-label="Wario Land 4"]')).not.toBeNull()
        expect(recovered.container.querySelector(".pico-launch-stage")?.textContent).not.toContain("Other game")
      } else {
        const hero = recovered.container.querySelector(".shift-cine-hero-stack")
        expect(hero?.getAttribute("data-korri-instance-id")).toBe("game:device-label:wl4")
        expect(hero?.textContent).toContain("Wario Land 4")
        expect(hero?.textContent).not.toContain("Other game")
        expect(hero?.textContent).not.toContain("Unrelated peer game")
      }
      await invoke(() => recovered.host().runAction("cancel-launch"))
      await waitFor(() => recovered.model().status._tag === "Browsing")
      expect(cancels).toHaveBeenCalledWith({ expectedLaunchId: id })
      expect(reserves).toHaveBeenCalledTimes(1); expect(starts).toHaveBeenCalledTimes(1)
      reserves.mockRestore(); starts.mockRestore(); cancels.mockRestore()
    })
  }
  test(`${presentation}: missing optional active game metadata stays unknown while exact Cancel remains effective`, async () => {
    // Deliberately collide a launch id with an unrelated real content id: the
    // two identities must never be treated as interchangeable.
    const unrelated = { ...game(false), id: "metadata-free-launch", title: "Unrelated selected game" }
    const active = { launchId: unrelated.id, phase: "running", initialHandoff: InitialHandoff.Waiting }
    const client = createInMemoryKorridClient({ games: [unrelated, game(false)], activeSession: active })
    const reserves = spyOn(client, "sessionReserve")
    const starts = spyOn(client, "sessionStart")
    const cancels = spyOn(client, "sessionCancel")
    const options = { presentation, initialGameId: "game:metadata-free-launch" }
    const first = await mount(client, options)
    await first.unmount()
    const recovered = await mount(client, options)
    const status = recovered.model().status
    expect(status._tag).toBe("Busy")
    expect(status).not.toHaveProperty("gameId")
    if (presentation === "pico") {
      expect(recovered.container.querySelector(".pico-launch-stage-cart")).toBeNull()
      expect(recovered.container.querySelector(".pico-launch-stage")?.textContent).not.toContain("Unrelated selected game")
    } else {
      expect(recovered.container.querySelectorAll('[data-korri-part="shift.cine-hero"]').length).toBe(0)
      expect(recovered.container.querySelector(".shift-cine-heroband")?.textContent).toContain("Starting live session")
      expect(recovered.container.querySelector(".shift-cine-heroband")?.textContent).not.toContain("Unrelated selected game")
    }
    const observed = await client.sessionStatus()
    expect(observed._tag === "Ok" && observed.payload.active?.gameId).toBeUndefined()
    await invoke(() => recovered.bus.emit({ type: "back" }))
    await waitFor(() => recovered.model().status._tag === "Browsing")
    expect(cancels).toHaveBeenCalledWith({ expectedLaunchId: active.launchId })
    expect(reserves).not.toHaveBeenCalled(); expect(starts).not.toHaveBeenCalled()
    reserves.mockRestore(); starts.mockRestore(); cancels.mockRestore()
  })
  test(`${presentation}: an exact local startup failure cannot be attributed to a same-id peer copy`, async () => {
    const local = { ...game(false), host: "device-label" }
    const peer = { ...game(false), host: "peer", title: "Unrelated peer game", source: { label: "Peer", isLocal: false } }
    const client = createInMemoryKorridClient({ games: [peer, local], behavior: "prepare-fail" })
    const view = await mount(client, { presentation, initialGameId: "game:peer:wl4" })
    await invoke(() => view.host().launchGame("game:device-label:wl4"))
    await waitFor(() => view.model().status._tag === "Problem")
    expect(view.model().status).toMatchObject({ _tag: "Problem", gameId: "game:device-label:wl4", gameTitle: "Wario Land 4" })
  })
}
