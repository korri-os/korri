import { afterEach, describe, expect, test } from "bun:test"
import { act, useEffect } from "react"
import { createRoot, type Root } from "react-dom/client"
import {
  SHELL_RESUMED_EVENT,
  STREAM_APPS_CHANGED_EVENT,
} from "@contracts/bridge/korri-native-bridge"
import {
  LaunchDisposition,
  LaunchForegroundKind,
  MoonlightImplementation,
} from "@contracts/generated/korrid"
import type {
  LocalLaunchSpec,
  QueryStreamAppsResult,
  QueryStreamHostsResult,
  StorageAccessResult,
  BackgroundNoticeResult,
  StreamHost,
} from "@contracts/bridge/korri-native-bridge"
import type {
  ActiveSession,
  CatalogSnapshotOutcome,
  Game,
  LocalGame,
  LocalGameLaunchOutcome,
  LocalGamesListOutcome,
  SessionPrepareOutcome,
  SessionStatusOutcome,
  SessionStopOutcome,
  SettingsSnapshotOutcome,
  SettingsUpdateOutcome,
} from "@contracts/generated/korrid"
import { SecretSettingStatus, SessionStopPhase } from "@contracts/generated/korrid"
import { createInMemoryLauncherBridge, type LauncherBridge } from "../bridge/launcher-bridge"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import { type Launchables, useLaunchables } from "./use-launchables"
import type { LaunchablesState, PortalEntry } from "../launchables/state"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true

const sleep = (ms = 0) => new Promise(resolve => setTimeout(resolve, ms))

class Deferred<T> {
  readonly promise: Promise<T>
  private resolveValue?: (value: T) => void

  constructor() {
    this.promise = new Promise<T>(resolve => {
      this.resolveValue = resolve
    })
  }

  resolve(value: T) {
    this.resolveValue?.(value)
  }
}

const localGame: LocalGame = {
  id: "wl4",
  title: "Wario Land 4",
  system: "Game Boy Advance",
}

const localLaunchSpec: LocalLaunchSpec = {
  launchId: "characterization-local",
  disposition: LaunchDisposition.Fresh,
  context: {
    gameId: "wl4",
    title: "Wario Land 4",
    contributors: [],
    foreground: { kind: LaunchForegroundKind.Component },
  },
  launcherId: "fixture-local",
  component: {
    packageName: "dev.fixture.runtime",
    className: "dev.fixture.runtime.MainActivity",
  },
  extras: { CONTENT: "/fixture/wl4.gba" },
  directories: [],
  files: [],
  integrity: "fixture-integrity",
}

const remoteGame = (host = "zao"): Game => ({
  id: "wl4",
  title: "Wario Land 4",
  host,
  source: { label: host, isLocal: false },
})

const activeSession = (overrides: Partial<ActiveSession> = {}): ActiveSession => ({
  launchId: "launch-1",
  title: "Wario Land 4",
  host: "zao",
  gameId: "wl4",
  ...overrides,
})

const streamHost = (name = "zao", uuid = `${name}-uuid`): StreamHost => ({
  uuid,
  name,
})

const streamApps = (id = 44): QueryStreamAppsResult => ({
  _tag: "StreamApps",
  items: [{ id, name: "Korri Stream" }],
})

const catalogOk = (games: readonly Game[] = []): CatalogSnapshotOutcome => ({
  _tag: "Ok",
  payload: { games: [...games] },
})

const localGamesOk = (
  games: readonly LocalGame[] = [],
): LocalGamesListOutcome => ({
  _tag: "Ok",
  payload: { games: [...games] },
})

const settingsOk = (revision = "settings-0"): SettingsSnapshotOutcome => ({
  _tag: "Ok",
  payload: {
    revision,
    deviceName: "Browser",
    plugins: [
      { id: "@korri:android-app", title: "Android", enabled: true },
      { id: "@korri:mgba", title: "mGBA", enabled: true },
      { id: "@korri:retroarch", title: "RetroArch", enabled: true },
    ],
    steamGridDbCredential: SecretSettingStatus.NotConfigured,
  },
})

function findEntry(state: LaunchablesState, kind: PortalEntry["kind"]): PortalEntry {
  if (state._tag === "Loading") throw new Error("expected loaded state")
  const entry = state.entries.find(candidate => candidate.kind === kind)
  if (entry === undefined) throw new Error(`expected ${kind} entry`)
  return entry
}

function findGameEntry(state: LaunchablesState, title = "Wario Land 4"): PortalEntry {
  if (state._tag === "Loading") throw new Error("expected loaded state")
  const entry = state.entries.find(
    candidate =>
      (candidate.kind === "game" || candidate.kind === "local-game") &&
      candidate.game.title === title,
  )
  if (entry === undefined) throw new Error(`expected ${title} entry`)
  return entry
}

function buildKorrid(
  overrides: Partial<KorridClient> = {},
  config: Parameters<typeof createInMemoryKorridClient>[0] = {},
): KorridClient {
  return { ...createInMemoryKorridClient(config), ...overrides }
}

function buildBridge(overrides: Partial<LauncherBridge> = {}): LauncherBridge {
  return {
    ...createInMemoryLauncherBridge({ streamHosts: [] }),
    ...overrides,
  }
}

async function waitFor(
  assertReady: () => void,
  label: string,
  attempts = 80,
) {
  let last: unknown
  for (let index = 0; index < attempts; index += 1) {
    try {
      assertReady()
      return
    } catch (error) {
      last = error
      await act(async () => {
        await sleep()
      })
    }
  }
  throw new Error(`${label}: ${last instanceof Error ? last.message : String(last)}`)
}

async function waitForWithin(
  assertReady: () => void,
  label: string,
  timeoutMs = 2_000,
) {
  let last: unknown
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    try {
      assertReady()
      return
    } catch (error) {
      last = error
      await act(async () => {
        await sleep(10)
      })
    }
  }
  throw new Error(`${label}: ${last instanceof Error ? last.message : String(last)}`)
}

async function waitForReady(view: HookView) {
  await waitFor(
    () => expect(view.current().state._tag).toBe("Ready"),
    "expected Ready state",
  )
}

interface HookView {
  current(): Launchables
  cleanup(): Promise<void>
}

/** Sealed stream startup requires a resolvable Moonlight transport. */
const availableMoonlight = {
  _tag: "Available" as const,
  payload: {
    transportId: "@korri:moonlight/moonlight",
    implementation: MoonlightImplementation.Artemis,
    sunshineApp: "Korri Stream",
  },
}

const mountedViews: HookView[] = []

afterEach(async () => {
  while (mountedViews.length > 0) {
    await mountedViews.pop()?.cleanup()
  }
})

async function renderUseLaunchables(
  bridge: LauncherBridge,
  korrid: KorridClient,
): Promise<HookView> {
  const container = document.createElement("div")
  document.body.append(container)
  const root: Root = createRoot(container)
  let latest: Launchables | undefined
  let disposed = false
  let view: HookView | undefined

  function Consumer() {
    const value = useLaunchables(bridge, korrid)
    useEffect(() => {
      latest = value
    })
    return null
  }

  const cleanup = async () => {
    if (disposed) return
    disposed = true
    if (view !== undefined) {
      const index = mountedViews.indexOf(view)
      if (index !== -1) mountedViews.splice(index, 1)
    }
    await act(async () => {
      root.unmount()
    })
    container.remove()
  }

  try {
    await act(async () => {
      root.render(<Consumer />)
    })

    view = {
      current() {
        if (latest === undefined) throw new Error("hook has not rendered")
        return latest
      },
      cleanup,
    }
    mountedViews.push(view)
    return view
  } catch (error) {
    await cleanup()
    throw error
  }
}

describe("useLaunchables load and core effects", () => {
  test("loads every source and queries apps for pinned and unpinned stream hosts", async () => {
    const hosts: QueryStreamHostsResult = {
      _tag: "StreamHosts",
      items: [streamHost("zao"), { uuid: "aka-uuid", name: "aka" }],
    }
    const queriedHosts: string[] = []
    const bridge = buildBridge({
      async queryStreamHosts() {
        return hosts
      },
      async queryStreamApps(hostUuid) {
        queriedHosts.push(hostUuid)
        return streamApps()
      },
      async storageAccess() {
        return { _tag: "Granted" }
      },
      async backgroundNotice() {
        return { _tag: "Visible" }
      },
    })
    const korrid = buildKorrid(
      {},
      {
        games: [remoteGame("zao")],
        localGames: [localGame],
        moonlight: availableMoonlight,
      },
    )

    const view = await renderUseLaunchables(bridge, korrid)

    expect(["Loading", "Ready"]).toContain(view.current().state._tag)
    await waitForReady(view)

    expect(queriedHosts).toEqual(["zao-uuid", "aka-uuid"])
    const current = view.current()
    expect(current.facts).toMatchObject({
      version: "korrid-in-memory",
      storage: { _tag: "Granted" },
      notice: { _tag: "Visible" },
      localGameCount: 1,
    })
    expect(current.facts.hosts?.map(host => host.name)).toEqual(["zao", "aka"])
    if (current.state._tag !== "Ready") throw new Error("expected Ready")
    expect(current.state.entries.map(entry => entry.kind)).toEqual([
      "local-game",
      "game",
      "background-notice",
    ])
  })

  test("launches a local game by obtaining the brain instruction before using the bridge", async () => {
    const calls: string[] = []
    const bridge = buildBridge({
      async launchLocal(spec) {
        calls.push(`bridge:${spec.launcherId}`)
        return { _tag: "Launched" }
      },
    })
    const base = createInMemoryKorridClient({
      games: [],
      localGames: [localGame],
      localLaunchSpecs: { wl4: localLaunchSpec },
    })
    const korrid = buildKorrid(
      {
        async localGameLaunch(gameId) {
          calls.push(`korrid:${gameId}`)
          return base.localGameLaunch(gameId)
        },
      },
      {
        games: [],
        localGames: [localGame],
        localLaunchSpecs: { wl4: localLaunchSpec },
      },
    )
    const view = await renderUseLaunchables(bridge, korrid)
    await waitForReady(view)

    await act(async () => {
      view.current().confirmEntry(findEntry(view.current().state, "local-game"))
    })
    await waitFor(
      () => expect(calls).toEqual(["korrid:wl4", "bridge:fixture-local"]),
      "expected local launch sequence",
    )
  })

  test("keeps the portal usable and skips the bridge when a local ROM is missing", async () => {
    let bridgeCalls = 0
    const bridge = buildBridge({
      async launchLocal() {
        bridgeCalls += 1
        return { _tag: "Launched" }
      },
    })
    const korrid = createInMemoryKorridClient({
      games: [],
      localGames: [localGame],
      localLaunchSpecs: {},
    })
    const view = await renderUseLaunchables(bridge, korrid)
    await waitForReady(view)

    await act(async () => {
      view.current().confirmEntry(findEntry(view.current().state, "local-game"))
    })
    await waitFor(
      () => expect(view.current().state).toMatchObject({ _tag: "Ready" }),
      "expected ready after missing local launch",
    )

    expect(bridgeCalls).toBe(0)
    expect(view.current().state).toMatchObject({
      _tag: "Ready",
      notice: { message: "LocalRomMissing: cannot launch wl4" },
    })
  })

  test("prepares a hosted game and attaches to the stream app on that same host", async () => {
    const calls: string[] = []
    const bridge = buildBridge({
      async queryStreamHosts() {
        return { _tag: "StreamHosts", items: [streamHost("zao")] }
      },
      async queryStreamApps(hostUuid) {
        return hostUuid === "zao-uuid" ? streamApps(9) : { _tag: "StreamApps", items: [] }
      },
      // Sealed startup hands the bridge one korrid-signed specification
      // instead of a host/app pair.
      async startStream(spec) {
        calls.push(`stream:${spec.hostUuid}:${spec.appId}`)
        return { _tag: "StreamStarted" }
      },
    })
    const base = createInMemoryKorridClient({
      games: [remoteGame("zao")],
      moonlight: availableMoonlight,
    })
    const korrid = buildKorrid(
      {
        async sessionPrepare(gameId, host) {
          calls.push(`prepare:${host}:${gameId}`)
          return base.sessionPrepare(gameId, host)
        },
      },
      { games: [remoteGame("zao")], moonlight: availableMoonlight },
    )
    const view = await renderUseLaunchables(bridge, korrid)
    await waitForReady(view)

    await act(async () => {
      view.current().confirmEntry(findGameEntry(view.current().state))
    })

    await waitFor(
      () => expect(calls).toEqual(["prepare:zao:wl4", "stream:zao-uuid:9"]),
      "expected remote launch sequence",
    )
  })

  test("background stream-app completion makes an empty first poll visible", async () => {
    let appsReady = false
    const calls: string[] = []
    const bridge = buildBridge({
      async queryStreamHosts() {
        return { _tag: "StreamHosts", items: [streamHost("zao")] }
      },
      async queryStreamApps() {
        return appsReady ? streamApps(9) : { _tag: "StreamApps", items: [] }
      },
      async startStream(spec) {
        calls.push(`stream:${spec.hostUuid}:${spec.appId}`)
        return { _tag: "StreamStarted" }
      },
    })
    const base = createInMemoryKorridClient({
      games: [remoteGame("zao")],
      moonlight: availableMoonlight,
    })
    const korrid = buildKorrid(
      {
        async sessionPrepare(gameId, host) {
          calls.push(`prepare:${host}:${gameId}`)
          return base.sessionPrepare(gameId, host)
        },
      },
      { games: [remoteGame("zao")], moonlight: availableMoonlight },
    )
    const view = await renderUseLaunchables(bridge, korrid)
    await waitForReady(view)

    await act(async () => {
      view.current().confirmEntry(findGameEntry(view.current().state))
    })
    await waitFor(
      () => expect(view.current().state).toMatchObject({
        _tag: "Ready",
        notice: { message: expect.stringContaining("NoStreamTarget") },
      }),
      "expected empty first stream poll",
    )

    appsReady = true
    await act(async () => {
      window.dispatchEvent(new Event(STREAM_APPS_CHANGED_EVENT))
    })
    await waitForReady(view)
    await act(async () => {
      view.current().confirmEntry(findGameEntry(view.current().state))
    })
    await waitFor(
      () => expect(calls).toEqual(["prepare:zao:wl4", "stream:zao-uuid:9"]),
      "expected background completion to publish route",
    )
  })

  test("does not prepare a hosted game when its exact stream host is absent", async () => {
    let prepareCalls = 0
    let streamCalls = 0
    const bridge = buildBridge({
      async queryStreamHosts() {
        return { _tag: "StreamHosts", items: [streamHost("aka")] }
      },
      async queryStreamApps() {
        return streamApps()
      },
      async startStream() {
        streamCalls += 1
        return { _tag: "StreamStarted" }
      },
    })
    const base = createInMemoryKorridClient({
      games: [remoteGame("zao")],
      moonlight: availableMoonlight,
    })
    const korrid = buildKorrid(
      {
        async sessionPrepare(gameId, host) {
          prepareCalls += 1
          return base.sessionPrepare(gameId, host)
        },
      },
      { games: [remoteGame("zao")], moonlight: availableMoonlight },
    )
    const view = await renderUseLaunchables(bridge, korrid)
    await waitForReady(view)

    await act(async () => {
      view.current().confirmEntry(findGameEntry(view.current().state))
    })

    await waitFor(
      () =>
        expect(view.current().state).toMatchObject({
          _tag: "Ready",
          notice: {
            message: 'NoStreamTarget: no "Korri Stream" app on provisioned host zao',
          },
        }),
      "expected no stream target notice",
    )
    expect(prepareCalls).toBe(0)
    expect(streamCalls).toBe(0)
  })

  test("requests a hidden background notice and opens settings only when Android will not prompt", async () => {
    let requests = 0
    let settings = 0
    const bridge = buildBridge({
      async backgroundNotice() {
        return { _tag: "Hidden" }
      },
      async requestBackgroundNotice() {
        requests += 1
        return { _tag: "Unprompted" }
      },
      async openNotificationSettings() {
        settings += 1
        return { _tag: "Opened" }
      },
    })
    const view = await renderUseLaunchables(bridge, createInMemoryKorridClient({ games: [] }))
    await waitForReady(view)

    await act(async () => {
      view.current().confirmEntry(findEntry(view.current().state, "background-notice"))
    })
    await waitFor(() => expect(settings).toBe(1), "expected notification settings")

    expect(requests).toBe(1)
  })

  test("opens notification settings directly when the background notice is visible", async () => {
    let requests = 0
    let settings = 0
    const bridge = buildBridge({
      async backgroundNotice() {
        return { _tag: "Visible" }
      },
      async requestBackgroundNotice() {
        requests += 1
        return { _tag: "Granted" }
      },
      async openNotificationSettings() {
        settings += 1
        return { _tag: "Opened" }
      },
    })
    const view = await renderUseLaunchables(bridge, createInMemoryKorridClient({ games: [] }))
    await waitForReady(view)

    await act(async () => {
      view.current().confirmEntry(findEntry(view.current().state, "background-notice"))
    })
    await waitFor(() => expect(settings).toBe(1), "expected notification settings")

    expect(requests).toBe(0)
  })

  test("publishes unavailable device action results as settings problems", async () => {
    const bridge = buildBridge({
      async openStorageAccessSettings() {
        return { _tag: "Unavailable", message: "no storage settings" }
      },
      async openNotificationSettings() {
        return { _tag: "Unavailable", message: "no notification settings" }
      },
      async requestBackgroundNotice() {
        return { _tag: "Unprompted" }
      },
    })
    const view = await renderUseLaunchables(bridge, createInMemoryKorridClient({ games: [] }))
    await waitForReady(view)

    await act(async () => {
      view.current().runDeviceAction("storage-access")
    })
    await waitFor(
      () =>
        expect(view.current().settingsStatus).toEqual({
          _tag: "Problem",
          settingId: "storage-access",
          message: "no storage settings",
        }),
      "expected storage settings problem",
    )

    await act(async () => {
      view.current().runDeviceAction("background-notice")
    })
    await waitFor(
      () =>
        expect(view.current().settingsStatus).toEqual({
          _tag: "Problem",
          settingId: "background-notice",
          message: "no notification settings",
        }),
      "expected notification settings problem",
    )
  })
})

describe("useLaunchables sequence guards", () => {
  test("only the newest overlapping load publishes state and facts", async () => {
    const first = new Deferred<CatalogSnapshotOutcome>()
    const second = new Deferred<CatalogSnapshotOutcome>()
    const catalogs = [first, second]
    const bridge = buildBridge()
    const korrid = buildKorrid({
      async catalogSnapshot() {
        const next = catalogs.shift()
        if (next === undefined) throw new Error("unexpected catalog read")
        return next.promise
      },
      async localGames() {
        return localGamesOk()
      },
    })
    const view = await renderUseLaunchables(bridge, korrid)
    await act(async () => {
      view.current().reload()
    })

    second.resolve(
      catalogOk([
        {
          id: "new",
          title: "New Game",
          source: { label: "zao", isLocal: false },
        },
      ]),
    )
    await waitFor(
      () => expect(findGameEntry(view.current().state, "New Game")).toBeDefined(),
      "expected second load",
    )

    first.resolve(
      catalogOk([
        {
          id: "old",
          title: "Old Game",
          source: { label: "zao", isLocal: false },
        },
      ]),
    )
    await act(async () => {
      await sleep()
    })

    expect(() => findGameEntry(view.current().state, "Old Game")).toThrow()
    expect(findGameEntry(view.current().state, "New Game")).toBeDefined()
  })

  test("unmount removes the shell resume listener", async () => {
    let catalogReads = 0
    const view = await renderUseLaunchables(
      buildBridge(),
      buildKorrid({
        async catalogSnapshot() {
          catalogReads += 1
          return catalogOk()
        },
      }),
    )
    await waitForReady(view)
    const readsBeforeUnmount = catalogReads

    await view.cleanup()
    await act(async () => {
      window.dispatchEvent(new Event(SHELL_RESUMED_EVENT))
      await sleep()
    })

    expect(catalogReads).toBe(readsBeforeUnmount)
  })

  test("shell resume rereads storage access and background notice", async () => {
    const storage: StorageAccessResult[] = [{ _tag: "Granted" }, { _tag: "Denied" }]
    const notices: BackgroundNoticeResult[] = [{ _tag: "Hidden" }, { _tag: "Visible" }]
    const bridge = buildBridge({
      async storageAccess() {
        return storage.shift() ?? { _tag: "Denied" }
      },
      async backgroundNotice() {
        return notices.shift() ?? { _tag: "Visible" }
      },
    })
    const view = await renderUseLaunchables(bridge, createInMemoryKorridClient({ games: [] }))
    await waitForReady(view)
    expect(view.current().facts.storage).toEqual({ _tag: "Granted" })
    expect(view.current().facts.notice).toEqual({ _tag: "Hidden" })

    await act(async () => {
      window.dispatchEvent(new Event(SHELL_RESUMED_EVENT))
    })

    await waitFor(
      () => {
        expect(view.current().facts.storage).toEqual({ _tag: "Denied" })
        expect(view.current().facts.notice).toEqual({ _tag: "Visible" })
      },
      "expected resume refresh",
    )
  })

  test("same-frame local confirmations issue one launch sequence", async () => {
    const launch = new Deferred<LocalGameLaunchOutcome>()
    let launchCalls = 0
    let bridgeCalls = 0
    const bridge = buildBridge({
      async launchLocal() {
        bridgeCalls += 1
        return { _tag: "Launched" }
      },
    })
    const korrid = buildKorrid(
      {
        async localGameLaunch() {
          launchCalls += 1
          return launch.promise
        },
      },
      { games: [], localGames: [localGame] },
    )
    const view = await renderUseLaunchables(bridge, korrid)
    await waitForReady(view)
    const entry = findEntry(view.current().state, "local-game")

    await act(async () => {
      view.current().confirmEntry(entry)
      view.current().confirmEntry(entry)
    })

    expect(launchCalls).toBe(1)
    launch.resolve({ _tag: "Ok", payload: localLaunchSpec })
    await waitFor(() => expect(bridgeCalls).toBe(1), "expected single bridge launch")
  })

  test("same-frame setting changes issue one write and publish the refreshed result", async () => {
    const update = new Deferred<SettingsUpdateOutcome>()
    const updates: [string, string, string][] = []
    let catalogReads = 0
    const initialSettings = settingsOk()
    if (initialSettings._tag !== "Ok") throw new Error("expected settings")
    let currentSettings = initialSettings.payload
    const korrid = buildKorrid({
      async catalogSnapshot() {
        catalogReads += 1
        return catalogOk()
      },
      async settingsSnapshot() {
        return { _tag: "Ok", payload: currentSettings }
      },
      async updateSetting(expectedRevision, settingId, value) {
        updates.push([expectedRevision, settingId, value])
        return update.promise
      },
    })
    const view = await renderUseLaunchables(buildBridge(), korrid)
    await waitForReady(view)
    const readsBeforeUpdate = catalogReads

    await act(async () => {
      view.current().changeSetting("device-name", "Retroid")
      view.current().changeSetting("device-name", "Ignored")
    })

    expect(updates).toEqual([["settings-0", "device-name", "Retroid"]])
    expect(view.current().settingsStatus).toEqual({
      _tag: "Saving",
      settingId: "device-name",
    })

    currentSettings = {
      revision: "settings-1",
      deviceName: "Retroid",
      plugins: [],
      steamGridDbCredential: SecretSettingStatus.NotConfigured,
    }
    update.resolve({ _tag: "Ok", payload: currentSettings })
    await waitFor(
      () => {
        expect(view.current().settingsStatus).toEqual({ _tag: "Idle" })
        expect(view.current().facts.settings).toEqual(currentSettings)
        expect(catalogReads).toBeGreaterThan(readsBeforeUpdate)
      },
      "expected published settings and launchability reload",
    )
  })

  test("a settings conflict publishes a problem, reloads, and can be dismissed", async () => {
    let catalogReads = 0
    const korrid = buildKorrid({
      async catalogSnapshot() {
        catalogReads += 1
        return catalogOk()
      },
      async updateSetting() {
        return {
          _tag: "Err",
          payload: { code: "SettingsConflict", message: "reload and try again" },
        }
      },
    })
    const view = await renderUseLaunchables(buildBridge(), korrid)
    await waitForReady(view)
    const readsBefore = catalogReads

    await act(async () => {
      view.current().changeSetting("device-name", "Retroid")
    })

    await waitFor(
      () =>
        expect(view.current().settingsStatus).toEqual({
          _tag: "Problem",
          settingId: "device-name",
          message: "reload and try again",
        }),
      "expected settings conflict problem",
    )
    await waitFor(
      () => expect(catalogReads).toBeGreaterThan(readsBefore),
      "expected conflict reload",
    )

    await act(async () => {
      view.current().dismissSettingsProblem()
    })
    expect(view.current().settingsStatus).toEqual({ _tag: "Idle" })
  })

  test("successful stop passes the exact launch identity and remains Stopping until status observes the session gone", async () => {
    let stopCalls = 0
    let stoppedLaunchId: string | undefined
    let statusCalls = 0
    const session = activeSession()
    const korrid = buildKorrid({
      async sessionStatus(): Promise<SessionStatusOutcome> {
        statusCalls += 1
        return statusCalls <= 2
          ? { _tag: "Ok", payload: { active: session } }
          : { _tag: "Ok", payload: {} }
      },
      async sessionStop(expectedLaunchId): Promise<SessionStopOutcome> {
        stopCalls += 1
        stoppedLaunchId = expectedLaunchId
        return { _tag: "Ok", payload: { phase: SessionStopPhase.Pending } }
      },
    })
    const view = await renderUseLaunchables(buildBridge(), korrid)
    await waitForReady(view)
    const entry = findEntry(view.current().state, "now-playing")

    await act(async () => {
      view.current().stopSession(entry)
      view.current().stopSession(entry)
    })

    expect(stopCalls).toBe(1)
    expect(stoppedLaunchId).toBe(session.launchId)
    await waitFor(
      () => expect(view.current().state._tag).toBe("Stopping"),
      "expected stopping publication",
    )
    await waitForWithin(
      () => expect(statusCalls).toBeGreaterThanOrEqual(3),
      "expected status polling to observe idle",
    )
    await waitForReady(view)
    const finalState = view.current().state
    if (finalState._tag !== "Ready") throw new Error("expected Ready")
    expect(finalState.entries.some(candidate => candidate.kind === "now-playing")).toBe(false)
  })

  test("non-timeout status failure after stop returns to ready with a notice", async () => {
    let statusCalls = 0
    const session = activeSession()
    const korrid = buildKorrid({
      async sessionStatus(): Promise<SessionStatusOutcome> {
        statusCalls += 1
        return statusCalls === 1
          ? { _tag: "Ok", payload: { active: session } }
          : {
              _tag: "Err",
              payload: { code: "HostUnavailable", message: "host offline" },
            }
      },
      async sessionStop() {
        return { _tag: "Ok", payload: { phase: SessionStopPhase.Pending } }
      },
    })
    const view = await renderUseLaunchables(buildBridge(), korrid)
    await waitForReady(view)

    await act(async () => {
      view.current().stopSession(findEntry(view.current().state, "now-playing"))
    })

    await waitFor(
      () =>
        expect(view.current().state).toMatchObject({
          _tag: "Ready",
          notice: { message: "HostUnavailable: host offline" },
        }),
      "expected status failure notice",
    )
  })
})
