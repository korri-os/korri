import type { SurfaceDisplay, SurfaceModel } from "@contracts/surface/korri-surface"
import { useEffect, useState } from "react"
import type { KorridClient } from "../korrid/client"
import type { PortalSurface } from "./surface-registry"

/** The same host boundary configures both catalog and overlay viewports. */
export function useSurfaceDisplays(
  model: SurfaceModel,
  surface: PortalSurface,
  korrid?: Pick<KorridClient, "surfaceDisplays">,
  launchId?: string,
  placementOwner?: object,
): SurfaceModel {
  const [facts, setFacts] = useState<readonly SurfaceDisplay[]>()
  const scope = surface.displays ?? "main"
  const presentation = model.presentation.kind
  useEffect(() => {
    let current = true
    setFacts(undefined)
    if (korrid !== undefined) {
      void korrid.surfaceDisplays(scope, presentation === "gameplay-overlay", launchId).then(outcome => {
        if (current && outcome._tag === "Ok") setFacts(outcome.payload)
      })
    }
    return () => { current = false }
  }, [korrid, scope, presentation, launchId, placementOwner])
  return scope === "all" && facts !== undefined ? { ...model, displays: facts } : model
}
