import { afterEach, describe, expect, it, mock, spyOn } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import {
  SecretSettingStatus,
  SessionFreezerState,
  SessionStopPhase,
  type ActiveSession,
  type Game,
  type SessionPrepareOutcome,
  type SessionStatusOutcome,
  type SessionStopOutcome,
} from "@contracts/generated/korrid"
import { createHttpKorridClient, type KorridClient } from "../korrid/client"
import type { PortalEntry } from "../launchables/state"
import { settingsFrom } from "./settings-model"
import { useLaunchables } from "./use-launchables"

;(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean })
  .IS_REACT_ACT_ENVIRONMENT = true

const roots: Root[] = []
afterEach(async () => {
  await act(async () => {
    for (const root of roots.splice(0)) root.unmount()
  })
  mock.restore()
})

function deferred<A>() {
  let resolve!: (value: A) => void
  const promise = new Promise<A>(complete => { resolve = complete })
  return { promise, resolve }
}

// Catalog/session shapes come from the generated Rust treaty, not Android's
// LocalGame/LaunchSpec. There is deliberately no native or identity fixture.
const game = {
  id: "neverball", title: "Neverball", host: "odin2portal",
  source: { label: "odin2portal", isLocal: true },
} satisfies Game
const active: ActiveSession = {
  launchId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  gameId: game.id,
  title: game.title,
  host: game.host,
}
const prepared: SessionPrepareOutcome = {
  _tag: "Ok", payload: { gameId: game.id, launchId: active.launchId },
}
const idle: SessionStatusOutcome = { _tag: "Ok", payload: {} }

function linuxClient(overrides: Partial<KorridClient> = {}): KorridClient {
  const unavailable = async (): Promise<never> => {
    throw new Error("Linux catalog must not call native/local-game discovery")
  }
  return {
    async health() { return { _tag: "Ok", payload: { version: "korrid-test" } } },
    async catalogSnapshot() { return { _tag: "Ok", payload: { games: [game] } } },
    async settingsSnapshot() {
      return {
        _tag: "Ok",
        payload: {
          revision: "r1", deviceName: "odin2portal", plugins: [],
          steamGridDbCredential: SecretSettingStatus.NotConfigured,
        },
      }
    },
    async sessionStatus() { return idle },
    async sessionPrepare() { return prepared },
    async sessionStop() { return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } } },
    updateSetting: unavailable,
    setSteamGridDbCredential: unavailable,
    clearSteamGridDbCredential: unavailable,
    discoverySnapshot: unavailable,
    registerDiscoveryReceipt: unavailable,
    removeDiscoveryLocation: unavailable,
    rescanDiscovery: unavailable,
    moonlightResolve: unavailable,
    moonlightLaunchPrepare: unavailable,
    moonlightLaunchCancel: unavailable,
    localGames: unavailable,
    localGameLaunch: unavailable,
    sessionFreeze: unavailable,
    sessionThaw: unavailable,
    sourceStatus: unavailable,
    sessionControls: unavailable,
    invokeSessionControl: unavailable,
    peerList: unavailable,
    ...overrides,
  }
}

async function mount(korrid: KorridClient) {
  let value: ReturnType<typeof useLaunchables> | undefined
  function Probe() {
    value = useLaunchables(undefined, korrid)
    return null
  }
  const root = createRoot(document.createElement("div"))
  roots.push(root)
  await act(async () => root.render(<Probe />))
  return {
    current() {
      if (!value) throw new Error("hook did not render")
      return value
    },
    async unmount() {
      roots.splice(roots.indexOf(root), 1)
      await act(async () => root.unmount())
    },
  }
}

function entry(hook: ReturnType<typeof useLaunchables>, kind: PortalEntry["kind"]) {
  if (hook.state._tag !== "Ready") throw new Error("expected Ready")
  const found = hook.state.entries.find(item => item.kind === kind)
  if (!found) throw new Error(`missing ${kind}`)
  return found
}

const invoke = async (action: () => void) => { await act(async () => action()) }

describe("Linux catalog execution without a native bridge", () => {
  it("loads only real catalog/device facts and omits Android settings and prompts", async () => {
    const harness = await mount(linuxClient())
    expect(harness.current().state).toEqual({
      _tag: "Ready", entries: [{ kind: "game", game }], notice: null,
    })
    expect(harness.current().facts).toEqual({
      version: "korrid-test",
      settings: {
        revision: "r1", deviceName: "odin2portal", plugins: [],
        steamGridDbCredential: SecretSettingStatus.NotConfigured,
      },
    })
    expect(settingsFrom(harness.current().facts, false).flatMap(group => group.items)
      .map(item => item.id)).toEqual([
      "device-name", "steamgriddb-credential", "korrid-version",
    ])
  })

  it.each([undefined, game.host])("prepares once per frame and refreshes the session (host: %s)", async host => {
    const pending = deferred<SessionPrepareOutcome>()
    const calls: unknown[] = []
    let status = idle
    const harness = await mount(linuxClient({
      async catalogSnapshot() {
        return { _tag: "Ok", payload: { games: [{ ...game, host }] } }
      },
      sessionPrepare(gameId, host) {
        calls.push([gameId, host])
        return pending.promise
      },
      async sessionStatus() { return status },
    }))
    const selected = entry(harness.current(), "game")
    await invoke(() => {
      harness.current().confirmEntry(selected)
      harness.current().confirmEntry(selected)
    })
    expect(harness.current().state._tag).toBe("Preparing")
    expect(calls).toEqual([[game.id, host]])
    status = { _tag: "Ok", payload: { active } }
    await invoke(() => pending.resolve(prepared))
    expect(entry(harness.current(), "now-playing")).toEqual({ kind: "now-playing", session: active })
  })

  it("reports prepare failure for the selected game without claiming launch", async () => {
    const harness = await mount(linuxClient({
      async sessionPrepare() {
        return { _tag: "Err", payload: { code: "HostLaunchFailed", message: "unit failed" } }
      },
    }))
    await invoke(() => harness.current().confirmEntry(entry(harness.current(), "game")))
    expect(harness.current().state).toMatchObject({
      _tag: "Ready", notice: { subject: { id: game.id, title: game.title }, message: "HostLaunchFailed: unit failed" },
    })
  })

  it("resumes exactly the displayed launch and keeps showing it", async () => {
    const thawed: string[] = []
    const harness = await mount(linuxClient({
      async sessionStatus() { return { _tag: "Ok", payload: { active } } },
      sessionPrepare: async () => { throw new Error("resume must not prepare") },
      async sessionThaw(expectedLaunchId) {
        thawed.push(expectedLaunchId)
        return {
          _tag: "Ok",
          payload: {
            launchId: active.launchId,
            state: SessionFreezerState.Running,
            changed: true,
          },
        }
      },
    }))
    await invoke(() => {
      harness.current().confirmEntry(entry(harness.current(), "now-playing"))
      harness.current().confirmEntry(entry(harness.current(), "now-playing"))
    })
    expect(thawed).toEqual([active.launchId])
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: null })
    expect(entry(harness.current(), "now-playing")).toEqual({ kind: "now-playing", session: active })
  })

  it("reports a refused resume without claiming the game returned", async () => {
    const harness = await mount(linuxClient({
      async sessionStatus() { return { _tag: "Ok", payload: { active } } },
      async sessionThaw() {
        return { _tag: "Err", payload: { code: "HostFocusFailed", message: "compositor refused focus" } }
      },
    }))
    await invoke(() => harness.current().confirmEntry(entry(harness.current(), "now-playing")))
    expect(harness.current().state).toMatchObject({
      _tag: "Ready",
      notice: { message: "HostFocusFailed: compositor refused focus", subject: { title: active.title } },
    })
    expect(entry(harness.current(), "now-playing")).toEqual({ kind: "now-playing", session: active })
  })

  it("refuses to resume a session that was replaced while the player chose", async () => {
    const harness = await mount(linuxClient({
      async sessionStatus() { return { _tag: "Ok", payload: { active } } },
      async sessionThaw() {
        return { _tag: "Err", payload: { code: "StaleLaunchIdentity", message: "old launch" } }
      },
    }))
    await invoke(() => harness.current().confirmEntry(entry(harness.current(), "now-playing")))
    expect(harness.current().state).toMatchObject({
      _tag: "Ready", notice: { message: "StaleLaunchIdentity: old launch" },
    })
  })

  it("stops exactly the displayed launch once, then confirms idle", async () => {
    const pending = deferred<SessionStopOutcome>()
    const calls: (string | undefined)[] = []
    let status: SessionStatusOutcome = { _tag: "Ok", payload: { active } }
    const harness = await mount(linuxClient({
      async sessionStatus() { return status },
      sessionStop(expectedLaunchId) {
        calls.push(expectedLaunchId)
        return pending.promise
      },
    }))
    const displayed = entry(harness.current(), "now-playing")
    await invoke(() => {
      harness.current().stopSession(displayed)
      harness.current().stopSession(displayed)
    })
    expect(calls).toEqual([active.launchId])
    expect(harness.current().state._tag).toBe("Stopping")
    status = idle
    await invoke(() => pending.resolve({ _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }))
    expect(harness.current().state).toEqual({
      _tag: "Ready", entries: [{ kind: "game", game }], notice: null,
    })
  })

  it("accepts Rust's explicit null active session on the HTTP wire", async () => {
    // browser_host_session_status_outcome serializes Option::None as null.
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      fetch() {
        return Response.json({
          _tag: "app.session.status", outcome: { _tag: "Ok", payload: { active: null } },
        })
      },
    })
    try {
      const http = createHttpKorridClient(server.url.origin, "capability")
      const harness = await mount(linuxClient({ sessionStatus: http.sessionStatus }))
      // Network completion is not a React effect; wait for the actual response.
      for (let attempt = 0; harness.current().state._tag === "Loading" && attempt < 100; attempt += 1) {
        await act(async () => { await new Promise(resolve => setTimeout(resolve, 1)) })
      }
      expect(harness.current().state).toEqual({
        _tag: "Ready", entries: [{ kind: "game", game }], notice: null,
      })
      await harness.unmount()
    } finally {
      server.stop(true)
    }
  })

  it("does not let an older status refresh replace the latest snapshot", async () => {
    const stale = deferred<SessionStatusOutcome>()
    let reads = 0
    const harness = await mount(linuxClient({
      sessionStatus() {
        reads += 1
        return reads === 2 ? stale.promise : Promise.resolve(idle)
      },
    }))
    await invoke(() => harness.current().reload())
    await invoke(() => harness.current().reload())
    await invoke(() => stale.resolve({ _tag: "Ok", payload: { active } }))
    expect(harness.current().state).toEqual({
      _tag: "Ready", entries: [{ kind: "game", game }], notice: null,
    })
  })

  it("ignores an older prepare response after reload and a newer launch", async () => {
    const first = deferred<SessionPrepareOutcome>()
    const second = deferred<SessionPrepareOutcome>()
    let calls = 0
    const harness = await mount(linuxClient({
      sessionPrepare() { return ++calls === 1 ? first.promise : second.promise },
    }))
    await invoke(() => harness.current().confirmEntry(entry(harness.current(), "game")))
    await invoke(() => harness.current().reload())
    await invoke(() => harness.current().confirmEntry(entry(harness.current(), "game")))
    await invoke(() => first.resolve(prepared))
    expect(harness.current().state._tag).toBe("Preparing")
    await invoke(() => second.resolve(prepared))
    expect(harness.current().state._tag).toBe("Ready")
  })

  it("does not refresh or publish a prepare response after unmount", async () => {
    const pending = deferred<SessionPrepareOutcome>()
    let reads = 0
    const harness = await mount(linuxClient({
      sessionPrepare() { return pending.promise },
      async sessionStatus() { reads += 1; return idle },
    }))
    await invoke(() => harness.current().confirmEntry(entry(harness.current(), "game")))
    await harness.unmount()
    await invoke(() => pending.resolve(prepared))
    expect(reads).toBe(1)
  })

  it("does not poll a stop response after unmount", async () => {
    const pending = deferred<SessionStopOutcome>()
    let reads = 0
    const harness = await mount(linuxClient({
      async sessionStatus() {
        reads += 1
        return { _tag: "Ok", payload: { active } }
      },
      sessionStop() { return pending.promise },
    }))
    await invoke(() => harness.current().stopSession(entry(harness.current(), "now-playing")))
    const readsBeforeUnmount = reads
    await harness.unmount()
    await invoke(() => pending.resolve({ _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }))
    expect(reads).toBe(readsBeforeUnmount)
  })

  it("ignores a late stop ACK after a reload observes a replacement session", async () => {
    const pending = deferred<SessionStopOutcome>()
    let status: SessionStatusOutcome = { _tag: "Ok", payload: { active } }
    const harness = await mount(linuxClient({
      async sessionStatus() { return status },
      sessionStop() { return pending.promise },
    }))
    await invoke(() => harness.current().stopSession(entry(harness.current(), "now-playing")))
    const replacement = { ...active, launchId: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" }
    status = { _tag: "Ok", payload: { active: replacement } }
    await invoke(() => harness.current().reload())
    await invoke(() => pending.resolve({ _tag: "Err", payload: { code: "StaleLaunchIdentity", message: "old launch" } }))
    expect(harness.current().state).toMatchObject({ _tag: "Ready", notice: null })
    expect(entry(harness.current(), "now-playing")).toEqual({ kind: "now-playing", session: replacement })
  })
})
