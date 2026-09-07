import { afterEach, describe, expect, it } from "bun:test"
import { act, type ReactNode } from "react"
import { createRoot, type Root } from "react-dom/client"
import {
  LaunchContributorKind,
  LaunchForegroundKind,
  MoonlightImplementation,
} from "@contracts/generated/korrid"
import {
  createInMemoryLauncherBridge,
  type LauncherBridge,
} from "../bridge/launcher-bridge"
import {
  createInMemoryKorridClient,
  type KorridClient,
} from "../korrid/client"
import type { PortalEntry } from "../launchables/state"
import { useLaunchables } from "./use-launchables"

;(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean })
  .IS_REACT_ACT_ENVIRONMENT = true

const roots: Root[] = []
afterEach(async () => {
  await act(async () => {
    for (const root of roots.splice(0)) root.unmount()
  })
})

interface Deferred<A> {
  readonly promise: Promise<A>
  resolve(value: A): void
}

const deferred = <A,>(): Deferred<A> => {
  let resolve!: (value: A) => void
  return {
    promise: new Promise<A>(complete => {
      resolve = complete
    }),
    resolve,
  }
}

const available = {
  _tag: "Available" as const,
  payload: {
    transportId: "@korri:moonlight/moonlight",
    implementation: MoonlightImplementation.Artemis,
    sunshineApp: "Korri Stream",
  },
}

const launchSpec = (launchId: string) => ({
  launchId,
  transportId: available.payload.transportId,
  context: {
    gameId: "game",
    title: "Game",
    contributors: [
      {
        kind: LaunchContributorKind.Transport,
        id: available.payload.transportId,
      },
    ],
    executor: { id: "android-moonlight", available: false },
    foreground: { kind: LaunchForegroundKind.ArtemisGame },
  },
  implementation: available.payload.implementation,
  sunshineApp: available.payload.sunshineApp,
  hostUuid: "host-uuid",
  appId: 7,
  integrity: `signed:${launchId}`,
})

const invoke = async (action: () => void) => {
  await act(async () => action())
}

const waitFor = async (predicate: () => boolean) => {
  for (let attempt = 0; attempt < 40; attempt += 1) {
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 0))
    })
    if (predicate()) return
  }
  throw new Error("condition did not become true")
}

interface Harness {
  current(): ReturnType<typeof useLaunchables>
  unmount(): Promise<void>
}

const mountLaunchables = async (
  korrid: KorridClient,
  bridge: LauncherBridge,
): Promise<Harness> => {
  let value: ReturnType<typeof useLaunchables> | undefined
  const Probe = (): ReactNode => {
    value = useLaunchables(bridge, korrid)
    return null
  }
  const container = document.createElement("div")
  const root = createRoot(container)
  roots.push(root)
  await act(async () => root.render(<Probe />))
  await waitFor(() => value?.state._tag === "Ready")
  return {
    current() {
      if (value === undefined) throw new Error("hook has not rendered")
      return value
    },
    async unmount() {
      const index = roots.indexOf(root)
      if (index >= 0) roots.splice(index, 1)
      await act(async () => root.unmount())
    },
  }
}

const remoteGame = (launchables: ReturnType<typeof useLaunchables>): PortalEntry => {
  if (launchables.state._tag !== "Ready") throw new Error("expected Ready")
  const entry = launchables.state.entries.find(candidate => candidate.kind === "game")
  if (entry === undefined) throw new Error("expected remote game")
  return entry
}

const nowPlaying = (launchables: ReturnType<typeof useLaunchables>): PortalEntry => {
  if (launchables.state._tag !== "Ready") throw new Error("expected Ready")
  const entry = launchables.state.entries.find(
    candidate => candidate.kind === "now-playing",
  )
  if (entry === undefined) throw new Error("expected now playing")
  return entry
}

const fixture = (
  korridOverrides: Partial<KorridClient> = {},
  bridgeOverrides: Partial<LauncherBridge> = {},
) => {
  const korrid = {
    ...createInMemoryKorridClient({
      games: [{ id: "game", title: "Game", host: "Host", source: { label: "Host", isLocal: false } }],
      moonlight: available,
    }),
    ...korridOverrides,
  }
  const bridge = {
    ...createInMemoryLauncherBridge({
      streamHosts: [{ uuid: "host-uuid", name: "Host" }],
      streamApps: { "host-uuid": [{ id: 7, name: "Korri Stream" }] },
    }),
    ...bridgeOverrides,
  }
  return { korrid, bridge }
}

describe("Moonlight launch orchestration", () => {
  it("cancels a reservation returned after the operation was superseded without preparing or starting", async () => {
    const signing = deferred<Awaited<ReturnType<KorridClient["moonlightLaunchPrepare"]>>>()
    const calls: string[] = []
    const { korrid, bridge } = fixture({
      moonlightLaunchPrepare() {
        calls.push("sign")
        return signing.promise
      },
      async moonlightLaunchCancel(launchId) {
        calls.push(`cancel:${launchId}`)
        return { _tag: "Ok", payload: { launchId } }
      },
      async sessionPrepare() {
        calls.push("host-prepare")
        return { _tag: "Ok", payload: { gameId: "game", launchId: "host" } }
      },
    }, {
      async startStream() {
        calls.push("native-start")
        return { _tag: "StreamStarted" }
      },
    })
    const harness = await mountLaunchables(korrid, bridge)

    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => calls.includes("sign"))
    await invoke(() => harness.current().reload())
    await invoke(() =>
      signing.resolve({ _tag: "Ok", payload: launchSpec("launch-a") }),
    )
    await waitFor(() => calls.includes("cancel:launch-a"))

    expect(calls).toEqual(["sign", "cancel:launch-a"])
  })

  it("cancels a reservation returned after unmount without starting Artemis", async () => {
    const signing = deferred<Awaited<ReturnType<KorridClient["moonlightLaunchPrepare"]>>>()
    const calls: string[] = []
    const { korrid, bridge } = fixture({
      moonlightLaunchPrepare() {
        calls.push("sign")
        return signing.promise
      },
      async moonlightLaunchCancel(launchId) {
        calls.push(`cancel:${launchId}`)
        return { _tag: "Ok", payload: { launchId } }
      },
    }, {
      async startStream() {
        calls.push("native-start")
        return { _tag: "StreamStarted" }
      },
    })
    const harness = await mountLaunchables(korrid, bridge)

    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => calls.includes("sign"))
    await harness.unmount()
    await invoke(() =>
      signing.resolve({ _tag: "Ok", payload: launchSpec("launch-a") }),
    )
    await waitFor(() => calls.includes("cancel:launch-a"))

    expect(calls).toEqual(["sign", "cancel:launch-a"])
  })

  it("cancels after host preparation when superseded at that await and refreshes status without starting Artemis", async () => {
    const hostPreparation = deferred<Awaited<ReturnType<KorridClient["sessionPrepare"]>>>()
    const calls: string[] = []
    let statusReads = 0
    const { korrid, bridge } = fixture({
      async moonlightLaunchPrepare() {
        calls.push("sign")
        return { _tag: "Ok", payload: launchSpec("launch-a") }
      },
      async moonlightLaunchCancel(launchId) {
        calls.push(`cancel:${launchId}`)
        return { _tag: "Ok", payload: { launchId } }
      },
      sessionPrepare() {
        calls.push("host-prepare")
        return hostPreparation.promise
      },
      async sessionStatus() {
        statusReads += 1
        return { _tag: "Ok", payload: {} }
      },
    }, {
      async startStream() {
        calls.push("native-start")
        return { _tag: "StreamStarted" }
      },
    })
    const harness = await mountLaunchables(korrid, bridge)
    const initialStatusReads = statusReads

    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => calls.includes("host-prepare"))
    await invoke(() => {
      hostPreparation.resolve({
        _tag: "Ok",
        payload: { gameId: "game", launchId: "host-launch" },
      })
      harness.current().reload()
    })
    await waitFor(() => calls.includes("cancel:launch-a"))
    await waitFor(() => statusReads > initialStatusReads + 1)

    expect(calls).toEqual(["sign", "host-prepare", "cancel:launch-a"])
  })

  it("binds cancellation to the superseded launch id when a replacement reservation wins", async () => {
    const firstHostPreparation = deferred<Awaited<ReturnType<KorridClient["sessionPrepare"]>>>()
    const calls: string[] = []
    let signing = 0
    let hostPreparations = 0
    const { korrid, bridge } = fixture({
      async moonlightLaunchPrepare() {
        signing += 1
        const launchId = signing === 1 ? "launch-a" : "launch-b"
        calls.push(`sign:${launchId}`)
        return { _tag: "Ok", payload: launchSpec(launchId) }
      },
      async moonlightLaunchCancel(launchId) {
        calls.push(`cancel:${launchId}`)
        return { _tag: "Ok", payload: { launchId } }
      },
      sessionPrepare() {
        hostPreparations += 1
        calls.push(`host-prepare:${hostPreparations}`)
        return hostPreparations === 1
          ? firstHostPreparation.promise
          : Promise.resolve({
              _tag: "Ok" as const,
              payload: { gameId: "game", launchId: "host-b" },
            })
      },
    }, {
      async startStream(spec) {
        calls.push(`native-start:${spec.launchId}`)
        return { _tag: "StreamStarted" }
      },
    })
    const harness = await mountLaunchables(korrid, bridge)

    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => calls.includes("host-prepare:1"))
    await invoke(() => harness.current().reload())
    await waitFor(() => harness.current().state._tag === "Ready")
    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => calls.includes("native-start:launch-b"))
    await invoke(() =>
      firstHostPreparation.resolve({
        _tag: "Ok",
        payload: { gameId: "game", launchId: "host-a" },
      }),
    )
    await waitFor(() => calls.includes("cancel:launch-a"))

    expect(calls).toContain("native-start:launch-b")
    expect(calls).toContain("cancel:launch-a")
    expect(calls).not.toContain("cancel:launch-b")
    expect(calls).not.toContain("native-start:launch-a")
  })

  it("does not prepare or start when signing fails", async () => {
    const calls: string[] = []
    const { korrid, bridge } = fixture({
      async moonlightLaunchPrepare() {
        calls.push("sign")
        return {
          _tag: "Err",
          payload: { code: "SigningFailed", message: "cannot sign" },
        }
      },
      async sessionPrepare() {
        calls.push("host-prepare")
        return { _tag: "Ok", payload: { gameId: "game", launchId: "host" } }
      },
    }, {
      async startStream() {
        calls.push("native-start")
        return { _tag: "StreamStarted" }
      },
    })
    const harness = await mountLaunchables(korrid, bridge)

    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => harness.current().state._tag === "Ready")

    expect(calls).toEqual(["sign"])
    expect(harness.current().state).toMatchObject({ notice: { message: "StartFailed: cannot sign" } })
  })

  it("invalidates the signed reservation when host preparation fails", async () => {
    const calls: string[] = []
    const { korrid, bridge } = fixture({
      async moonlightLaunchPrepare() {
        calls.push("sign")
        return { _tag: "Ok", payload: launchSpec("launch-a") }
      },
      async moonlightLaunchCancel(launchId) {
        calls.push(`cancel:${launchId}`)
        return { _tag: "Ok", payload: { launchId } }
      },
      async sessionPrepare() {
        calls.push("host-prepare")
        return {
          _tag: "Err",
          payload: { code: "UpstreamFailure", message: "host prepare failed" },
        }
      },
    }, {
      async startStream() {
        calls.push("native-start")
        return { _tag: "StreamStarted" }
      },
    })
    const harness = await mountLaunchables(korrid, bridge)

    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => calls.includes("cancel:launch-a"))

    expect(calls).toEqual(["sign", "host-prepare", "cancel:launch-a"])
    expect(harness.current().state).toMatchObject({
      _tag: "Ready",
      notice: { message: "UpstreamFailure: host prepare failed" },
    })
  })

  it("refreshes the active host session after native startup fails", async () => {
    let hostPrepared = false
    const calls: string[] = []
    const { korrid, bridge } = fixture({
      async moonlightLaunchPrepare() {
        return { _tag: "Ok", payload: launchSpec("launch-a") }
      },
      async moonlightLaunchCancel(launchId) {
        calls.push(`cancel:${launchId}`)
        return { _tag: "Err", payload: { code: "Consumed", message: "already used" } }
      },
      async sessionPrepare() {
        hostPrepared = true
        return { _tag: "Ok", payload: { gameId: "game", launchId: "host-launch" } }
      },
      async sessionStatus() {
        return hostPrepared
          ? {
              _tag: "Ok",
              payload: {
                active: {
                  launchId: "host-launch",
                  gameId: "game",
                  title: "Game",
                  host: "Host",
                },
              },
            }
          : { _tag: "Ok", payload: {} }
      },
    }, {
      async startStream() {
        calls.push("native-start")
        return {
          _tag: "StreamFailed",
          reason: "HostUnreachable",
          message: "native attach failed",
        }
      },
    })
    const harness = await mountLaunchables(korrid, bridge)

    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => {
      const state = harness.current().state
      return (
        state._tag === "Ready" &&
        state.entries.some(entry => entry.kind === "now-playing")
      )
    })

    expect(calls).toEqual(["native-start"])
    expect(nowPlaying(harness.current())).toMatchObject({
      session: { launchId: "host-launch" },
    })
  })

  it("preserves resume but cancels its reservation when superseded before native start", async () => {
    const signing = deferred<Awaited<ReturnType<KorridClient["moonlightLaunchPrepare"]>>>()
    const calls: string[] = []
    const base = createInMemoryKorridClient({
      games: [{ id: "game", title: "Game", host: "Host", source: { label: "Host", isLocal: false } }],
      moonlight: available,
      activeSession: {
        launchId: "host-launch",
        gameId: "game",
        title: "Game",
        host: "Host",
      },
    })
    const { bridge } = fixture()
    const korrid = {
      ...base,
      moonlightLaunchPrepare() {
        calls.push("sign")
        return signing.promise
      },
      async moonlightLaunchCancel(launchId: string) {
        calls.push(`cancel:${launchId}`)
        return { _tag: "Ok" as const, payload: { launchId } }
      },
    }
    const recordingBridge = {
      ...bridge,
      async startStream() {
        calls.push("native-start")
        return { _tag: "StreamStarted" as const }
      },
    }
    const harness = await mountLaunchables(korrid, recordingBridge)

    await invoke(() => harness.current().confirmEntry(nowPlaying(harness.current())))
    await waitFor(() => calls.includes("sign"))
    await invoke(() => harness.current().reload())
    await invoke(() =>
      signing.resolve({ _tag: "Ok", payload: launchSpec("resume-launch") }),
    )
    await waitFor(() => calls.includes("cancel:resume-launch"))

    expect(calls).toEqual(["sign", "cancel:resume-launch"])
  })

  it("refreshes after cancellation while native startup is in flight because host preparation already succeeded", async () => {
    const nativeStart = deferred<Awaited<ReturnType<LauncherBridge["startStream"]>>>()
    let statusReads = 0
    let nativeStartCalled = false
    const { korrid, bridge } = fixture({
      async moonlightLaunchPrepare() {
        return { _tag: "Ok", payload: launchSpec("launch-a") }
      },
      async moonlightLaunchCancel(launchId) {
        return { _tag: "Err", payload: { code: "Consumed", message: launchId } }
      },
      async sessionPrepare() {
        return { _tag: "Ok", payload: { gameId: "game", launchId: "host-launch" } }
      },
      async sessionStatus() {
        statusReads += 1
        return { _tag: "Ok", payload: {} }
      },
    }, {
      startStream() {
        nativeStartCalled = true
        return nativeStart.promise
      },
    })
    const harness = await mountLaunchables(korrid, bridge)

    await invoke(() => harness.current().confirmEntry(remoteGame(harness.current())))
    await waitFor(() => nativeStartCalled)
    const beforeCancel = statusReads
    await invoke(() => harness.current().reload())
    await invoke(() => nativeStart.resolve({ _tag: "StreamStarted" }))
    await waitFor(
      () => statusReads > beforeCancel && harness.current().state._tag === "Ready",
    )

    expect(harness.current().state._tag).toBe("Ready")
  })
})

describe("overlay permission settings", () => {
  it("rechecks the actual grant when the shell resumes", async () => {
    let reads = 0
    const base = createInMemoryLauncherBridge()
    const bridge: LauncherBridge = {
      ...base,
      async overlayPermission() {
        reads += 1
        return reads === 1
          ? { _tag: "Disabled" as const }
          : { _tag: "Enabled" as const }
      },
    }
    const harness = await mountLaunchables(createInMemoryKorridClient(), bridge)
    await waitFor(() => harness.current().facts.overlay?._tag === "Disabled")

    await invoke(() => window.dispatchEvent(new Event("korri-shell-resumed")))
    await waitFor(() => harness.current().facts.overlay?._tag === "Enabled")

    expect(reads).toBeGreaterThanOrEqual(2)
  })

  it("reports an unavailable settings screen without changing the grant", async () => {
    const base = createInMemoryLauncherBridge()
    const bridge: LauncherBridge = {
      ...base,
      async overlayPermission() {
        return { _tag: "RestrictedOrUnavailable" }
      },
      async openOverlaySettings() {
        return { _tag: "Unavailable", message: "restricted by Android" }
      },
    }
    const harness = await mountLaunchables(createInMemoryKorridClient(), bridge)
    await waitFor(
      () => harness.current().facts.overlay?._tag === "RestrictedOrUnavailable",
    )

    await invoke(() => harness.current().runDeviceAction("overlay-access"))
    await waitFor(() => harness.current().settingsStatus._tag === "Problem")

    expect(harness.current().settingsStatus).toEqual({
      _tag: "Problem",
      settingId: "overlay-access",
      message: "restricted by Android",
    })
  })
})
