import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { PicoHome } from "./pages/PicoHome"
import { picoHomeViewFromCatalog } from "./pico-home-view"

/**
 * Pico's composition root — the only component a host renders.
 *
 * This is the single place that reads the treaty. Everything below receives
 * plain values, which is what lets any part mount in a preview or a test with
 * no Korri behind it.
 *
 * The gameplay overlay is not implemented yet, and this renders nothing for it
 * on purpose: drawing the library on top of a running game would be worse than
 * drawing nothing, and inventing a menu Pico cannot actually drive would be
 * worse still. A host that needs the overlay should mount a surface that has
 * one until this slice lands.
 */
export function PicoSurface({
  model,
  host,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
}) {
  if (model.presentation.kind !== "catalog") return null

  return (
    <div className="pico-theme pico-screen">
      <PicoHome
        clockLabel={model.clockLabel}
        onLaunch={(gameId, launchLocationId) =>
          host.launchGame(gameId, launchLocationId)
        }
        onRetry={() => host.reload()}
        view={picoHomeViewFromCatalog(model.catalog)}
      />
    </div>
  )
}
