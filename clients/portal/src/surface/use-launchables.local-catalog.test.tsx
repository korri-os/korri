import { afterEach, describe, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { ActiveSession, Game, SelectedGameLaunchOutcome, SessionStatusOutcome } from "@contracts/generated/korrid"
import { FocusOwnership, InitialHandoff, PendingLaunchPhase, SessionFreezerState, SessionStopPhase } from "@contracts/generated/korrid"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import type { PortalEntry } from "../launchables/state"
import { surfaceModelFrom } from "./surface-model"
import { useLaunchables, type Launchables } from "./use-launchables"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const roots: Root[] = []
afterEach(async () => {
  await act(async () => {
    for (const root of roots.splice(0)) root.unmount()
  })
})

const deferred = <A,>() => {
  let resolve!: (value: A) => void
  const promise = new Promise<A>(complete => { resolve = complete })
  return { promise, resolve }
}
const invoke = async (action: () => void) => { await act(async () => action()) }
const waitFor = async (predicate: () => boolean) => {
  for (let attempt = 0; attempt < 150; attempt += 1) {
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 10)) })
    if (predicate()) return
  }
  throw new Error("condition did not become true")
}

const game: Game = {
  id: "wl4",
  title: "Wario Land 4",
  host: "device-label",
  supportsRunnerSelection: false,
  source: { label: "device-label", isLocal: true },
}
// Linux's host_session_status_outcome omits host and title even though the
// catalog carries config.label as Game.host (services/korrid/src/lib.rs).
const active: ActiveSession = {
  launchId: "local-launch-1", gameId: game.id, phase: "running", focusOwnership: FocusOwnership.Launch, initialHandoff: InitialHandoff.Observed,
}
const prepared: SelectedGameLaunchOutcome = {
  _tag: "Ok", payload: { session: { gameId: game.id, launchId: active.launchId }, warnings: [] },
}
const idle: SessionStatusOutcome = {
  _tag: "Err", payload: { code: "NoActiveSession", message: "no host launch is active" },
}
const completed: SessionStatusOutcome = {
  _tag: "Err", payload: { code: "SessionCompleted", message: `host launch ${active.launchId} completed` },
}
const running: SessionStatusOutcome = { _tag: "Ok", payload: { active } }
const preparing: SessionStatusOutcome = { _tag: "Ok", payload: { pendingLaunches: [{
  session: { gameId: game.id, launchId: active.launchId }, phase: PendingLaunchPhase.Preparing,
}] } }

function fixture(overrides: Partial<KorridClient> = {}, initial = idle, catalog = [game]) {
  let status = initial
  const calls: {
    starts: Parameters<KorridClient["sessionStart"]>[]
    stops: Parameters<KorridClient["sessionStop"]>[]
    native: string[]
    statusReads: number
    catalogReads: number
    resumes: string[]
  } = {
    starts: [], stops: [], native: [], statusReads: 0, catalogReads: 0, resumes: [],
  }
  const base = createInMemoryKorridClient({ games: catalog })
  const korrid: KorridClient = {
    ...base,
    async catalogSnapshot() {
      calls.catalogReads += 1
      return base.catalogSnapshot()
    },
    async sessionReserve({ gameId }) { return { _tag: "Ok", payload: { gameId, launchId: active.launchId } } },
    async sessionCancel() { status = idle; return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } } },
    async sessionStart(...args) {
      calls.starts.push(args)
      status = running
      return prepared
    },
    /* Confirming the banner resumes the exact launch through korrid. It does
     * not reserve or start the game again, so it must not appear in calls.starts. */
    async sessionThaw(expectedLaunchId) {
      calls.resumes.push(expectedLaunchId)
      return { _tag: "Ok", payload: { launchId: expectedLaunchId, state: SessionFreezerState.Running, changed: true } }
    },
    async sessionStatus() { calls.statusReads += 1; return status },
    async sessionStop(...args) {
      calls.stops.push(args)
      status = completed
      return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }
    },
    ...overrides,
  }
  return { korrid, calls, setStatus(next: SessionStatusOutcome) { status = next } }
}

async function mount(config: ReturnType<typeof fixture>) {
  let value!: Launchables
  function Probe() { value = useLaunchables(config.korrid); return null }
  const root = createRoot(document.createElement("div"))
  roots.push(root)
  await act(async () => root.render(<Probe />))
  await waitFor(() => value.state._tag === "Ready")
  return {
    current: () => value,
    entry(kind: PortalEntry["kind"]): PortalEntry {
      if (value.state._tag === "Loading") throw new Error("not loaded")
      const entry = value.state.entries.find(candidate => candidate.kind === kind)
      if (!entry) throw new Error(`missing ${kind}`)
      return entry
    },
    async unmount() {
      roots.splice(roots.indexOf(root), 1)
      await act(async () => root.unmount())
    },
  }
}
const hasSession = (value: Launchables) => value.state._tag !== "Loading" && value.state.entries.some(entry => entry.kind === "now-playing")

describe("source-local catalog orchestration", () => {

  test("starts once without a host or native launch, then exposes the exact active session", async () => {
    const config = fixture()
    const harness = await mount(config)
    const entry = harness.entry("game")
    await invoke(() => {
      harness.current().confirmEntry(entry)
      harness.current().confirmEntry(entry)
    })
    await waitFor(() => harness.current().state._tag === "Ready" && hasSession(harness.current()))
    expect(config.calls.starts).toEqual([[{ gameId: game.id, expectedLaunchId: active.launchId }]])
    expect(config.calls.native).toEqual([])
    expect(harness.entry("now-playing")).toEqual({ kind: "now-playing", session: active })
    const model = surfaceModelFrom(harness.current().state)
    expect(model.catalog).toMatchObject({ _tag: "Ready", games: expect.arrayContaining([
      expect.objectContaining({ id: `now-playing:${active.launchId}`, subtitle: "This device" }),
    ]) })
    // Neither the catalog entry nor the banner may restart or natively resume it.
    await invoke(() => harness.current().confirmEntry(entry))
    await invoke(() => harness.current().confirmEntry(harness.entry("now-playing")))
    expect(config.calls.starts).toHaveLength(1)
    expect(config.calls.native).toEqual([])
    expect(surfaceModelFrom(harness.current().state).status._tag).toBe("Browsing")
  })

  test("same-id peer sessions neither become local banners nor block the local copy", async () => {
    const peer = { ...game, host: "peer", source: { label: "peer", isLocal: false } }
    const config = fixture({}, { _tag: "Ok", payload: { active: { ...active, host: "peer" } } }, [peer, game])
    const harness = await mount(config)
    await invoke(() => harness.current().confirmEntry(harness.entry("now-playing")))
    const state = harness.current().state
    if (state._tag !== "Ready") throw new Error("not ready")
    const local = state.entries.find(entry => entry.kind === "game" && entry.game.source.isLocal)
    if (!local) throw new Error("missing local copy")
    await invoke(() => harness.current().confirmEntry(local))
    await waitFor(() => config.calls.statusReads > 1)
    expect(config.calls.starts).toEqual([[{ gameId: game.id, expectedLaunchId: active.launchId }]])
    expect(harness.entry("now-playing")).toEqual({ kind: "now-playing", session: active })
    await invoke(() => harness.current().confirmEntry(local))
    expect(config.calls.starts).toHaveLength(1)
    expect(config.calls.native).toEqual([])
  })

  test("reports startup failure against its game and permits a later retry", async () => {
    let attempts = 0
    const config = fixture({ async sessionStart() {
      attempts += 1
      return { _tag: "Err", payload: { code: "LocalRomMissing", message: "ROM was removed" } }
    } })
    const harness = await mount(config)
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    expect(harness.current().state).toMatchObject({
      _tag: "Ready", notice: { message: "LocalRomMissing: ROM was removed", subject: { id: game.id, title: game.title } },
    })
    await invoke(() => harness.current().dismissNotice())
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    expect(attempts).toBe(2)
    expect(config.calls.native).toEqual([])
  })

  test.each(["SessionCompleted", "NoActiveSession"])("exact stop reconciles %s without the ended banner", async code => {
    const ended = code === "SessionCompleted" ? completed : idle
    const config = fixture({ async sessionStop(...args) {
      config.calls.stops.push(args)
      config.setStatus(ended)
      return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }
    } }, running)
    const harness = await mount(config)
    const entry = harness.entry("now-playing")
    await invoke(() => { harness.current().stopSession(entry); harness.current().stopSession(entry) })
    await waitFor(() => harness.current().state._tag === "Ready" && !hasSession(harness.current()))
    expect(config.calls.stops).toEqual([[active.launchId]])
    expect(harness.current().state).toMatchObject({ notice: null })
    expect(config.calls.catalogReads).toBeGreaterThan(1)
    expect(surfaceModelFrom(harness.current().state).status._tag).toBe("Browsing")
  })

  test("a reload observing NoActiveSession resolves a pending stop and ignores its late ACK", async () => {
    const stop = deferred<Awaited<ReturnType<KorridClient["sessionStop"]>>>()
    const config = fixture({ sessionStop: () => stop.promise }, running)
    const harness = await mount(config)
    await invoke(() => harness.current().stopSession(harness.entry("now-playing")))
    expect(harness.current().state._tag).toBe("Stopping")
    config.setStatus(idle)
    await invoke(() => harness.current().reload())
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: null })
    expect(hasSession(harness.current())).toBe(false)
    await invoke(() => stop.resolve({ _tag: "Err", payload: { code: "StopFailed", message: "old stop" } }))
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: null })
    expect(hasSession(harness.current())).toBe(false)
  })

  test("retains the session and reports an exact-stop failure", async () => {
    const config = fixture({ async sessionStop() {
      return { _tag: "Err", payload: { code: "StopFailed", message: "unit did not stop" } }
    } }, running)
    const harness = await mount(config)
    await invoke(() => harness.current().stopSession(harness.entry("now-playing")))
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: { message: "StopFailed: unit did not stop" } })
    expect(hasSession(harness.current())).toBe(true)
  })

  test.each(["SessionCompleted", "NoActiveSession"])("reconciles natural exit (%s) without blur and refreshes the catalog", async code => {
    const ended = code === "SessionCompleted" ? completed : idle
    const config = fixture()
    const harness = await mount(config)
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    await waitFor(() => harness.current().state._tag === "Ready" && hasSession(harness.current()))
    const reads = config.calls.catalogReads
    config.setStatus(ended)
    await waitFor(() => harness.current().state._tag === "Ready" && !hasSession(harness.current()))
    expect(config.calls.catalogReads).toBeGreaterThan(reads)
    expect(surfaceModelFrom(harness.current().state).status._tag).toBe("Browsing")
  })

  test("handles a game that exits before start returns without any blur", async () => {
    const config = fixture({ async sessionStart() { config.setStatus(completed); return prepared } })
    const harness = await mount(config)
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    await waitFor(() => harness.current().state._tag === "Ready" && !hasSession(harness.current()))
    expect(config.calls.statusReads).toBeGreaterThan(1)
    expect(surfaceModelFrom(harness.current().state).status._tag).toBe("Browsing")
  })

  for (const event of ["focus", "visibilitychange"]) {
    test(`${event} refreshes local session and catalog without cancelling pending start`, async () => {
      const preparation = deferred<SelectedGameLaunchOutcome>()
      const config = fixture({ sessionStart: () => { config.setStatus(preparing); return preparation.promise } })
      const harness = await mount(config)
      await invoke(() => harness.current().confirmEntry(harness.entry("game")))
      expect(harness.current().state._tag).toBe("Starting")
      await invoke(() => (event === "focus" ? window : document).dispatchEvent(new Event(event)))
      expect(harness.current().state._tag).toBe("Starting")
      config.setStatus(running)
      await invoke(() => preparation.resolve(prepared))
      await waitFor(() => harness.current().state._tag === "Ready" && hasSession(harness.current()))
      const reads = config.calls.catalogReads
      config.setStatus(completed)
      await invoke(() => (event === "focus" ? window : document).dispatchEvent(new Event(event)))
      await waitFor(() => harness.current().state._tag === "Ready" && !hasSession(harness.current()))
      expect(config.calls.catalogReads).toBeGreaterThan(reads)
    })
  }

  test("a status failure keeps the known exact launch and retries rather than relaunching", async () => {
    const config = fixture()
    const harness = await mount(config)
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    await waitFor(() => harness.current().state._tag === "Ready" && hasSession(harness.current()))
    config.setStatus({ _tag: "Err", payload: { code: "StatusTimeout", message: "timed out" } })
    const reads = config.calls.statusReads
    await waitFor(() => config.calls.statusReads > reads)
    expect(hasSession(harness.current())).toBe(true)
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    expect(config.calls.starts).toHaveLength(1)
    config.setStatus(completed)
    await waitFor(() => !hasSession(harness.current()))
  })

  test("a failed return refresh retains known locality and cannot turn the session into a stream", async () => {
    let failCatalog = false
    const config = fixture({ async catalogSnapshot() {
      return failCatalog
        ? { _tag: "Err", payload: { code: "BrainUnreachable", message: "disconnected" } }
        : { _tag: "Ok", payload: { games: [game] } }
    } }, running)
    const harness = await mount(config)
    failCatalog = true
    config.setStatus({ _tag: "Err", payload: { code: "BrainUnreachable", message: "disconnected" } })
    await invoke(() => window.dispatchEvent(new Event("focus")))
    expect(hasSession(harness.current())).toBe(true)
    const banner = harness.entry("now-playing")
    await invoke(() => harness.current().confirmEntry(banner))
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: { _tag: "Catalog", message: "games: BrainUnreachable: disconnected" } })
    expect(config.calls.native).toEqual([])
    const reads = config.calls.statusReads
    config.setStatus(completed)
    await waitFor(() => config.calls.statusReads > reads && !hasSession(harness.current()))
  })

  for (const observed of [running, { _tag: "Err", payload: { code: "BrainUnreachable", message: "disconnected" } } satisfies SessionStatusOutcome]) {
    test(`full reload retains only session locality when catalog fails and status is ${observed._tag}`, async () => {
      let catalogReads = 0
      const catalog = deferred<Awaited<ReturnType<KorridClient["catalogSnapshot"]>>>()
      const config = fixture({ catalogSnapshot() {
        catalogReads += 1
        return catalogReads === 1
          ? Promise.resolve({ _tag: "Ok", payload: { games: [game] } })
          : catalog.promise
      } }, running)
      const harness = await mount(config)
      const staleGame = harness.entry("game")
      config.setStatus(observed)
      await invoke(() => harness.current().reload())
      expect(harness.current().state._tag).toBe("Loading")
      // A second full reload must not erase the evidence held through Loading.
      await invoke(() => harness.current().reload())
      await invoke(() => catalog.resolve({ _tag: "Err", payload: { code: "BrainUnreachable", message: "disconnected" } }))
      await waitFor(() => harness.current().state._tag === "Ready")
      expect(hasSession(harness.current())).toBe(true)
      expect(surfaceModelFrom(harness.current().state).catalog).toMatchObject({ _tag: "Ready", games: expect.arrayContaining([
        expect.objectContaining({ id: `now-playing:${active.launchId}`, subtitle: "This device" }),
      ]) })
      const state = harness.current().state
      if (state._tag !== "Ready") throw new Error("not ready")
      expect(state.entries.some(entry => entry.kind === "game")).toBe(false)
      await invoke(() => harness.current().confirmEntry(harness.entry("now-playing")))
      await invoke(() => harness.current().confirmEntry(staleGame))
      expect(config.calls.starts).toEqual([])
      expect(config.calls.native).toEqual([])
      expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: { _tag: "Catalog", message: "games: BrainUnreachable: disconnected" } })
      const reads = catalogReads
      await invoke(() => window.dispatchEvent(new Event("focus")))
      expect(catalogReads).toBeGreaterThan(reads)
      config.setStatus(idle)
      await waitFor(() => !hasSession(harness.current()))
      await invoke(() => harness.current().confirmEntry(staleGame))
      expect(config.calls.starts).toEqual([])
    })
  }

  test("a completed return refresh clears the banner even when the catalog read fails", async () => {
    let failCatalog = false
    const config = fixture({ async catalogSnapshot() {
      return failCatalog
        ? { _tag: "Err", payload: { code: "BrainUnreachable", message: "disconnected" } }
        : { _tag: "Ok", payload: { games: [game] } }
    } }, running)
    const harness = await mount(config)
    failCatalog = true
    config.setStatus(completed)
    await invoke(() => window.dispatchEvent(new Event("focus")))
    expect(hasSession(harness.current())).toBe(false)
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: { _tag: "Catalog", message: "games: BrainUnreachable: disconnected" } })
  })

  test("ignores a cancelled start failure after reload and a newer start", async () => {
    const first = deferred<SelectedGameLaunchOutcome>()
    const second = deferred<SelectedGameLaunchOutcome>()
    let attempts = 0
    const config = fixture({ sessionStart: () => { config.setStatus(preparing); return ++attempts === 1 ? first.promise : second.promise } })
    const harness = await mount(config)
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    await invoke(() => harness.current().cancelLaunch())
    await invoke(() => harness.current().reload())
    await waitFor(() => harness.current().state._tag === "Ready")
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    await invoke(() => first.resolve({ _tag: "Err", payload: { code: "OldFailure", message: "old operation" } }))
    expect(harness.current().state._tag).toBe("Starting")
    config.setStatus(running)
    await invoke(() => second.resolve(prepared))
    await waitFor(() => harness.current().state._tag === "Ready" && hasSession(harness.current()))
    expect(harness.current().state).toMatchObject({ notice: null })
  })

  test("ignores old status completion after a replacement launch is loaded", async () => {
    const oldStatus = deferred<SessionStatusOutcome>()
    let reads = 0
    const replacement = { ...active, launchId: "local-launch-2" }
    const config = fixture({ sessionStatus() {
      reads += 1
      if (reads === 1) return Promise.resolve(running)
      if (reads === 2) return oldStatus.promise
      return Promise.resolve({ _tag: "Ok", payload: { active: replacement } })
    } }, running)
    const harness = await mount(config)
    await waitFor(() => reads === 2)
    await invoke(() => harness.current().reload())
    await waitFor(() => harness.current().state._tag === "Ready")
    await invoke(() => oldStatus.resolve(completed))
    expect(harness.entry("now-playing")).toMatchObject({ session: { launchId: replacement.launchId } })
  })

  test("a pre-ACK focus snapshot cannot erase an acknowledged launch", async () => {
    const preparation = deferred<SelectedGameLaunchOutcome>()
    const beforeAck = deferred<SessionStatusOutcome>()
    let reads = 0
    const config = fixture({
      sessionStart: () => preparation.promise,
      sessionStatus() {
        reads += 1
        return reads === 1 ? Promise.resolve(idle) : reads === 2 ? beforeAck.promise : Promise.resolve(running)
      },
    })
    const harness = await mount(config)
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    await invoke(() => window.dispatchEvent(new Event("focus")))
    await waitFor(() => reads >= 2)
    await invoke(() => preparation.resolve(prepared))
    await invoke(() => beforeAck.resolve(idle))
    await waitFor(() => harness.current().state._tag === "Ready" && hasSession(harness.current()))
    expect(harness.entry("now-playing")).toMatchObject({ session: active })
  })

  test("recovers observed session truth after the newer start conflicts, preserving its notice and polling", async () => {
    const first = deferred<SelectedGameLaunchOutcome>()
    const second = deferred<SelectedGameLaunchOutcome>()
    const otherGame = { ...game, id: "other-gba", title: "Other GBA game" }
    const competing = { ...active, launchId: "external-launch" }
    const otherIdentity = { gameId: otherGame.id, launchId: "local-launch-2" }
    const config = fixture({
      async sessionReserve({ gameId }) { return { _tag: "Ok", payload: gameId === otherGame.id ? otherIdentity : { gameId, launchId: active.launchId } } },
      sessionStart(...args) {
        config.calls.starts.push(args)
        config.setStatus({ _tag: "Ok", payload: { pendingLaunches: [{
          session: config.calls.starts.length === 1 ? { gameId: game.id, launchId: active.launchId } : otherIdentity,
          phase: PendingLaunchPhase.Preparing,
        }] } })
        return config.calls.starts.length === 1 ? first.promise : second.promise
      },
    }, idle, [game, otherGame])
    const harness = await mount(config)
    const firstEntry = harness.entry("game")
    await invoke(() => harness.current().confirmEntry(firstEntry))
    await invoke(() => harness.current().cancelLaunch())
    await invoke(() => harness.current().reload())
    await waitFor(() => harness.current().state._tag === "Ready")
    const state = harness.current().state
    if (state._tag !== "Ready") throw new Error("not ready")
    const secondEntry = state.entries.find(entry => entry.kind === "game" && entry.game.id === otherGame.id)
    if (!secondEntry) throw new Error("missing second game")
    await invoke(() => harness.current().confirmEntry(secondEntry))
    // A retired reply is not authority. A genuinely newer external live session
    // can coexist with B's pending route, before B returns its conflict.
    config.setStatus({ _tag: "Ok", payload: { active: competing, pendingLaunches: [{ session: otherIdentity, phase: PendingLaunchPhase.Preparing }] } })
    await invoke(() => first.resolve(prepared))
    expect(config.calls.catalogReads).toBeGreaterThanOrEqual(3)
    expect(harness.current().state._tag).toBe("Starting")
    expect(hasSession(harness.current())).toBe(true)
    await invoke(() => harness.current().confirmEntry(secondEntry))
    config.setStatus({ _tag: "Ok", payload: { active: competing } })
    await invoke(() => second.resolve({ _tag: "Err", payload: {
      code: "ActiveSessionConflict", message: "one host game is already running or stopping",
    } }))
    await waitFor(() => harness.current().state._tag === "Ready" && hasSession(harness.current()))
    expect(harness.current().state).toMatchObject({ notice: {
      message: "ActiveSessionConflict: one host game is already running or stopping",
      subject: { id: otherGame.id, title: otherGame.title },
    } })
    expect(harness.entry("now-playing")).toEqual({ kind: "now-playing", session: competing })
    await invoke(() => harness.current().confirmEntry(firstEntry))
    await invoke(() => harness.current().confirmEntry(harness.entry("now-playing")))
    expect(config.calls.starts).toEqual([[{ gameId: game.id, expectedLaunchId: active.launchId }], [{ gameId: otherGame.id, expectedLaunchId: otherIdentity.launchId }]])
    expect(config.calls.native).toEqual([])
    config.setStatus(completed)
    await waitFor(() => !hasSession(harness.current()))
    expect(config.calls.catalogReads).toBeGreaterThan(3)
  })

  test("a late startup-failure recovery read cannot unlock a newer command", async () => {
    const recovery = deferred<SessionStatusOutcome>()
    const preparation = deferred<SelectedGameLaunchOutcome>()
    const config = fixture({
      sessionStart(...args) {
        config.calls.starts.push(args)
        return config.calls.starts.length === 1
          ? Promise.resolve({ _tag: "Err", payload: { code: "ActiveSessionConflict", message: "already running" } })
          : preparation.promise
      },
      sessionStatus() {
        config.calls.statusReads += 1
        return config.calls.statusReads === 1 ? Promise.resolve(idle) : config.calls.statusReads === 2 ? recovery.promise : Promise.resolve(preparing)
      },
    })
    const harness = await mount(config)
    const entry = harness.entry("game")
    await invoke(() => harness.current().confirmEntry(entry))
    expect(config.calls.statusReads).toBeGreaterThanOrEqual(2)
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: { message: "ActiveSessionConflict: already running" } })
    await invoke(() => harness.current().confirmEntry(entry))
    expect(harness.current().state._tag).toBe("Starting")
    await invoke(() => recovery.resolve(running))
    expect(harness.current().state._tag).toBe("Starting")
    expect(hasSession(harness.current())).toBe(true)
    await invoke(() => harness.current().confirmEntry(entry))
    expect(config.calls.starts).toEqual([[{ gameId: game.id, expectedLaunchId: active.launchId }], [{ gameId: game.id, expectedLaunchId: active.launchId }]])
    expect(config.calls.native).toEqual([])
  })

  test("an old status read cannot finish a newer exact stop", async () => {
    const oldRead = deferred<SessionStatusOutcome>()
    const stopping = deferred<Awaited<ReturnType<KorridClient["sessionStop"]>>>()
    let reads = 0
    const config = fixture({
      sessionStop: () => stopping.promise,
      sessionStatus() {
        reads += 1
        return reads === 2 ? oldRead.promise : Promise.resolve(running)
      },
    }, running)
    const harness = await mount(config)
    await waitFor(() => reads === 2)
    await invoke(() => harness.current().stopSession(harness.entry("now-playing")))
    await invoke(() => oldRead.resolve(completed))
    expect(harness.current().state._tag).toBe("Stopping")
    await invoke(() => stopping.resolve({ _tag: "Err", payload: { code: "StopFailed", message: "still active" } }))
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: { message: "StopFailed: still active" } })
  })

  test("unmount retires an outstanding status read and the return listeners", async () => {
    const pending = deferred<SessionStatusOutcome>()
    let reads = 0
    const config = fixture({ sessionStatus() {
      reads += 1
      return reads === 1 ? Promise.resolve(running) : pending.promise
    } }, running)
    const harness = await mount(config)
    await waitFor(() => reads === 2)
    await harness.unmount()
    const catalogReads = config.calls.catalogReads
    await invoke(() => {
      pending.resolve(idle)
      window.dispatchEvent(new Event("focus"))
      document.dispatchEvent(new Event("visibilitychange"))
    })
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 550)) })
    expect(reads).toBe(2)
    expect(config.calls.catalogReads).toBe(catalogReads)
  })

  test("does not start reconciliation after an unmounted start completes", async () => {
    const preparation = deferred<SelectedGameLaunchOutcome>()
    const config = fixture({ sessionStart: () => preparation.promise })
    const harness = await mount(config)
    await invoke(() => harness.current().confirmEntry(harness.entry("game")))
    await harness.unmount()
    const reads = config.calls.statusReads
    await invoke(() => preparation.resolve(prepared))
    expect(config.calls.statusReads).toBe(reads)
    expect(config.calls.native).toEqual([])
  })
})
