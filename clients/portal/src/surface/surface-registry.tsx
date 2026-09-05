import type {
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { PicoSurface } from "@korri/pico"
import { ShiftSurface } from "@korri/shift"
import type { ReactNode } from "react"

/**
 * Which of the treaty's presentations a surface actually renders.
 *
 * The treaty lets a surface present less than Korri knows, and both shipped
 * surfaces take it up: Shift renders the catalog and the gameplay overlay, Pico
 * renders only the catalog. Declaring that here means the portal can pick a
 * surface per presentation instead of discovering the gap as a blank screen
 * over a running game.
 */
export type PortalPresentation = "catalog" | "gameplay-overlay"

export interface PortalSurface {
  readonly id: string
  readonly title: string
  readonly presentations: readonly PortalPresentation[]
  readonly render: (props: {
    readonly model: SurfaceModel
    readonly host: SurfaceHost
  }) => ReactNode
}

/**
 * Every surface this build can mount. A surface earns a line here and nothing
 * else: no portal code names one, so adding a third is this list plus an alias.
 */
export const PORTAL_SURFACES: readonly PortalSurface[] = [
  {
    id: "shift",
    title: "Shift",
    presentations: ["catalog", "gameplay-overlay"],
    render: ({ model, host }) => <ShiftSurface host={host} model={model} />,
  },
  {
    id: "pico",
    title: "Pico",
    presentations: ["catalog", "gameplay-overlay"],
    render: ({ model, host }) => <PicoSurface host={host} model={model} />,
  },
]

/** The surface used when nothing is chosen, and the fallback when a chosen one
 * cannot serve a presentation. It must implement every presentation. */
export const DEFAULT_SURFACE_ID = "shift"

export function portalSurfaceById(id: string): PortalSurface | undefined {
  return PORTAL_SURFACES.find((surface) => surface.id === id)
}

/**
 * The surface to mount for one presentation.
 *
 * A preference is honoured only where the chosen surface has an implementation.
 * Falling back is deliberate and narrow: a themed catalog whose author has not
 * written a gameplay overlay yet should still be usable, and the alternative —
 * drawing nothing over a live game — takes the device away from the user with
 * no way back.
 */
export function portalSurfaceFor(
  presentation: PortalPresentation,
  preferredId?: string,
): PortalSurface {
  const preferred =
    preferredId === undefined ? undefined : portalSurfaceById(preferredId)
  if (preferred?.presentations.includes(presentation) === true) return preferred

  const fallback = portalSurfaceById(DEFAULT_SURFACE_ID)
  if (fallback === undefined) {
    throw new Error(`No surface registered for ${DEFAULT_SURFACE_ID}`)
  }
  return fallback
}
