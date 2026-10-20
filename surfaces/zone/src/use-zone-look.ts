/**
 * The look an element is drawn in, for the hooks that draw in TypeScript
 * (the stair clips, the ground, the moments, the dialog's motion).
 *
 * One source: the root's `data-look`, which zone-tokens.css turns into the
 * token `--zone-look` for everything inside it. A hook reads it from its
 * own element when it measures, as it reads `--zone-px` (zoneArtPxOf), so a
 * part shown alone in Caliper (whose frame names the look on `<html>`)
 * draws the same as the product. Where no stylesheet is loaded (a test
 * DOM), the nearest `data-look` answers; with none, the default.
 *
 * A hook measures again when the look changes: ZoneSurface provides the
 * look through `ZoneLookContext`, and `useZoneLookKey` returns it for a
 * hook's dependencies. It is only the trigger; what the hook draws comes
 * from the element. A part with no provider never changes look.
 */
import { createContext, useContext } from "react"
import { isZoneLook, ZONE_DEFAULT_LOOK, zoneLookDrawing, type ZoneLook, type ZoneLookDrawing } from "./plan/look"

export const ZoneLookContext = createContext<ZoneLook | undefined>(undefined)

/** The look, as a hook's dependency: it changes when the person picks another. */
export function useZoneLookKey(): ZoneLook | undefined {
  return useContext(ZoneLookContext)
}

/** The look `element` stands in. */
export function zoneLookOf(element: Element): ZoneLook {
  const view = element.ownerDocument.defaultView
  const token = view === null ? "" : view.getComputedStyle(element).getPropertyValue("--zone-look").trim()
  if (isZoneLook(token)) return token
  const marked = element.closest("[data-look]")?.getAttribute("data-look")
  return isZoneLook(marked) ? marked : ZONE_DEFAULT_LOOK
}

/** How to draw for the look `element` stands in. */
export function zoneDrawingOf(element: Element): ZoneLookDrawing {
  return zoneLookDrawing(zoneLookOf(element))
}
