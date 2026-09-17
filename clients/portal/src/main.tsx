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
import { OverlayRoot } from "./overlay/OverlayRoot"
import { loadLinuxRuntimeConfig } from "./runtime-config"
import { resolveSurfacePreference } from "./surface/surface-preference"
import { portalSurfaceFor } from "./surface/surface-registry"
import { SurfaceRoot } from "./surface/SurfaceRoot"
import "./index.css"

/* The session screen and the gameplay overlay are the same bundled app booted
 * with a query param. Korri reaches them through a URL, so the names are part
 * of how the portal is addressed and not an implementation detail. */
const GAMEPLAY_OVERLAY_SCREEN_PARAM = "screen"
const GAMEPLAY_OVERLAY_SCREEN_VALUE = "gameplay-overlay"

const rootElement = document.getElementById("app")
if (!rootElement) throw new Error("#app element not found")
const root = ReactDOM.createRoot(rootElement)
const query = new URLSearchParams(window.location.search)

const isGameplayOverlay =
  query.get(GAMEPLAY_OVERLAY_SCREEN_PARAM) === GAMEPLAY_OVERLAY_SCREEN_VALUE
/* The registry decides whether the chosen surface can serve a presentation.
 * A surface with no gameplay overlay still gets to own the catalog. */
const preferredSurfaceId = resolveSurfacePreference(
  window.location,
  window.localStorage,
)

if (isGameplayOverlay) {
  // Window transparency must exist before React's first overlay frame.
  document.documentElement.dataset.korriGameplayOverlay = ""
}

if (isGameplayOverlay) {
  const bus = createInputBus()
  createSpatialFocusController(bus)
  bus.use(createKeyboardAdapter())
  /* Same reason as the session screen: korrid performs no in-game control on
   * Linux yet, so the overlay runs against its in-memory controller. */
  root.render(
    <OverlayRoot
      bus={bus}
      controller={createInMemoryOverlayController()}
      surface={portalSurfaceFor("gameplay-overlay", preferredSurfaceId)}
    />,
  )
} else {
  void mountCatalog().catch(error => {
    rootElement.textContent = "Korri could not connect to this device."
    console.error("Portal startup failed", error)
  })
}

// Composition root for production Linux and for browser development.
async function mountCatalog() {
  // korrid's credential arrives privately at runtime, so it must be read
  // before anything mounts. Browser development has no such file.
  const linuxRuntime = import.meta.env.DEV
    ? undefined
    : await loadLinuxRuntimeConfig()
  const bus = createInputBus()
  bus.use(createKeyboardAdapter())
  bus.use(createGamepadAdapter())
  createSpatialFocusController(bus)

  const korrid = linuxRuntime
    ? createHttpKorridClient(
        `http://127.0.0.1:${linuxRuntime.korridPort}`,
        linuxRuntime.korridCapability,
      )
    : createInMemoryKorridClient()

  root.render(
    <SurfaceRoot
      bus={bus}
      korrid={korrid}
      surface={portalSurfaceFor("catalog", resolveSurfacePreference(
        window.location,
        window.localStorage,
        linuxRuntime?.surfaceId,
      ))}
    />,
  )
}
