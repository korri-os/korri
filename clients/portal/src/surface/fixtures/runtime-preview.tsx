import { createRoot } from "react-dom/client"
import { createInputBus } from "../../input/bus"
import { createKeyboardAdapter, defaultKeyboardKeyMap } from "../../input/keyboard-adapter"
import { createSpatialFocusController } from "../../input/spatial-focus"
import { createInMemoryKorridClient } from "../../korrid/client"
import { SurfaceRoot } from "../SurfaceRoot"
import { portalSurfaceFor } from "../surface-registry"
import { runtimeRoutes } from "./runtime-routes"
import "../../index.css"
import "./runtime-preview.css"

// A first-class fixture consumer of the production client seam. There are no
// live RPC calls, request interceptions, or changes to global network APIs.
const params = new URLSearchParams(location.search)
const fixture = params.get("fixture")
const routes = structuredClone(runtimeRoutes)
if (fixture === "ready" || fixture === "warnings") delete routes.gameRuntime
if (fixture === "warnings") {
  for (const route of routes.routes)
    route.warnings.push({
      setting: "video_driver",
      launcherId: route.launcherId,
      build: route.launcherBuild,
      message: "Unsupported in pinned version 1.20; setting omitted",
    })
}
const korrid = createInMemoryKorridClient({
  games: [{ id: "wl4", title: "Wario Land 4", source: { label: "This device", isLocal: true } }],
  gameRoutes: fixture === "error" ? [] : [routes],
  routeDelayMs: fixture === "loading" ? 4000 : 0,
  routeMutationDelayMs: fixture === "busy" ? 4000 : 0,
  routePermission:
    fixture === "read-only" ? "ReadOnly" : fixture === "local-sessions" ? "LocalSessions" : "Full",
  ...(fixture === "conflict"
    ? { activeSession: { launchId: "other-session", gameId: "other-game", title: "Another game" } }
    : {}),
})
const bus = createInputBus()
bus.use(
  createKeyboardAdapter({ keymap: { ...defaultKeyboardKeyMap, options: ["o"], menu: ["m"] } }),
)
createSpatialFocusController(bus)
const app = document.getElementById("app")
if (!app) throw new Error("Missing fixture root")
createRoot(app).render(
  <SurfaceRoot
    bus={bus}
    korrid={korrid}
    surface={portalSurfaceFor("catalog", params.get("surface") ?? "shift")}
  />,
)
