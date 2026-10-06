import { afterEach, expect, spyOn, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { FocusOwnership, InitialHandoff, type Game } from "@contracts/generated/korrid"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type InMemoryKorridClientConfig } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"
import { runnerRoutes } from "./fixtures/runner-routes"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
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
async function mount(selected: boolean, config: InMemoryKorridClientConfig = {}) {
  const korrid = createInMemoryKorridClient({
    games: [game(selected)],
    gameRoutes: [{ ...runnerRoutes, gameRunner: undefined, systemRunners: {}, routes: runnerRoutes.routes.slice(0, 1) }],
    ...config,
  })
  let model!: SurfaceModel
  let host!: SurfaceHost
  const bus = createInputBus()
  let backs = 0
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.push(root)
  await act(async () => root.render(<SurfaceRoot bus={bus} korrid={korrid} surface={{
    id: "acceptance", title: "Acceptance", presentations: ["catalog"],
    render: props => {
      model = props.model
      host = props.host
      return <PicoSurface {...props} initialView={{ _tag: "Detail", gameId: "game:wl4" }} />
    },
  }} />))
  await waitFor(() => model.catalog._tag === "Ready")
  host.input.on("back", () => backs++)
  return {
    korrid, container, bus, model: () => model, host: () => host, backs: () => backs,
    async launch() {
      await invoke(() => host.launchGame("game:wl4"))
      await waitFor(() => model.status._tag === "Busy")
    },
    async back() { await invoke(() => bus.emit({ type: "back" })) },
    currentId() {
      return model.catalog._tag === "Ready" ? model.catalog.games.find(item => item.resumable)?.id.replace("now-playing:", "") : undefined
    },
  }
}

for (const selected of [false, true]) {
  const path = selected ? "selected runner" : "configured command"
  test(`${path}: ACK without exact focus keeps the shared Pico launch stage`, async () => {
    const view = await mount(selected, { sessionFocusDelayMs: 800 })
    await view.launch()
    expect(view.model().runnerChoice?._tag).toBe("Closed")
    expect(view.container.querySelector(".pico-launch-stage")).not.toBeNull()
    expect(view.model().status).toMatchObject({ _tag: "Busy", actions: [{ id: "cancel-launch", label: "Cancel", enabled: true }] })
    await waitFor(() => view.model().status._tag === "Browsing")
    expect(view.container.querySelector(".pico-launch-stage")).toBeNull()
    const id = view.currentId()!
    await invoke(() => { void view.korrid.sessionFreeze(id) })
    await waitFor(() => view.model().status._tag === "Browsing")
    await invoke(() => window.dispatchEvent(new Event("focus")))
    expect(view.model().status._tag).toBe("Browsing")
    await invoke(() => view.host().runGameAction(`now-playing:${id}`, "resume"))
    await waitFor(() => view.model().status._tag === "Browsing")
    expect(view.currentId()).toBe(id)
    await invoke(() => view.host().runGameAction(`now-playing:${id}`, "stop"))
    await waitFor(() => view.currentId() === undefined && view.model().status._tag === "Browsing")
  })
  test(`${path}: cancel before reservation reply never dispatches start`, async () => {
    const reserve = gate()
    const view = await mount(selected, { sessionReserveGate: reserve.promise })
    const starts = spyOn(view.korrid, "sessionStart")
    const cancels = spyOn(view.korrid, "sessionCancel")
    await view.launch()
    await view.back()
    expect(view.backs()).toBe(0)
    expect(view.model().status).toMatchObject({ _tag: "Busy", kicker: "Cancelling launch…" })
    await invoke(reserve.resolve)
    await waitFor(() => view.model().status._tag === "Browsing")
    expect(await view.korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
    expect(view.currentId()).toBeUndefined()
    expect(starts).not.toHaveBeenCalled()
    expect(cancels).toHaveBeenCalledTimes(1)
    expect(cancels).toHaveBeenCalledWith({ expectedLaunchId: "reserved:1:wl4" })
    starts.mockRestore()
    cancels.mockRestore()
  })
  test(`${path}: cancel during startup does not wait for ACK or End a replacement`, async () => {
    const start = gate()
    const view = await mount(selected, { sessionStartGate: start.promise, sessionFocusDelayMs: 60_000 })
    const cancels = spyOn(view.korrid, "sessionCancel")
    await view.launch()
    await waitFor(() => view.currentId() !== undefined)
    const oldId = view.currentId()!
    await invoke(() => view.host().runAction("cancel-launch"))
    await waitFor(() => view.model().status._tag === "Browsing")
    expect(cancels).toHaveBeenCalledTimes(1)
    expect(cancels).toHaveBeenCalledWith({ expectedLaunchId: oldId })
    const reservation = await view.korrid.sessionReserve({ gameId: "wl4" })
    if (reservation._tag !== "Ok") throw new Error("reservation failed")
    const replacement = view.korrid.sessionStart({ gameId: "wl4", expectedLaunchId: reservation.payload.launchId, ...(selected ? { runnerId: "retroarch/mgba" } : {}) })
    await invoke(start.resolve)
    await replacement
    await invoke(() => view.host().reload())
    await waitFor(() => view.currentId() === reservation.payload.launchId)
    expect(view.currentId()).not.toBe(oldId)
    expect((await view.korrid.sessionStatus())._tag).toBe("Ok")
    expect(cancels).toHaveBeenCalledTimes(1)
    cancels.mockRestore()
  })
  test(`${path}: early exit resolves startup without focus or browser blur`, async () => {
    const view = await mount(selected, { sessionFocusDelayMs: 60_000 })
    await view.launch()
    await waitFor(() => view.currentId() !== undefined)
    await invoke(() => { void view.korrid.sessionStop(view.currentId()!) })
    await waitFor(() => view.model().status._tag === "Browsing" && view.currentId() === undefined)
  })
  test(`${path}: frozen status after missing final focus reply recovers browsing`, async () => {
    const view = await mount(selected, { sessionFocusDelayMs: 60_000 })
    await view.launch()
    await waitFor(() => view.currentId() !== undefined)
    await invoke(() => { void view.korrid.sessionFreeze(view.currentId()!) })
    await invoke(() => window.dispatchEvent(new Event("focus")))
    await waitFor(() => view.model().status._tag === "Browsing")
    expect(view.currentId()).toBeDefined()
  })
}

test("status unavailable retains cancellable startup and never relaunches", async () => {
  let unavailable = false
  const view = await mount(false, { sessionStatusUnavailable: () => unavailable, sessionFocusDelayMs: 60_000 })
  await view.launch()
  unavailable = true
  expect(await view.korrid.sessionStatus()).toMatchObject({ _tag: "Err", payload: { code: "BrainUnreachable" } })
  await act(async () => { await new Promise(done => setTimeout(done, 600)) })
  expect(view.model().status._tag).toBe("Busy")
  const id = view.currentId()
  await invoke(() => { view.host().reload(); view.host().launchGame("game:wl4") })
  expect(view.model().status._tag).toBe("Busy")
  expect(view.currentId()).toBe(id)
  await view.back()
  // Exact cancel succeeded, but unavailable global status still blocks new work.
  await waitFor(() => view.model().status._tag === "Problem")
  unavailable = false
  await invoke(() => view.host().reload())
  await waitFor(() => view.model().status._tag === "Browsing")
})

for (const selected of [false, true]) {
  test(`${selected ? "selected" : "command"}: Pending cancel stays Busy through unavailable status and late ACK`, async () => {
    const start = gate()
    let unavailable = false
    const view = await mount(selected, {
      sessionStartGate: start.promise, sessionFocusDelayMs: 60_000,
      sessionCancelPending: true, sessionStatusUnavailable: () => unavailable,
    })
    await view.launch()
    await waitFor(() => view.currentId() !== undefined)
    const id = view.currentId()!
    unavailable = true
    expect(await view.korrid.sessionStatus()).toMatchObject({ _tag: "Err", payload: { code: "BrainUnreachable" } })
    await view.back()
    await invoke(() => { view.host().runAction("cancel-launch"); start.resolve() })
    await act(async () => { await new Promise(done => setTimeout(done, 600)) })
    expect(view.model().status).toMatchObject({ _tag: "Busy", kicker: "Cancelling launch…" })
    expect(view.model().status).toMatchObject({ _tag: "Busy", actions: [{ id: "cancel-launch", enabled: false }] })
    expect(view.backs()).toBe(0)
    expect(view.currentId()).toBe(id)
    await invoke(() => { void view.korrid.sessionStop(id) })
    unavailable = false
    await waitFor(() => view.model().status._tag === "Browsing" && view.currentId() === undefined)
  })
}

for (const selected of [false, true]) {
  test(`${selected ? "selected" : "command"}: observed early End retires a delayed ACK without resurrection`, async () => {
    const start = gate()
    const view = await mount(selected, { sessionStartGate: start.promise, sessionFocusDelayMs: 60_000 })
    await view.launch()
    await waitFor(() => view.currentId() !== undefined)
    // Allow the real observer to sample the exact running launch before End.
    await act(async () => { await new Promise(done => setTimeout(done, 550)) })
    await invoke(() => { void view.korrid.sessionStop(view.currentId()!) })
    await waitFor(() => view.model().status._tag === "Browsing" && view.currentId() === undefined)
    await invoke(start.resolve)
    expect(view.model().status._tag).toBe("Browsing")
    expect(view.currentId()).toBeUndefined()
  })
}

const warningRoutes = [{
  ...runnerRoutes, gameRunner: undefined, systemRunners: {},
  routes: runnerRoutes.routes.slice(0, 1).map(route => ({ ...route, warnings: [{
    setting: "video_driver", runnerId: route.runnerId, build: route.runnerBuild,
    message: "Pinned runtime does not support this setting",
  }] })),
}]

test("selected warnings survive handoff; Done does not cancel the running session", async () => {
  const view = await mount(true, { gameRoutes: warningRoutes, sessionFocusDelayMs: 500 })
  const cancels = spyOn(view.korrid, "sessionCancel")
  await view.launch()
  await waitFor(() => view.model().runnerChoice?._tag === "Warnings")
  expect(view.model().status._tag).toBe("Busy")
  expect(view.container.querySelector(".pico-launch-stage")).not.toBeNull()
  await waitFor(() => view.model().status._tag === "Browsing")
  const id = view.currentId()
  expect(view.model().runnerChoice).toMatchObject({ _tag: "Warnings", warnings: [expect.stringContaining("Pinned runtime")] })
  const warnings = view.model().runnerChoice
  if (!warnings || warnings._tag !== "Warnings") throw new Error("expected warnings")
  const done = warnings.actions.find(action => action.label === "Done")
  if (!done) throw new Error("missing published Done action")
  await invoke(() => view.host().runAction(done.id))
  expect(view.model().runnerChoice?._tag).toBe("Closed")
  expect(view.currentId()).toBe(id)
  expect(cancels).not.toHaveBeenCalled()
  expect(await view.korrid.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { launchId: id } } })
  cancels.mockRestore()
})

test("Done on selected warnings before handoff dismisses only the panel and startup completes normally", async () => {
  let ownership = FocusOwnership.Other
  const view = await mount(true, { gameRoutes: warningRoutes, sessionFocusOwnership: () => ownership })
  const cancels = spyOn(view.korrid, "sessionCancel")
  await view.launch()
  await waitFor(() => view.model().runnerChoice?._tag === "Warnings")
  const id = view.currentId()
  expect(view.model().status._tag).toBe("Busy")
  const done = [...view.container.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent?.trim() === "Done")
  if (!done) throw new Error("missing published Done button")
  await invoke(() => done.click())
  expect(view.model().runnerChoice?._tag).toBe("Closed")
  expect(cancels).not.toHaveBeenCalled()
  expect(view.model().status._tag).toBe("Busy")
  expect(view.currentId()).toBe(id)
  expect(await view.korrid.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { launchId: id, initialHandoff: InitialHandoff.Waiting } } })
  ownership = FocusOwnership.Launch
  await invoke(() => view.host().reload())
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(view.currentId()).toBe(id)
  expect(cancels).not.toHaveBeenCalled()
  cancels.mockRestore()
})

test("closing the warning panel (Shift's close button and scrim send runner:cancel) never ends startup", async () => {
  const view = await mount(true, { gameRoutes: warningRoutes, sessionFocusDelayMs: 60_000 })
  const cancels = spyOn(view.korrid, "sessionCancel")
  await view.launch()
  await waitFor(() => view.model().runnerChoice?._tag === "Warnings")
  const id = view.currentId()
  await invoke(() => view.host().runAction("runner:cancel"))
  expect(view.model().runnerChoice?._tag).toBe("Closed")
  expect(cancels).not.toHaveBeenCalled()
  expect(view.model().status._tag).toBe("Busy")
  expect(view.currentId()).toBe(id)
  cancels.mockRestore()
})

test("Back while ACK warnings exist cancels shared startup, not only the warning panel", async () => {
  const view = await mount(true, { gameRoutes: warningRoutes, sessionFocusDelayMs: 60_000 })
  await view.launch()
  await waitFor(() => view.model().runnerChoice?._tag === "Warnings")
  await view.back()
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(view.model().runnerChoice?._tag).toBe("Closed")
  expect(view.currentId()).toBeUndefined()
  expect(view.backs()).toBe(0)
})

for (const selected of [false, true]) {
  test(`${selected ? "selected" : "command"}: startup failure names its game and reports the error`, async () => {
    const view = await mount(selected, { behavior: "prepare-fail" })
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.model().status._tag === "Problem")
    expect(view.model().status).toMatchObject({ _tag: "Problem", kicker: "Couldn't start Wario Land 4" })
    expect(view.currentId()).toBeUndefined()
  })
}

test("same-frame Back cancels the shared startup without reaching the underlying surface", async () => {
  const reserve = gate()
  const view = await mount(false, { sessionReserveGate: reserve.promise })
  await invoke(() => {
    view.host().launchGame("game:wl4")
    view.bus.emit({ type: "back" })
  })
  expect(view.backs()).toBe(0)
  expect(view.model().status).toMatchObject({ _tag: "Busy", kicker: "Cancelling launch…" })
  await invoke(reserve.resolve)
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(view.currentId()).toBeUndefined()
})
