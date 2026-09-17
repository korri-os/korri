import { afterEach, describe, expect, jest, mock, test } from "bun:test"
import * as React from "react"
import * as ReactJsxRuntime from "react/jsx-runtime"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type {
  CatalogSnapshotOutcome,
  Game,
  HealthOutcome,
  LocalGame,
  LocalGamesListOutcome,
  SessionFreezeOutcome,
  SessionPrepareOutcome,
  SessionStatusOutcome,
  SessionStopOutcome,
  SettingsSnapshotOutcome,
  SettingsUpdateOutcome,
  SourceStatusOutcome,
} from "@contracts/generated/korrid"
import {
  SecretSettingStatus,
  SessionFreezerState,
  SessionStopPhase,
  SourceCatalogState,
  SourceStreamControlState,
} from "@contracts/generated/korrid"
import { createInputBus, type InputBus } from "../input/bus"
import { createSpatialFocusController } from "../input/spatial-focus"
import {
  SessionControlFailureReason,
} from "@contracts/generated/korrid"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import type { PortalSurface } from "./surface-registry"
// The portal compiles Shift from source, but Bun resolves Shift's peer React
// from the surface package once its dependencies are installed. Derive that
// package location through the same alias the product imports, then keep this
// cross-boundary test on one React instance without replacing Shift itself.
const shiftPackageRoot = new URL("../", import.meta.resolve("@korri/shift"))
const shiftReact = new URL("node_modules/react/index.js", shiftPackageRoot).pathname
const shiftJsxRuntime = new URL(
  "node_modules/react/jsx-runtime.js",
  shiftPackageRoot,
).pathname

mock.module("react", () => React)
mock.module("react/jsx-runtime", () => ReactJsxRuntime)
mock.module(shiftReact, () => React)
mock.module(shiftJsxRuntime, () => ReactJsxRuntime)

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true

let loadedSurfaceRoot:
  | typeof import("./SurfaceRoot").SurfaceRoot
  | undefined

async function surfaceRootComponent() {
  loadedSurfaceRoot ??= (await import("./SurfaceRoot")).SurfaceRoot
  return loadedSurfaceRoot
}

/* Built from Shift's public entry, loaded after the React mocks above for the
 * same reason SurfaceRoot itself is. Loading the registry would pull every
 * surface package in at once; what it chooses is covered by its own test. */
async function catalogSurface(): Promise<PortalSurface> {
  const { ShiftSurface } = await import("@korri/shift")
  return {
    id: "shift",
    title: "Shift",
    presentations: ["catalog"],
    render: ({ model, host }) => <ShiftSurface host={host} model={model} />,
  }
}

const sleep = (ms = 0) => new Promise(resolve => setTimeout(resolve, ms))

const identity = {
  kind: "provider" as const,
  value: { provider: "fixture", ref: "wario-land-4" },
}

const localGame = (title = "Wario Land 4"): LocalGame => ({
  id: "wl4",
  title,
  system: "Game Boy Advance",
  identity,
})

const remoteGame = (host: string, id = "wl4"): Game => ({
  id,
  title: "Wario Land 4",
  host,
  identity,
  supportsRunnerSelection: false,
  source: { label: host, isLocal: false },
})

interface Calls {
  localLaunches: string[]
  prepared: Array<{ gameId: string; host: string | undefined }>
  streams: Array<{ hostUuid: string; appId: number }>
}

interface Sources {
  localGames: readonly LocalGame[]
  remoteGames: readonly Game[]
}

function okCatalog(games: readonly Game[]): CatalogSnapshotOutcome {
  return { _tag: "Ok", payload: { games: [...games] } }
}

function okLocalGames(games: readonly LocalGame[]): LocalGamesListOutcome {
  return { _tag: "Ok", payload: { games: [...games] } }
}

function okSettings(): SettingsSnapshotOutcome {
  return {
    _tag: "Ok",
    payload: {
      revision: "settings-0",
      deviceName: "Browser",
      plugins: [
        { id: "@korri:mgba", title: "mGBA", enabled: true },
        { id: "@korri:retroarch", title: "RetroArch", enabled: true },
      ],
      steamGridDbCredential: SecretSettingStatus.NotConfigured,
    },
  }
}

function buildKorrid(sources: Sources, calls: Calls): KorridClient {
  return {
    ...createInMemoryKorridClient(),
    async health(): Promise<HealthOutcome> {
      return { _tag: "Ok", payload: { version: "surface-root-test" } }
    },
    async settingsSnapshot(): Promise<SettingsSnapshotOutcome> {
      return okSettings()
    },
    async updateSetting(): Promise<SettingsUpdateOutcome> {
      return okSettings()
    },
    async setSteamGridDbCredential() {
      return {
        _tag: "Ok" as const,
        payload: { status: SecretSettingStatus.Configured },
      }
    },
    async clearSteamGridDbCredential() {
      return {
        _tag: "Ok" as const,
        payload: { status: SecretSettingStatus.NotConfigured },
      }
    },
    async discoverySnapshot() {
      return {
        _tag: "Ok" as const,
        payload: {
          generation: "discovery-0",
          state: { _tag: "Idle" as const, payload: {} },
          locations: [],
          diagnostics: [],
        },
      }
    },
    async registerDiscoveryReceipt() {
      return this.discoverySnapshot()
    },
    async removeDiscoveryLocation() {
      return this.discoverySnapshot()
    },
    async rescanDiscovery() {
      return this.discoverySnapshot()
    },
    async catalogSnapshot(): Promise<CatalogSnapshotOutcome> {
      return okCatalog(sources.remoteGames)
    },
    async localGames(): Promise<LocalGamesListOutcome> {
      return okLocalGames(sources.localGames)
    },
    async sessionPrepare(gameId, host): Promise<SessionPrepareOutcome> {
      calls.prepared.push({ gameId, host })
      return {
        _tag: "Ok",
        payload: { gameId, launchId: `surface-root-prepared-${gameId}` },
      }
    },
    async sessionStatus(): Promise<SessionStatusOutcome> {
      return { _tag: "Ok", payload: {} }
    },
    async sessionStop(): Promise<SessionStopOutcome> {
      return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }
    },
    async sessionFreeze(expectedLaunchId): Promise<SessionFreezeOutcome> {
      return {
        _tag: "Ok",
        payload: {
          launchId: expectedLaunchId,
          state: SessionFreezerState.Frozen,
          changed: true,
        },
      }
    },
    async sessionThaw(expectedLaunchId): Promise<SessionFreezeOutcome> {
      return {
        _tag: "Ok",
        payload: {
          launchId: expectedLaunchId,
          state: SessionFreezerState.Running,
          changed: true,
        },
      }
    },
    async peerList() {
      return { _tag: "Ok", payload: { peers: [] } }
    },
    async sourceStatus(): Promise<SourceStatusOutcome> {
      return {
        _tag: "Ok",
        payload: {
          catalog: SourceCatalogState.Available,
          streamControl: SourceStreamControlState.Enabled,
        },
      }
    },
  }
}


interface SurfaceRootView {
  readonly container: HTMLElement
  readonly bus: InputBus
  cleanup(): void
}

const mounted: SurfaceRootView[] = []

afterEach(() => {
  while (mounted.length > 0) mounted.pop()?.cleanup()
  document.body.innerHTML = ""
  jest.useRealTimers()
})

async function renderSurfaceRoot(
  sources: Sources,
  calls: Calls,
  options: {
    readonly surface?: PortalSurface
    readonly korrid?: KorridClient
  } = {},
): Promise<SurfaceRootView> {
  const container = document.createElement("div")
  document.body.append(container)
  const root: Root = createRoot(container)
  const bus = createInputBus()
  const stopSpatialFocus = createSpatialFocusController(bus)
  let disposed = false
  const cleanup = () => {
    if (disposed) return
    disposed = true
    act(() => root.unmount())
    stopSpatialFocus()
    bus.dispose()
    container.remove()
  }

  try {
    const SurfaceRoot = await surfaceRootComponent()
    const surface = options.surface ?? await catalogSurface()
    await act(async () => {
      root.render(
        <SurfaceRoot
          bus={bus}
          korrid={options.korrid ?? buildKorrid(sources, calls)}
          surface={surface}
        />,
      )
    })

    const view: SurfaceRootView = { container, bus, cleanup }
    mounted.push(view)
    return view
  } catch (error) {
    cleanup()
    throw error
  }
}

async function waitFor(assertReady: () => void, label: string, attempts = 80) {
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

function accessibleName(element: HTMLElement): string {
  return element.getAttribute("aria-label") ?? element.textContent?.trim() ?? ""
}

function buttons(root: ParentNode = document): HTMLButtonElement[] {
  return Array.from(root.querySelectorAll<HTMLButtonElement>("button"))
}

function buttonNamed(name: string, root: ParentNode = document): HTMLButtonElement {
  const button = buttons(root).find(candidate => accessibleName(candidate) === name)
  if (button === undefined) {
    throw new Error(
      `Missing button ${name}. Buttons: ${buttons(root)
        .map(accessibleName)
        .join(", ")}`,
    )
  }
  return button
}

function dialogNamed(name: string): HTMLElement {
  const dialog = document.querySelector<HTMLElement>(`[role="dialog"][aria-label="${name}"]`)
  if (dialog === null) throw new Error(`Missing dialog ${name}`)
  return dialog
}

async function click(button: HTMLButtonElement) {
  await act(async () => {
    button.focus()
    button.dispatchEvent(new FocusEvent("focusin", { bubbles: true }))
    await sleep()
    button.dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true }),
    )
    await sleep()
  })
}

// happy-dom has no layout. Supply geometry only; focus and click stay native.
async function confirm(view: SurfaceRootView) {
  for (const button of buttons(view.container)) {
    button.getBoundingClientRect = () => new DOMRect(0, 0, 100, 40)
  }
  await act(async () => {
    view.bus.emit({ type: "confirm" })
  })
}

async function openWarioDetail() {
  await click(buttonNamed("Library"))
  await waitFor(
    () => expect(document.querySelector("[data-shift-library]")).not.toBeNull(),
    "expected Library screen",
  )
  await click(buttonNamed("Wario Land 4", document.querySelector("[data-shift-library]")!))
  await waitFor(
    () => expect(document.querySelector("[data-shift-detail]")).not.toBeNull(),
    "expected Wario detail screen",
  )
}

describe("SurfaceRoot", () => {
})
