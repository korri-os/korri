import type { SurfaceDisplay } from "@contracts/surface/korri-surface"

/** The Anbernic RG DS as the host states it: two 640 × 480 touch displays,
 * the main one above. */
export const rgdsDisplays: readonly SurfaceDisplay[] = [
  { id: "top", main: true, x: 0, y: 0, width: 640, height: 480, touch: true },
  { id: "bottom", main: false, x: 0, y: 480, width: 640, height: 480, touch: true },
]

/** The same two displays side by side, the main one on the right, so nothing
 * can lean on "top" and "bottom". */
export const sideBySideDisplays: readonly SurfaceDisplay[] = [
  { id: "left", main: false, x: 0, y: 0, width: 640, height: 480, touch: true },
  { id: "right", main: true, x: 640, y: 0, width: 640, height: 480, touch: false },
]
