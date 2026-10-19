/**
 * The framework-agnostic mount adapter.
 *
 * A host that does not speak React calls `zoneSurface.mount(...)`, pushes new
 * models through the returned handle, and tears it down the same way. React
 * is an implementation detail confined to this file and what it renders.
 */
import type {
  KorriSurface,
  SurfaceHost,
  SurfaceInstance,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { createRoot } from "react-dom/client"
import { ZoneSurface } from "./ZoneSurface"

export const zoneSurface: KorriSurface = {
  id: "zone",
  title: "Zone",
  mount(
    container: HTMLElement,
    model: SurfaceModel,
    host: SurfaceHost,
  ): SurfaceInstance {
    const root = createRoot(container)
    root.render(<ZoneSurface host={host} model={model} />)
    return {
      update(next) {
        root.render(<ZoneSurface host={host} model={next} />)
      },
      unmount() {
        root.unmount()
      },
    }
  },
}
