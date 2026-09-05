import { afterEach, describe, expect, test } from "bun:test"
import { SHELL_RESUMED_EVENT } from "@contracts/bridge/korri-native-bridge"
import type { DiscoverySnapshot } from "@contracts/generated/korrid"
import { act, createElement, useEffect } from "react"
import { createRoot } from "react-dom/client"
import type { Root } from "react-dom/client"
import {
  createInMemoryLauncherBridge,
  type InMemoryLauncherBridgeConfig,
  type LauncherBridge,
} from "../bridge/launcher-bridge"
import {
  createInMemoryKorridClient,
  type InMemoryKorridClientConfig,
  type KorridClient,
} from "../korrid/client"
import type { Launchables } from "./use-launchables"
import {
  completeFolderReceiptRegistration,
  initialFolderReceiptState,
  releaseUnknownFolderReceipt,
  selectFolderReceipt,
} from "./folder-receipt-state"
import { resolveLocalGameCoverUrls, useLaunchables } from "./use-launchables"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true

interface RecordingLauncherBridge {
  readonly bridge: LauncherBridge
  readonly calls: {
    openedGameFolderPicker: number
    acknowledgedGameFolderPicker: string[]
  }
}

function createRecordingLauncherBridge(
  config: InMemoryLauncherBridgeConfig = {},
): RecordingLauncherBridge {
  const bridge = createInMemoryLauncherBridge(config)
  const calls: RecordingLauncherBridge["calls"] = {
    openedGameFolderPicker: 0,
    acknowledgedGameFolderPicker: [],
  }
  return {
    bridge: {
      ...bridge,
      async openGameFolderPicker() {
        calls.openedGameFolderPicker += 1
        return bridge.openGameFolderPicker()
      },
      async acknowledgeGameFolderPicker(generation) {
        calls.acknowledgedGameFolderPicker.push(generation)
        return bridge.acknowledgeGameFolderPicker(generation)
      },
    },
    calls,
  }
}

interface ActionGate {
  readonly promise: Promise<void>
  release(): void
}

function createActionGate(): ActionGate {
  let release!: () => void
  const promise = new Promise<void>(resolve => {
    release = resolve
  })
  return { promise, release }
}

interface RecordingKorridClient {
  readonly client: KorridClient
  readonly calls: {
    registeredDiscoveryReceipts: string[]
    removedDiscoveryLocations: string[]
    rescannedDiscovery: number
  }
}

function createRecordingKorridClient(
  config: InMemoryKorridClientConfig = {},
  options: {
    readonly failRemoveLocationIds?: readonly string[]
    readonly registerGate?: ActionGate
    readonly removeGate?: ActionGate
    readonly rescanGate?: ActionGate
  } = {},
): RecordingKorridClient {
  const client = createInMemoryKorridClient(config)
  const failRemoveLocationIds = new Set(options.failRemoveLocationIds ?? [])
  const calls: RecordingKorridClient["calls"] = {
    registeredDiscoveryReceipts: [],
    removedDiscoveryLocations: [],
    rescannedDiscovery: 0,
  }
  return {
    client: {
      ...client,
      async registerDiscoveryReceipt(receipt) {
        calls.registeredDiscoveryReceipts.push(receipt)
        await options.registerGate?.promise
        return client.registerDiscoveryReceipt(receipt)
      },
      async removeDiscoveryLocation(locationId) {
        calls.removedDiscoveryLocations.push(locationId)
        await options.removeGate?.promise
        if (failRemoveLocationIds.has(locationId)) {
          return {
            _tag: "Err",
            payload: {
              code: "DiscoveryLocationBusy",
              message: `cannot remove ${locationId}`,
            },
          }
        }
        return client.removeDiscoveryLocation(locationId)
      },
      async rescanDiscovery() {
        calls.rescannedDiscovery += 1
        await options.rescanGate?.promise
        return client.rescanDiscovery()
      },
    },
    calls,
  }
}

interface LaunchablesHarness {
  readonly current: () => Launchables
  readonly unmount: () => Promise<void>
}

function LaunchablesProbe(props: {
  readonly bridge: LauncherBridge
  readonly korrid: KorridClient
  readonly onRender: (launchables: Launchables) => void
}) {
  const launchables = useLaunchables(props.bridge, props.korrid)
  useEffect(() => props.onRender(launchables))
  return null
}

async function renderLaunchables(
  bridge: LauncherBridge,
  korrid: KorridClient,
): Promise<LaunchablesHarness> {
  const container = document.createElement("div")
  document.body.appendChild(container)
  const root: Root = createRoot(container)
  let current: Launchables | undefined

  await act(async () => {
    root.render(
      createElement(LaunchablesProbe, {
        bridge,
        korrid,
        onRender: launchables => {
          current = launchables
        },
      }),
    )
  })

  const harness: LaunchablesHarness = {
    current: () => {
      if (current === undefined) throw new Error("launchables has not rendered")
      return current
    },
    async unmount() {
      await act(async () => root.unmount())
      container.remove()
    },
  }

  await waitFor(() => harness.current().state._tag === "Ready")
  return harness
}

async function waitForMicrotasks(
  predicate: () => boolean,
  description = "condition",
): Promise<void> {
  let lastError: unknown
  for (let attempt = 0; attempt < 20; attempt += 1) {
    await act(async () => {
      await Promise.resolve()
    })
    try {
      if (predicate()) return
    } catch (error) {
      lastError = error
    }
  }
  if (lastError !== undefined) throw lastError
  throw new Error(`timed out waiting for ${description}`)
}

async function waitFor(
  predicate: () => boolean,
  description = "condition",
): Promise<void> {
  const deadline = Date.now() + 1000
  let lastError: unknown
  while (Date.now() < deadline) {
    await act(async () => {
      await Promise.resolve()
    })
    try {
      if (predicate()) return
    } catch (error) {
      lastError = error
    }
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 1))
    })
  }
  if (lastError !== undefined) throw lastError
  throw new Error(`timed out waiting for ${description}`)
}

async function resumeShell() {
  await act(async () => {
    window.dispatchEvent(new Event(SHELL_RESUMED_EVENT))
  })
}

async function letQueuedTimersRun() {
  await act(async () => {
    await new Promise(resolve => setTimeout(resolve, 0))
  })
}

afterEach(() => {
  document.body.innerHTML = ""
})

function settingIsSaving(launchables: Launchables, settingId: string): boolean {
  const status = launchables.settingsStatus
  return status._tag === "Saving" && status.settingId === settingId
}

function readyStateHasGame(launchables: Launchables, gameId: string): boolean {
  const state = launchables.state
  return (
    state._tag === "Ready" &&
    state.entries.some(
      entry => entry.kind === "game" && entry.game.id === gameId,
    )
  )
}

describe("resolveLocalGameCoverUrls", () => {
  test("converts opaque local cover asset ids through the bridge seam", async () => {
    const assetId = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png"
    const bridge = createInMemoryLauncherBridge({
      localGameAssetUrls: { [assetId]: "data:image/png;base64,fixture" },
    })

    const outcome = await resolveLocalGameCoverUrls(bridge, {
      _tag: "Ok",
      payload: {
        games: [{ id: "wl4", title: "Wario Land 4", system: "GBA", coverAssetId: assetId }],
      },
    })

    expect(outcome).toEqual({
      _tag: "Ok",
      payload: {
        games: [
          {
            id: "wl4",
            title: "Wario Land 4",
            system: "GBA",
            coverAssetId: assetId,
            coverArtUrl: "data:image/png;base64,fixture",
          },
        ],
      },
    })
  })

  test("leaves missing or unresolved assets absent", async () => {
    const outcome = await resolveLocalGameCoverUrls(createInMemoryLauncherBridge(), {
      _tag: "Ok",
      payload: {
        games: [
          {
            id: "wl4",
            title: "Wario Land 4",
            system: "GBA",
            coverAssetId: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.png",
          },
          { id: "tmnt", title: "TMNT", system: "Android" },
        ],
      },
    })

    expect(outcome._tag).toBe("Ok")
    if (outcome._tag !== "Ok") return
    expect(outcome.payload.games[0]).not.toHaveProperty("coverArtUrl")
    expect(outcome.payload.games[1]).not.toHaveProperty("coverArtUrl")
  })
})

describe("useLaunchables discovery actions", () => {
  test("adds a selected folder once, acknowledges it, rescans, and removes it", async () => {
    const registerGate = createActionGate()
    const rescanGate = createActionGate()
    const removeGate = createActionGate()
    const { bridge, calls: bridgeCalls } = createRecordingLauncherBridge()
    const { client, calls: clientCalls } = createRecordingKorridClient(
      {
        games: [
          {
            id: "wl4",
            title: "Wario Land 4",
            source: { label: "zao", isLocal: false },
          },
        ],
      },
      { registerGate, removeGate, rescanGate },
    )
    const harness = await renderLaunchables(bridge, client)
    try {
      await act(async () => {
        harness.current().runDeviceAction("game-folder-add")
      })

      await waitFor(
        () => settingIsSaving(harness.current(), "game-folder-add"),
        "folder add saving status",
      )
      expect(bridgeCalls.openedGameFolderPicker).toBe(1)

      await letQueuedTimersRun()
      await resumeShell()

      await waitFor(
        () => clientCalls.registeredDiscoveryReceipts.length === 1,
        "selected receipt registration",
      )
      expect(clientCalls.registeredDiscoveryReceipts).toEqual([
        "in-memory-folder-receipt",
      ])
      expect(harness.current().settingsStatus).toEqual({
        _tag: "Saving",
        settingId: "game-folder-add",
      })

      registerGate.release()
      await waitForMicrotasks(
        () =>
          bridgeCalls.acknowledgedGameFolderPicker.length === 1 &&
          harness.current().settingsStatus._tag === "Idle" &&
          harness.current().facts.discovery?.state._tag === "Scanning",
        "accepted receipt acknowledgement",
      )
      expect(harness.current().facts.discovery?.locations).toEqual([
        { id: "in-memory-folder-receipt", label: "Selected folder 1" },
      ])

      await letQueuedTimersRun()
      await resumeShell()
      await waitFor(
        () => harness.current().facts.discovery?.state._tag === "Idle",
        "settled folder registration scan",
      )

      await act(async () => {
        harness.current().runDeviceAction("game-folder-rescan")
      })
      await waitFor(
        () => settingIsSaving(harness.current(), "game-folder-rescan"),
        "rescan saving status",
      )
      await waitFor(
        () => clientCalls.rescannedDiscovery === 1,
        "rescan discovery route",
      )
      rescanGate.release()
      await waitForMicrotasks(
        () =>
          harness.current().settingsStatus._tag === "Idle" &&
          harness.current().facts.discovery?.state._tag === "Scanning",
        "rescan discovery facts",
      )
      await letQueuedTimersRun()
      await resumeShell()
      await waitFor(
        () => harness.current().facts.discovery?.state._tag === "Idle",
        "settled rescan",
      )

      await act(async () => {
        harness
          .current()
          .runDeviceAction("game-folder-remove:in-memory-folder-receipt")
      })
      await waitFor(
        () =>
          settingIsSaving(
            harness.current(),
            "game-folder:in-memory-folder-receipt",
          ),
        "folder remove saving status",
      )
      await waitFor(
        () => clientCalls.removedDiscoveryLocations.length === 1,
        "folder remove route",
      )
      expect(clientCalls.removedDiscoveryLocations).toEqual([
        "in-memory-folder-receipt",
      ])
      removeGate.release()
      await waitForMicrotasks(
        () =>
          harness.current().settingsStatus._tag === "Idle" &&
          harness.current().facts.discovery?.locations.length === 0,
        "folder remove discovery facts",
      )
    } finally {
      await harness.unmount()
    }
  })

  test("maps a failed row removal to that setting without dropping games", async () => {
    const discovery: DiscoverySnapshot = {
      generation: "fixture-discovery-0",
      state: { _tag: "Idle", payload: {} },
      locations: [
        { id: "keep-me", label: "Keep me" },
        { id: "other", label: "Other folder" },
      ],
      diagnostics: [],
    }
    const removeGate = createActionGate()
    const { bridge } = createRecordingLauncherBridge()
    const { client, calls } = createRecordingKorridClient(
      {
        discovery,
        games: [
          {
            id: "wl4",
            title: "Wario Land 4",
            source: { label: "zao", isLocal: false },
          },
        ],
      },
      { failRemoveLocationIds: ["keep-me"], removeGate },
    )
    const harness = await renderLaunchables(bridge, client)
    try {
      await waitFor(
        () => readyStateHasGame(harness.current(), "wl4"),
        "initial game list",
      )

      await act(async () => {
        harness.current().runDeviceAction("game-folder-remove:keep-me")
      })
      await waitFor(
        () => settingIsSaving(harness.current(), "game-folder:keep-me"),
        "failed remove saving status",
      )
      expect(calls.removedDiscoveryLocations).toEqual(["keep-me"])
      removeGate.release()
      await waitForMicrotasks(
        () => harness.current().settingsStatus._tag === "Problem",
        "failed remove problem status",
      )

      expect(harness.current().settingsStatus).toEqual({
        _tag: "Problem",
        settingId: "game-folder:keep-me",
        message: "cannot remove keep-me",
      })
      expect(harness.current().facts.discovery?.locations).toEqual(
        discovery.locations,
      )
      expect(readyStateHasGame(harness.current(), "wl4")).toBe(true)
    } finally {
      await harness.unmount()
    }
  })
})

describe("folder receipt state", () => {
  test("keeps an unreachable receipt actionable until a new picker generation proceeds", () => {
    let state = initialFolderReceiptState()

    const selected = selectFolderReceipt(state, "picker-1")
    expect(selected._tag).toBe("Submit")
    state = selected.state

    const unreachable = completeFolderReceiptRegistration(
      state,
      "picker-1",
      "BrainUnreachable",
      "brain offline",
    )
    expect(unreachable).toMatchObject({
      _tag: "ReportProblem",
      message: "brain offline",
    })
    state = unreachable.state

    const resumed = selectFolderReceipt(state, "picker-1")
    expect(resumed._tag).toBe("Submit")
    state = resumed.state

    const unknown = completeFolderReceiptRegistration(
      state,
      "picker-1",
      "ReceiptUnknown",
      "receipt expired",
    )
    expect(unknown).toMatchObject({
      _tag: "ReportUnknown",
      message:
        "Korri could not confirm that folder after reconnecting. Choose it again.",
    })
    expect(unknown._tag).not.toBe("Acknowledge")
    state = unknown.state

    const repeated = selectFolderReceipt(state, "picker-1")
    expect(repeated._tag).toBe("Ignore")
    state = releaseUnknownFolderReceipt(state, "picker-1")

    const nextPicker = selectFolderReceipt(state, "picker-2")
    expect(nextPicker._tag).toBe("Submit")
  })
})
