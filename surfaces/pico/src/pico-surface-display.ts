/**
 * A local copy of the treaty's display facts, until they land in
 * `contracts/surface/korri-surface.ts`.
 *
 * The interface below matches the approved treaty text byte for byte, so the
 * merge keeps one definition: delete this file, import `SurfaceDisplay` from
 * the treaty, and use `SurfaceModel` where `PicoSurfaceModel` stands.
 */
import type { SurfaceModel } from "@contracts/surface/korri-surface"

/** One display a surface draws on, in CSS pixels of the surface's viewport. */
export interface SurfaceDisplay {
  readonly id: string
  readonly main: boolean
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
  readonly touch: boolean
}

/** `SurfaceModel` with the treaty's `displays`, present only when the host
 * gives Pico every display. */
export type PicoSurfaceModel = SurfaceModel & {
  readonly displays?: readonly SurfaceDisplay[]
}
