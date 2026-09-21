import type { ActiveSession } from "@contracts/generated/korrid"
import type { KorriRpcBridgeSurface } from "@contracts/bridge/korri-rpc-bridge"
import ReactDOM from "react-dom/client"
import { createInputBus } from "./input/bus"
import { createGamepadAdapter } from "./input/gamepad-adapter"
import { createKeyboardAdapter } from "./input/keyboard-adapter"
import { createSpatialFocusController } from "./input/spatial-focus"
import {
  createHttpKorridClient,
  createInMemoryKorridClient,
} from "./korrid/client"
import { createInMemoryOverlayController } from "./overlay/in-memory-overlay-controller"
import { createLiveOverlayController } from "./overlay/live-overlay-controller"
import { OverlayRoot } from "./overlay/OverlayRoot"
import {
  createInspectionGeneration,
  reconcileFocusedSession,
} from "./overlay/reconcile-focused-session"
import { readLinuxRuntimeConfig } from "./runtime-config"
import { resolveSurfacePreference } from "./surface/surface-preference"
import { portalSurfaceFor } from "./surface/surface-registry"
import { SurfaceRoot } from "./surface/SurfaceRoot"
import "./index.css"

declare global {
  interface Window {
    KorriRpc?: KorriRpcBridgeSurface
  }
}

const GAMEPLAY_OVERLAY_SCREEN_PARAM = "screen"
const GAMEPLAY_OVERLAY_SCREEN_VALUE = "gameplay-overlay"

const rootElement = document.getElementById("app")
if (!rootElement) throw new Error("#app element not found")
const root = ReactDOM.createRoot(rootElement)
const query = new URLSearchParams(window.location.search)
const isGameplayOverlay =
  query.get(GAMEPLAY_OVERLAY_SCREEN_PARAM) === GAMEPLAY_OVERLAY_SCREEN_VALUE
const preferredSurfaceId = resolveSurfacePreference(
  window.location,
  window.localStorage,
)

if (isGameplayOverlay) {
  document.documentElement.dataset.korriGameplayOverlay = ""
}

if (import.meta.env.DEV && isGameplayOverlay) {
  const bus = createInputBus()
  createSpatialFocusController(bus)
  bus.use(createKeyboardAdapter())
  root.render(
    <OverlayRoot
      bus={bus}
      controller={createInMemoryOverlayController()}
      surface={portalSurfaceFor("gameplay-overlay", preferredSurfaceId)}
    />,
  )
} else {
  void mountPortal().catch(error => {
    rootElement.textContent = "Korri could not connect to this device."
    console.error("Portal startup failed", error)
  })
}

async function mountPortal() {
  // Production consumes both private bridge methods before any surface mounts.
  const linuxRuntime = import.meta.env.DEV
    ? undefined
    : readLinuxRuntimeConfig(window.KorriRpc)
  const korrid = linuxRuntime
    ? createHttpKorridClient(
        `http://127.0.0.1:${linuxRuntime.korridPort}`,
        linuxRuntime.korridCapability,
      )
    : createInMemoryKorridClient()
  const bus = createInputBus()
  bus.use(createKeyboardAdapter())
  bus.use(createGamepadAdapter())
  createSpatialFocusController(bus)

  let presentation: "catalog" | "gameplay-overlay" = "catalog"
  let overlayLaunchId: string | undefined
  let retainedFailureLaunchId: string | undefined
  const inspections = createInspectionGeneration()

  const showCatalog = () => {
    inspections.invalidate()
    presentation = "catalog"
    overlayLaunchId = undefined
    retainedFailureLaunchId = undefined
    delete document.documentElement.dataset.korriGameplayOverlay
    root.render(
      <SurfaceRoot
        bus={bus}
        korrid={korrid}
        surface={portalSurfaceFor("catalog", preferredSurfaceId)}
      />,
    )
  }

  const showCatalogWhileWatchingReturn = () => {
    const exactLaunchId = overlayLaunchId
    showCatalog()
    retainedFailureLaunchId = exactLaunchId
  }

  const showOverlay = (active: ActiveSession) => {
    presentation = "gameplay-overlay"
    overlayLaunchId = active.launchId
    document.documentElement.dataset.korriGameplayOverlay = ""
    root.render(
      <OverlayRoot
        bus={bus}
        controller={createLiveOverlayController({
          launchId: active.launchId,
          ...(active.title === undefined ? {} : { title: active.title }),
          korrid,
          platform: {
            openKorri: showCatalogWhileWatchingReturn,
            sessionEnded: showCatalog,
            // The stale overlay stays visible with its notice. The next Home
            // focus or explicit retry reads state again; no action retargets.
            sessionChanged: () => {},
          },
        })}
        surface={portalSurfaceFor("gameplay-overlay", preferredSurfaceId)}
      />,
    )
  }

  const inspectFocusedSession = async () => {
    const isCurrent = inspections.begin()
    await reconcileFocusedSession({
      korrid,
      isCurrent,
      onActive: active => {
        if (presentation === "gameplay-overlay" && overlayLaunchId === active.launchId) return
        showOverlay(active)
      },
    })
  }

  showCatalog()
  window.addEventListener("focus", () => {
    // A browser focus event is only a prompt to inspect. Mount the overlay
    // only from korrid's process-local Home intent while that same exact launch
    // is frozen or focus-failed. A replacement or arbitrary frozen launch
    // never opens the overlay.
    void inspectFocusedSession()
  })

  // Focusing an already-focused portal does not emit another browser focus
  // event. While the full catalog remains in front, watch only the explicit
  // focus-failed phase. Frozen sessions are ignored here so Open Korri remains
  // a deliberate catalog choice.
  let retainedInspectionInFlight = false
  window.setInterval(() => {
    if (
      retainedInspectionInFlight ||
      presentation !== "catalog" ||
      retainedFailureLaunchId === undefined ||
      !document.hasFocus()
    ) return
    retainedInspectionInFlight = true
    const isCurrent = inspections.begin()
    void reconcileFocusedSession({
      korrid,
      isCurrent,
      allowFrozen: false,
      expectedLaunchId: retainedFailureLaunchId,
      attempts: 1,
      onActive: active => {
        if (presentation === "catalog" && document.hasFocus()) showOverlay(active)
      },
    }).finally(() => { retainedInspectionInFlight = false })
  }, 250)

  if (isGameplayOverlay || document.hasFocus()) await inspectFocusedSession()
}
