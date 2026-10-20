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

/**
 * Call `then` each time an Anton face finishes loading in `document`. Anton
 * loads on demand, the first time a look (Modern, Blend) draws in it, and
 * a fit measured before it arrived measured the fallback. Only Anton: the
 * pixel faces load as they did before the looks, so Retro fits exactly as
 * it did. Returns the unsubscribe.
 */
export function onZoneAntonLoaded(document: Document | undefined, then: () => void): () => void {
  const fonts = document?.fonts as (FontFaceSet & EventTarget) | undefined
  if (fonts?.addEventListener === undefined) return () => undefined
  const loaded = (event: Event) => {
    const faces = (event as Event & { readonly fontfaces?: readonly FontFace[] }).fontfaces ?? []
    if (faces.some(face => face.family.replace(/"/g, "").startsWith("Zone Anton"))) then()
  }
  fonts.addEventListener("loadingdone", loaded)
  return () => fonts.removeEventListener("loadingdone", loaded)
}
