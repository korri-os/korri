/**
 * Boxbuster's root, and the only component that reads the treaty.
 *
 * It turns Korri's model into a store view once and hands the rest of the
 * surface tapes and rooms, never treaty types. The host is used for nothing but
 * a reload until the focus-driven input model lands.
 */
import "./BoxbusterSurface.css"
import type {
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { useMemo } from "react"
import { BoxbusterNotice } from "./BoxbusterNotice"
import { BoxbusterStore } from "./BoxbusterStore"
import { storeSignature, storeViewFrom } from "./boxbuster-store-view"

export function BoxbusterSurface({
  model,
  host,
}: {
  model: SurfaceModel
  host: SurfaceHost
}) {
  // Korri republishes the whole model on any change, a clock tick included,
  // so reading the time here re-checks the 14-day window at least once a
  // minute. The signature covers every fact the store view reads, so the
  // store, its textures, and its cover requests rebuild only when a tape
  // changes place or a drawn fact changes.
  const now = Date.now()
  const signature = storeSignature(model, now)
  // `model` and `now` are deliberately absent from the dependencies: the
  // signature stands in for both.
  const view = useMemo(() => storeViewFrom(model, now), [signature])

  return (
    <div className="boxbuster-surface" data-boxbuster-surface="">
      <BoxbusterStore view={view} />
      <BoxbusterNotice view={view} onReload={() => host.reload()} />
    </div>
  )
}
