/**
 * The keyboard's own fit, for a holder that does not plan it (OPTIONS'
 * entry, SAVE DATA's entry, first boot's NAME step): a measurement, not
 * state. It tries the plan's steps in yield order (src/plan/keyboard.ts:
 * the spacing tightens, then the keys get shorter), writing each step's
 * numbers on the panel as the --zone-kb-* properties its stylesheet reads,
 * and keeps the first whose keys the panel holds without scrolling. Every
 * key's height and width is the plan's, so a key is never a share of the
 * box and never cuts its label. Only when no step fits does the panel
 * scroll, as the last resort.
 *
 * A holder that plans the keyboard itself (FIND, `useZoneFindPlan`) says
 * so (ZoneKeyboard's `planned`); then this writes nothing.
 */
import { useLayoutEffect, type RefObject } from "react"
import { ZONE_KEYBOARD_STEPS, zoneKeyboardPanel, type ZoneKeyboardPlan } from "./plan/keyboard"
import { zoneArtPxOf } from "./use-zone-plan"
import { useZoneLookKey } from "./use-zone-look"

export const ZONE_KEYBOARD_PLANNED = [
  "--zone-kb-key-h",
  "--zone-kb-gap",
  "--zone-kb-pad",
  "--zone-kb-slab",
  "--zone-kb-slant",
  "--zone-kb-unit",
  "--zone-kb-prefer",
  "--zone-kb-grow",
] as const

/** Write one keyboard plan's numbers (art px) on an element, for ZoneKeyboard's stylesheet. */
export function writeZoneKeyboardPlan(element: HTMLElement, keys: ZoneKeyboardPlan): void {
  const values: Record<(typeof ZONE_KEYBOARD_PLANNED)[number], number> = {
    "--zone-kb-key-h": keys.keyH,
    "--zone-kb-gap": keys.gap,
    "--zone-kb-pad": keys.pad,
    "--zone-kb-slab": keys.slab,
    "--zone-kb-slant": keys.slant,
    "--zone-kb-unit": keys.unit,
    "--zone-kb-prefer": keys.controls === "preferred" ? 1 : 0,
    "--zone-kb-grow": keys.controls === "preferred" ? 0 : 1,
  }
  for (const name of ZONE_KEYBOARD_PLANNED) element.style.setProperty(name, String(values[name]))
}

export function clearZoneKeyboardPlan(element: HTMLElement): void {
  for (const name of ZONE_KEYBOARD_PLANNED) element.style.removeProperty(name)
}

/** Fit the keyboard `panel` to the room its holder gives it. */
export function fitZoneKeyboard(panel: HTMLElement): void {
  clearZoneKeyboardPlan(panel)
  const px = zoneArtPxOf(panel)
  const width = Math.floor(panel.offsetWidth / px)
  if (width === 0) return
  for (const [spacing, keyH] of ZONE_KEYBOARD_STEPS) {
    writeZoneKeyboardPlan(panel, zoneKeyboardPanel(width, spacing, keyH))
    if (panel.scrollHeight <= panel.clientHeight + 1) return
  }
}

export function useZoneKeyboardFit(panel: RefObject<HTMLElement | null>, on = true): void {
  // A look changes the faces: measure again in the one now drawn.
  const look = useZoneLookKey()
  useLayoutEffect(() => {
    const element = panel.current
    if (element === null) return
    if (!on) {
      clearZoneKeyboardPlan(element)
      return
    }
    const fit = () => fitZoneKeyboard(element)
    fit()
    let live = true
    void document.fonts?.ready.then(() => {
      if (live) fit()
    })
    const holder = element.parentElement
    if (holder === null || typeof ResizeObserver === "undefined") {
      return () => {
        live = false
      }
    }
    // The holder's box is the room; the panel only follows the steps.
    const observer = new ResizeObserver(fit)
    observer.observe(holder)
    return () => {
      live = false
      observer.disconnect()
    }
  }, [panel, on, look])
}
