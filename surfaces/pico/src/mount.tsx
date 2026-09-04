/**
 * The framework-agnostic mount adapter.
 *
 * A host that does not speak React calls `picoSurface.mount(...)`, pushes new
 * models through the returned handle, and tears it down the same way. React is
 * an implementation detail confined to this file and everything it renders.
 */
import type {
  KorriSurface,
  SurfaceHost,
  SurfaceInstance,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { createRoot } from "react-dom/client"
import { PicoSurface } from "./PicoSurface"

export const picoSurface: KorriSurface = {
  id: "pico",
  title: "Pico",
  mount(
    container: HTMLElement,
    model: SurfaceModel,
    host: SurfaceHost,
  ): SurfaceInstance {
    const root = createRoot(container)
    root.render(<PicoSurface host={host} model={model} />)
    return {
      update(next) {
        root.render(<PicoSurface host={host} model={next} />)
      },
      unmount() {
        root.unmount()
      },
    }
  },
}
