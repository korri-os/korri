import type {
  SurfaceHost,
  SurfaceInputAction,
} from "@contracts/surface/korri-surface"
import { useEffect, useMemo, useSyncExternalStore } from "react"
import type { InputBus } from "../input/bus"
import type { PortalSurface } from "../surface/surface-registry"
import type { OverlayController } from "./overlay-controller"

export interface OverlayRootProps {
  readonly bus: InputBus
  readonly controller: OverlayController
  readonly surface: PortalSurface
}

/** Dedicated portal composition: one controller-backed model, one treaty host,
 * and the surface's own entry. The portal never reaches into a surface's sheet. */
export function OverlayRoot({
  bus,
  controller,
  surface,
}: OverlayRootProps) {
  const model = useSyncExternalStore(
    controller.subscribe,
    controller.model,
    controller.model,
  )

  useEffect(() => {
    void controller.refresh()
    return () => controller.destroy()
  }, [controller])

  const host = useMemo<SurfaceHost>(
    () => ({
      input: {
        on: (action: SurfaceInputAction, handler: () => void) =>
          bus.onAction(action, handler),
      },
      launchGame: () => {},
      runAction: () => {},
      changeSetting: () => {},
      dismissSettingsProblem: () => {},
      gameActions: () => [],
      runGameAction: () => {},
      invokeGameplayControl: (controlId, value) => {
        void controller.invoke(controlId, value)
      },
      dismissGameplayOverlay: controller.dismiss,
      retry: () => {
        void controller.refresh()
      },
      dismiss: controller.dismiss,
      reload: () => {
        void controller.refresh()
      },
    }),
    [bus, controller],
  )

  return surface.render({ model, host })
}
