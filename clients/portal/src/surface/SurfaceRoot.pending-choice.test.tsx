import { afterEach, expect, spyOn, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { InitialHandoff, type Game } from "@contracts/generated/korrid"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { PicoSurface } from "@korri/pico"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"

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
const game: Game = { id: "wl4", title: "Wario Land 4", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }
const other: Game = { ...game, id: "other", title: "Other game" }
const unreachable = { _tag: "Err" as const, payload: { code: "BrainUnreachable", message: "Connection lost" } }
async function reserve(client: KorridClient, gameId = game.id) {
  const result = await client.sessionReserve({ gameId })
  if (result._tag !== "Ok") throw new Error(result.payload.message)
  return result.payload.launchId
}
async function mount(korrid: KorridClient) {
  let model!: SurfaceModel
  let host!: SurfaceHost
  const bus = createInputBus()
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.add(root)
  await act(async () => root.render(<SurfaceRoot bus={bus} korrid={korrid} surface={{
    id: "manual-choice", title: "Manual choice", presentations: ["catalog"],
    render: props => { model = props.model; host = props.host; return <PicoSurface {...props} /> },
  }} />))
  await waitFor(() => model.catalog._tag !== "Loading")
  let backs = 0
  host.input.on("back", () => backs++)
  return {
    container, bus, host: () => host, model: () => model, backs: () => backs,
    actions() { const status = model.status; if (status._tag !== "Busy") throw new Error("expected Busy"); return status.actions ?? [] },
    button(label: string) { const button = [...container.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent?.trim() === label); if (!button) throw new Error(`missing ${label}`); return button },
  }
}

test("fresh same-game choices publish distinct exact actions without launching or cancelling", async () => {
  const implementation = createInMemoryKorridClient({ games: [game] })
  const first = await reserve(implementation)
  const second = await reserve(implementation)
  const delayed = gate()
  const targets: string[] = []
  const client: KorridClient = { ...implementation, async sessionCancel(request) {
    targets.push(request.expectedLaunchId)
    if (request.expectedLaunchId === first) await delayed.promise
    return implementation.sessionCancel(request)
  } }
  const starts = spyOn(client, "sessionStart")
  const view = await mount(client)
  expect(view.model().status).toEqual({ _tag: "Busy", kicker: "2 launches are starting", detail: "Cancel each launch you do not want.", actions: [
    { id: `cancel-pending:${first}`, label: "Cancel Wario Land 4 (1)", description: "Waiting to start", enabled: true },
    { id: `cancel-pending:${second}`, label: "Cancel Wario Land 4 (2)", description: "Waiting to start", enabled: true },
  ] })
  expect(view.container.querySelectorAll(".pico-launch-stage button").length).toBe(2)
  expect(starts).not.toHaveBeenCalled(); expect(targets).toEqual([])
  await invoke(() => view.bus.emit({ type: "back" }))
  expect(view.backs()).toBe(0); expect(targets).toEqual([])
  await invoke(() => { view.button("Cancel Wario Land 4 (1)").click(); view.host().runAction(`cancel-pending:${first}`); view.bus.emit({ type: "back" }); view.host().runAction("cancel-pending:unknown") })
  expect(targets).toEqual([first])
  expect(view.actions().map(action => action.enabled)).toEqual([false, true])
  expect(view.button("Cancel Wario Land 4 (1)").disabled).toBe(true)
  expect(view.button("Cancel Wario Land 4 (2)").disabled).toBe(false)
  expect(view.backs()).toBe(0)
  await invoke(delayed.resolve)
  await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
  expect(view.actions()).toEqual([{ id: "cancel-launch", label: "Cancel", enabled: true }])
  await invoke(() => view.button("Cancel").click())
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(targets).toEqual([first, second])
  expect(await implementation.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  expect(starts).not.toHaveBeenCalled(); starts.mockRestore()
})

for (const code of ["BrainUnreachable", "HostRecoveryBlocked", "PermissionDenied"]) {
  test(`${code}: failed choice retains an exact retry without affecting its sibling`, async () => {
    const implementation = createInMemoryKorridClient({ games: [game] })
    const first = await reserve(implementation)
    const second = await reserve(implementation)
    const targets: string[] = []
    const client: KorridClient = { ...implementation, async sessionCancel(request) {
      targets.push(request.expectedLaunchId)
      return targets.length === 1 ? { _tag: "Err", payload: { code, message: "Exact cancel failed" } } : implementation.sessionCancel(request)
    } }
    const view = await mount(client)
    await invoke(() => view.host().runAction(`cancel-pending:${first}`))
    expect(view.actions()).toEqual([
      { id: `cancel-pending:${first}`, label: "Cancel Wario Land 4 (1)", description: "Cancel failed. Try again.", enabled: true },
      { id: `cancel-pending:${second}`, label: "Cancel Wario Land 4 (2)", description: "Waiting to start", enabled: true },
    ])
    expect(view.model().status).toMatchObject({ _tag: "Busy", detail: "Exact cancel failed" })
    await invoke(() => view.host().reload())
    expect(view.actions()[0]?.description).toBe("Cancel failed. Try again.")
    await invoke(() => view.button("Cancel Wario Land 4 (1)").click())
    await waitFor(() => view.actions().length === 1)
    expect(targets).toEqual([first, first])
    expect(await implementation.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ session: { launchId: second } }] } })
  })
}

test("different-game choices do not number their labels", async () => {
  const client = createInMemoryKorridClient({ games: [game, other] })
  const first = await reserve(client)
  const second = await reserve(client, other.id)
  const view = await mount(client)
  expect(view.actions().map(action => ({ id: action.id, label: action.label }))).toEqual([
    { id: `cancel-pending:${first}`, label: "Cancel Wario Land 4" },
    { id: `cancel-pending:${second}`, label: "Cancel Other game" },
  ])
})

test("active initial waiting plus another reservation offers exact live cancellation only", async () => {
  const client = createInMemoryKorridClient({ games: [game, other], activeSession: {
    launchId: "live:A", gameId: game.id, phase: "running", initialHandoff: InitialHandoff.Waiting,
  } })
  const pending = await reserve(client, other.id)
  const cancels = spyOn(client, "sessionCancel")
  const starts = spyOn(client, "sessionStart")
  const view = await mount(client)
  expect(view.actions()).toEqual([
    { id: "cancel-pending:live:A", label: "Cancel Wario Land 4", description: "Waiting for its window", enabled: true },
    { id: `cancel-pending:${pending}`, label: "Cancel Other game", description: "Waiting to start", enabled: true },
  ])
  await invoke(() => view.host().runAction("cancel-pending:live:A"))
  await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
  expect(cancels).toHaveBeenCalledTimes(1)
  expect(cancels).toHaveBeenCalledWith({ expectedLaunchId: "live:A" })
  expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ session: { launchId: pending } }] } })
  expect(starts).not.toHaveBeenCalled()
  cancels.mockRestore(); starts.mockRestore()
})

test("daemon cancelling facts keep that choice disabled and never dispatch another cancel", async () => {
  const commit = gate()
  const client = createInMemoryKorridClient({ games: [game], sessionCommitGate: commit.promise })
  const first = await reserve(client)
  const work = client.sessionStart({ gameId: game.id, expectedLaunchId: first })
  // Await the claim/commit microtasks through the real client before cancellation.
  await invoke(() => {})
  expect(await client.sessionCancel({ expectedLaunchId: first })).toMatchObject({ _tag: "Ok", payload: { phase: "pending" } })
  const second = await reserve(client)
  const cancels = spyOn(client, "sessionCancel")
  const view = await mount(client)
  expect(view.actions()).toEqual([
    { id: `cancel-pending:${first}`, label: "Cancel Wario Land 4 (1)", description: "Cancelling", enabled: false },
    { id: `cancel-pending:${second}`, label: "Cancel Wario Land 4 (2)", description: "Waiting to start", enabled: true },
  ])
  await invoke(() => { view.host().runAction(`cancel-pending:${first}`); view.button("Cancel Wario Land 4 (1)").click() })
  expect(cancels).not.toHaveBeenCalled()
  await invoke(() => view.host().runAction(`cancel-pending:${second}`))
  await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
  expect(view.actions()[0]?.enabled).toBe(false)
  await invoke(commit.resolve)
  await work
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(cancels).toHaveBeenCalledTimes(1); cancels.mockRestore()
})

for (const fail of [false, true]) {
  test(`a surviving sent choice becomes single cancelling and its late ${fail ? "error" : "success"} still resolves exactly`, async () => {
    const implementation = createInMemoryKorridClient({ games: [game] })
    const first = await reserve(implementation)
    const second = await reserve(implementation)
    const delayed = gate()
    const targets: string[] = []
    const client: KorridClient = { ...implementation, async sessionCancel(request) {
      targets.push(request.expectedLaunchId)
      if (targets.length === 1) { await delayed.promise; if (fail) return unreachable }
      return implementation.sessionCancel(request)
    } }
    const view = await mount(client)
    await invoke(() => view.host().runAction(`cancel-pending:${first}`))
    await invoke(() => view.host().runAction(`cancel-pending:${second}`))
    await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
    expect(view.actions()).toEqual([{ id: "cancel-launch", label: "Cancel", enabled: false }])
    expect(view.model().status).toMatchObject({ _tag: "Busy", kicker: "Cancelling launch…" })
    await invoke(() => { view.host().runAction("cancel-launch"); view.host().runAction(`cancel-pending:${first}`); view.bus.emit({ type: "back" }) })
    expect(targets).toEqual([first, second])
    await invoke(delayed.resolve)
    if (fail) {
      await waitFor(() => view.actions()[0]?.enabled === true)
      expect(view.actions()).toEqual([{ id: "cancel-launch", label: "Retry Cancel", enabled: true }])
      expect(view.model().status).toMatchObject({ _tag: "Busy", detail: "Connection lost" })
      await invoke(() => view.button("Retry Cancel").click())
    }
    await waitFor(() => view.model().status._tag === "Browsing")
    expect(targets).toEqual(fail ? [first, second, first] : [first, second])
    expect(await implementation.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  })
}

for (const nativeFailure of [false, true]) {
  test(`${nativeFailure ? "native" : "transport"} observation failure retains choices and exact retry while refusing replacement work`, async () => {
    const implementation = createInMemoryKorridClient({ games: [game, other] })
    const first = await reserve(implementation)
    const second = await reserve(implementation, other.id)
    let unavailable = false
    const targets: string[] = []
    const client: KorridClient = { ...implementation,
      sessionCancel: async request => { targets.push(request.expectedLaunchId); return unreachable },
      sessionStatus: async () => unavailable
        ? nativeFailure ? { _tag: "Ok", payload: { observationFailure: { code: "NativeQueryFailed", message: "Window unknown" } } } : unreachable
        : implementation.sessionStatus(),
    }
    const starts = spyOn(client, "sessionStart")
    const reservations = spyOn(client, "sessionReserve")
    const view = await mount(client)
    await invoke(() => view.host().runAction(`cancel-pending:${first}`))
    unavailable = true
    await invoke(() => view.host().reload())
    expect(view.model().status).toMatchObject({ _tag: "Busy", kicker: "2 launches are starting", detail: nativeFailure ? "Window unknown" : "Connection lost" })
    expect(view.actions().map(action => [action.id, action.description, action.enabled])).toEqual([
      [`cancel-pending:${first}`, "Cancel failed. Try again.", true],
      [`cancel-pending:${second}`, "Waiting to start", true],
    ])
    await invoke(() => { view.host().launchGame("game:other"); view.host().runAction("cancel-launch"); view.bus.emit({ type: "back" }) })
    expect(starts).not.toHaveBeenCalled(); expect(reservations).not.toHaveBeenCalled()
    expect(targets).toEqual([first]); expect(view.backs()).toBe(0)
    unavailable = false
    await invoke(() => view.host().reload())
    expect(view.actions().map(action => action.id)).toEqual([`cancel-pending:${first}`, `cancel-pending:${second}`])
    expect(view.actions()[0]?.description).toBe("Cancel failed. Try again.")
    expect(view.model().status).toMatchObject({ _tag: "Busy", detail: "Cancel each launch you do not want." })
    starts.mockRestore(); reservations.mockRestore()
  })
}

for (const lateError of [false, true]) {
  test(`a retired choice's held ${lateError ? "error" : "success"} cannot affect replacement startup`, async () => {
    const implementation = createInMemoryKorridClient({ games: [game, other], sessionFocusDelayMs: 60_000 })
    const first = await reserve(implementation)
    const second = await reserve(implementation)
    const delayed = gate()
    const targets: string[] = []
    let held = false
    const client: KorridClient = { ...implementation, async sessionCancel(request) {
      targets.push(request.expectedLaunchId)
      const result = await implementation.sessionCancel(request)
      if (request.expectedLaunchId === first) { held = true; await delayed.promise; if (lateError) return unreachable }
      return result
    } }
    const view = await mount(client)
    await invoke(() => view.host().runAction(`cancel-pending:${first}`))
    await waitFor(() => held)
    await invoke(() => view.host().reload())
    await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
    await invoke(() => view.host().runAction("cancel-launch"))
    await waitFor(() => view.model().status._tag === "Browsing")
    await invoke(() => view.host().launchGame("game:other"))
    await waitFor(() => view.model().status._tag === "Busy")
    const replacement = await implementation.sessionStatus()
    if (replacement._tag !== "Ok" || !replacement.payload.active) throw new Error("missing replacement")
    const replacementId = replacement.payload.active.launchId
    await invoke(delayed.resolve)
    expect(view.model().status).toMatchObject({ _tag: "Busy", gameId: "game:other", kicker: "Starting Other game…", detail: "Opening your session" })
    expect(await implementation.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { launchId: replacementId } } })
    expect(targets).toEqual([first, second])
  })
}

test("an old multiple-choice status cannot resurrect choices after cancellation and replacement", async () => {
  const implementation = createInMemoryKorridClient({ games: [game, other], sessionFocusDelayMs: 60_000 })
  const first = await reserve(implementation)
  const second = await reserve(implementation)
  const delayed = gate()
  let holdNext = false
  let held = false
  const client: KorridClient = { ...implementation, async sessionStatus() {
    const result = await implementation.sessionStatus()
    if (holdNext) { holdNext = false; held = true; await delayed.promise }
    return result
  } }
  const cancels = spyOn(client, "sessionCancel")
  const view = await mount(client)
  holdNext = true
  await invoke(() => view.host().reload())
  await waitFor(() => held)
  await invoke(() => view.host().runAction(`cancel-pending:${first}`))
  await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
  await invoke(() => view.host().runAction("cancel-launch"))
  await waitFor(() => view.model().status._tag === "Browsing")
  await invoke(() => view.host().launchGame("game:other"))
  await waitFor(() => view.model().status._tag === "Busy")
  await invoke(delayed.resolve)
  expect(view.model().status).toMatchObject({ _tag: "Busy", gameId: "game:other", kicker: "Starting Other game…" })
  expect(cancels.mock.calls.map(call => call[0]?.expectedLaunchId)).toEqual([first, second])
  cancels.mockRestore()
})

test("a known exact startup owner keeps precedence over another pending launch", async () => {
  const preparation = gate()
  const client = createInMemoryKorridClient({ games: [game], sessionPreparationGate: preparation.promise })
  const view = await mount(client)
  await invoke(() => view.host().launchGame("game:wl4"))
  const owned = await client.sessionStatus()
  if (owned._tag !== "Ok" || !owned.payload.pendingLaunches?.[0]) throw new Error("missing owned launch")
  const first = owned.payload.pendingLaunches[0].session.launchId
  const second = await reserve(client)
  const cancels = spyOn(client, "sessionCancel")
  await invoke(() => view.host().reload())
  expect(view.actions()).toEqual([{ id: "cancel-launch", label: "Cancel", enabled: true }])
  await invoke(() => view.host().runAction(`cancel-pending:${second}`))
  expect(cancels).not.toHaveBeenCalled()
  await invoke(() => view.bus.emit({ type: "back" }))
  expect(cancels).toHaveBeenCalledTimes(1)
  expect(cancels).toHaveBeenCalledWith({ expectedLaunchId: first })
  await invoke(preparation.resolve)
  await waitFor(() => view.actions().length === 1 && view.actions()[0]?.enabled === true)
  expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ session: { launchId: second } }] } })
  cancels.mockRestore()
})

for (const code of ["StaleLaunchIdentity", "NoActiveSession", "SessionCompleted"]) {
  test(`${code} only re-observes and never cancels a surviving sibling`, async () => {
    const implementation = createInMemoryKorridClient({ games: [game] })
    const first = await reserve(implementation)
    const second = await reserve(implementation)
    const targets: string[] = []
    const client: KorridClient = { ...implementation, async sessionCancel(request) {
      targets.push(request.expectedLaunchId)
      await implementation.sessionCancel(request)
      return { _tag: "Err", payload: { code, message: "Exact launch already ended" } }
    } }
    const view = await mount(client)
    await invoke(() => view.host().runAction(`cancel-pending:${first}`))
    await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
    expect(view.actions()).toEqual([{ id: "cancel-launch", label: "Cancel", enabled: true }])
    expect(targets).toEqual([first])
    expect(await implementation.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ session: { launchId: second } }] } })
  })
}

test("manual exact Cancel Pending re-observes daemon cancelling instead of declaring completion", async () => {
  const commit = gate()
  const client = createInMemoryKorridClient({ games: [game], sessionCommitGate: commit.promise })
  const first = await reserve(client)
  const work = client.sessionStart({ gameId: game.id, expectedLaunchId: first })
  await invoke(() => {})
  const second = await reserve(client)
  const cancels = spyOn(client, "sessionCancel")
  const starts = spyOn(client, "sessionStart")
  const view = await mount(client)
  expect(view.actions()[0]).toEqual({ id: `cancel-pending:${first}`, label: "Cancel Wario Land 4 (1)", description: "Starting", enabled: true })
  await invoke(() => view.button("Cancel Wario Land 4 (1)").click())
  await waitFor(() => view.actions()[0]?.description === "Cancelling")
  expect(view.actions()[0]).toEqual({ id: `cancel-pending:${first}`, label: "Cancel Wario Land 4 (1)", description: "Cancelling", enabled: false })
  expect(view.actions()[1]?.enabled).toBe(true)
  expect(cancels).toHaveBeenCalledTimes(1)
  expect(cancels).toHaveBeenCalledWith({ expectedLaunchId: first })
  await invoke(() => view.host().runAction(`cancel-pending:${second}`))
  await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
  expect(view.actions()[0]?.enabled).toBe(false)
  await invoke(commit.resolve)
  await work
  await waitFor(() => view.model().status._tag === "Browsing")
  expect(cancels.mock.calls.map(call => call[0]?.expectedLaunchId)).toEqual([first, second])
  expect(starts).not.toHaveBeenCalled()
  cancels.mockRestore(); starts.mockRestore()
})

for (const single of [false, true]) {
  test(`Pending cancellation recovers from HostRecoveryBlocked in ${single ? "surviving single" : "multiple-choice"} startup`, async () => {
    const commit = gate()
    const failure = { code: "HostRecoveryBlocked", message: "Exact cleanup failed" }
    let blocked = false
    const client = createInMemoryKorridClient({ games: [game], sessionCommitGate: commit.promise,
      sessionCleanupFailure: failure, sessionObservationFailure: () => blocked ? failure : undefined,
    })
    const first = await reserve(client)
    const work = client.sessionStart({ gameId: game.id, expectedLaunchId: first })
    await invoke(() => {})
    const second = await reserve(client)
    const starts = spyOn(client, "sessionStart")
    const reservations = spyOn(client, "sessionReserve")
    const cancels = spyOn(client, "sessionCancel")
    const view = await mount(client)
    await invoke(() => view.host().runAction(`cancel-pending:${first}`))
    await waitFor(() => view.actions()[0]?.description === "Cancelling")
    expect(view.actions()[0]?.enabled).toBe(false)
    if (single) {
      await invoke(() => view.host().runAction(`cancel-pending:${second}`))
      await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
      expect(view.actions()[0]?.enabled).toBe(false)
    }
    const retryId = single ? "cancel-launch" : `cancel-pending:${first}`
    blocked = true
    await invoke(() => view.host().reload())
    expect(view.model().status).toMatchObject({ _tag: "Busy", detail: failure.message })
    expect(view.actions()[0]?.enabled).toBe(true)
    if (single) expect(view.actions()[0]?.label).toBe("Retry Cancel")
    else {
      expect(view.actions()[0]?.description).toBe("Cancel failed. Try again.")
      expect(view.actions()[1]).toEqual({ id: `cancel-pending:${second}`, label: "Cancel Wario Land 4 (2)", description: "Waiting to start", enabled: true })
    }
    // The daemon may still report cancelling after an exact retry. That healthy
    // fact wins over the earlier failed observation without inventing a phase.
    blocked = false
    await invoke(() => { view.host().runAction(retryId); view.host().runAction(retryId) })
    const beforeCleanup = single ? [first, second, first] : [first, first]
    expect(cancels.mock.calls.map(call => call[0]?.expectedLaunchId)).toEqual(beforeCleanup)
    expect(view.actions()[0]?.enabled).toBe(false)
    if (!single) expect(view.actions()[0]?.description).toBe("Cancelling")
    blocked = true
    await invoke(commit.resolve)
    expect(await work).toEqual({ _tag: "Err", payload: failure })
    await invoke(() => view.host().reload())
    expect(view.actions()[0]?.enabled).toBe(true)
    expect(view.model().status).toMatchObject({ _tag: "Busy", detail: failure.message })
    blocked = false
    await invoke(() => view.host().runAction(retryId))
    if (single) await waitFor(() => view.model().status._tag === "Browsing")
    else {
      await waitFor(() => view.actions().length === 1 && view.actions()[0]?.id === "cancel-launch")
      expect(view.actions()).toEqual([{ id: "cancel-launch", label: "Cancel", enabled: true }])
      expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [{ session: { launchId: second } }] } })
    }
    expect(cancels.mock.calls.map(call => call[0]?.expectedLaunchId)).toEqual([...beforeCleanup, first])
    expect(starts).not.toHaveBeenCalled(); expect(reservations).not.toHaveBeenCalled()
    cancels.mockRestore(); starts.mockRestore(); reservations.mockRestore()
  })
}

test("fresh stopping session with waiting history remains Browsing and never offers startup Cancel", async () => {
  const client = createInMemoryKorridClient({ games: [game], activeSession: {
    launchId: "stopping:A", gameId: game.id, phase: "stopping", initialHandoff: InitialHandoff.Waiting,
  } })
  const starts = spyOn(client, "sessionStart")
  const reservations = spyOn(client, "sessionReserve")
  const cancels = spyOn(client, "sessionCancel")
  const view = await mount(client)
  expect(view.model().status).toEqual({ _tag: "Browsing" })
  expect(view.container.querySelector(".pico-launch-stage")).toBeNull()
  await invoke(() => view.host().runAction("cancel-launch"))
  expect(starts).not.toHaveBeenCalled(); expect(reservations).not.toHaveBeenCalled(); expect(cancels).not.toHaveBeenCalled()
  expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { active: { launchId: "stopping:A", phase: "stopping", initialHandoff: InitialHandoff.Waiting } } })
  starts.mockRestore(); reservations.mockRestore(); cancels.mockRestore()
})
