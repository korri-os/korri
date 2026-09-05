/**
 * Which surface serves which presentation.
 *
 * Pure: no DOM and no rendering, because the question here is selection, not
 * pixels. Loading this module loads every surface package, which is exactly why
 * the rendering suites build their own surface instead of importing it.
 */
import { describe, expect, test } from "bun:test"
import {
  DEFAULT_SURFACE_ID,
  PORTAL_SURFACES,
  portalSurfaceById,
  portalSurfaceFor,
} from "./surface-registry"

describe("the surface registry", () => {
  test("registers each surface once", () => {
    const ids = PORTAL_SURFACES.map(surface => surface.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  test("the default surface implements every presentation", () => {
    // It is the fallback for anything a chosen surface cannot serve, so a gap
    // here would leave a presentation with nothing to render at all.
    const fallback = portalSurfaceById(DEFAULT_SURFACE_ID)
    expect(fallback?.presentations).toContain("catalog")
    expect(fallback?.presentations).toContain("gameplay-overlay")
  })

  test("honours a preference where the surface implements the presentation", () => {
    expect(portalSurfaceFor("catalog", "pico").id).toBe("pico")
    expect(portalSurfaceFor("catalog", "shift").id).toBe("shift")
  })

  test("falls back rather than rendering a surface that has no overlay", () => {
    // Pico presents the catalog only. Drawing nothing over a live game would
    // take the device away from the user with no way back.
    expect(portalSurfaceById("pico")?.presentations).not.toContain(
      "gameplay-overlay",
    )
    expect(portalSurfaceFor("gameplay-overlay", "pico").id).toBe(
      DEFAULT_SURFACE_ID,
    )
  })

  test("falls back for an unknown or absent preference", () => {
    expect(portalSurfaceFor("catalog", "does-not-exist").id).toBe(
      DEFAULT_SURFACE_ID,
    )
    expect(portalSurfaceFor("catalog").id).toBe(DEFAULT_SURFACE_ID)
  })
})
