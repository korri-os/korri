import {
  GAMEPLAY_OVERLAY_SCREEN_PARAM,
  GAMEPLAY_OVERLAY_SCREEN_VALUE,
  SESSION_SCREEN_PARAM,
  SESSION_SCREEN_VALUE,
  type KorriNativeBridgeSurface,
  type KorriSessionBridgeSurface,
} from "@contracts/bridge/korri-native-bridge"
import ReactDOM from "react-dom/client"
import {
  createInMemoryLauncherBridge,
  createKorriNativeLauncherBridge,
} from "./bridge/launcher-bridge"
import { createInputBus } from "./input/bus"
import { createKeyboardAdapter } from "./input/keyboard-adapter"
import { createKorriNativeAdapter } from "./input/korri-native-adapter"
import { createSpatialFocusController } from "./input/spatial-focus"
import {
  createHttpKorridClient,
  createInMemoryKorridClient,
} from "./korrid/client"
import { createInMemoryOverlayController } from "./overlay/in-memory-overlay-controller"
import { createOverlayController } from "./overlay/overlay-controller"
import { createNativeOverlayHost } from "./overlay/overlay-host"
import { createNativeOverlayConnection } from "./overlay/overlay-native"
import { OverlayRoot } from "./overlay/OverlayRoot"
import { createSessionLifecycleAdapter } from "./session/lifecycle-adapter"
import {
  createFixtureLifecycleAdapter,
  SessionScreen,
} from "./session/SessionScreen"
import { resolveSurfacePreference } from "./surface/surface-preference"
import { portalSurfaceFor } from "./surface/surface-registry"
import { SurfaceRoot } from "./surface/SurfaceRoot"
import "./index.css"

declare global {
  interface Window {
    KorriNative?: KorriNativeBridgeSurface
    KorriSession?: KorriSessionBridgeSurface
  }
}

const rootElement = document.getElementById("app")
if (!rootElement) throw new Error("#app element not found")
const root = ReactDOM.createRoot(rootElement)
const query = new URLSearchParams(window.location.search)

/* Which surface the user has chosen, resolved once. The registry decides
 * whether that choice can serve a given presentation; a surface with no
 * gameplay overlay still gets to own the catalog. */
const preferredSurfaceId = resolveSurfacePreference(
  window.location,
  window.localStorage,
)

// The session screen is the same bundled app booted with a query param
// (treaty: SESSION_SCREEN_PARAM). Inside the stream Activity's overlay the
// shell injects KorriSession; in browser dev a fixture timeline plays.
const isSessionScreen =
  query.get(SESSION_SCREEN_PARAM) === SESSION_SCREEN_VALUE
const isGameplayOverlay =
  query.get(GAMEPLAY_OVERLAY_SCREEN_PARAM) === GAMEPLAY_OVERLAY_SCREEN_VALUE
if (isGameplayOverlay) {
  // Window transparency must exist before React's first overlay frame.
  document.documentElement.dataset.korriGameplayOverlay = ""
}

if (isSessionScreen) {
  const session = window.KorriSession
  const adapter = session
    ? createSessionLifecycleAdapter(session)
    : createFixtureLifecycleAdapter()
  const exit = () => {
    if (session) session.exitToPortal()
    else window.location.search = ""
  }
  root.render(<SessionScreen adapter={adapter} onExit={exit} />)
} else if (isGameplayOverlay) {
  const bus = createInputBus()
  createSpatialFocusController(bus)
  if (window.KorriOverlay) {
    const connection = createNativeOverlayConnection(window.KorriOverlay, bus)
    createNativeOverlayHost({
      connection,
      page: window,
      createController(config) {
        const korrid = createHttpKorridClient(
          `http://127.0.0.1:${config.korridPort}`,
          config.korridCapability,
        )
        return createOverlayController({
          launchId: config.launchId,
          korrid,
          platform: connection.platform,
        })
      },
      mount(controller) {
        root.render(
          <OverlayRoot
            bus={bus}
            controller={controller}
            surface={portalSurfaceFor("gameplay-overlay", preferredSurfaceId)}
          />,
        )
      },
      unmount() {
        root.unmount()
      },
    })
  } else {
    bus.use(createKeyboardAdapter())
    root.render(
      <OverlayRoot
        bus={bus}
        controller={createInMemoryOverlayController()}
        surface={portalSurfaceFor("gameplay-overlay", preferredSurfaceId)}
      />,
    )
  }
} else {
  // Composition root: this is the one place that knows whether we're inside
  // the Android shell or a desktop browser dev session.
  const bus = createInputBus()
  bus.use(createKeyboardAdapter())
  bus.use(createKorriNativeAdapter())
  createSpatialFocusController(bus)

  const bridge = window.KorriNative
    ? createKorriNativeLauncherBridge(window.KorriNative)
    : createInMemoryLauncherBridge()

  // The brain: embedded korrid inside the shell, in-memory in browser dev.
  const korridPort = window.KorriNative?.korridPort() ?? -1
  const korridCapability = window.KorriNative?.korridCapability() ?? ""
  const korrid =
    korridPort > 0 && korridCapability !== ""
      ? createHttpKorridClient(
          `http://127.0.0.1:${korridPort}`,
          korridCapability,
        )
      : createInMemoryKorridClient()

  root.render(
    <SurfaceRoot
      bridge={bridge}
      bus={bus}
      korrid={korrid}
      surface={portalSurfaceFor("catalog", preferredSurfaceId)}
    />,
  )
}
