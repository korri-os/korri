/**
 * FIND's keyboard, planned from the box it rises in: a measurement, not
 * state. While the keyboard is up this measures the box under the tabs
 * (the GAMES body, or the screen below the tabs when the keyboard rises
 * over the foot), runs `planZoneFind` on it in art px and writes the answer
 * on the page's root as custom properties ZoneKeyboard and the page's
 * stylesheet read (--zone-kb-*, --zone-find-*; the page passes the
 * keyboard `planned`, so its own fit stands aside) and as `data-find-band`
 * ("shown" or "hidden"). It re-plans when the box resizes and re-renders
 * nothing.
 */
import { useLayoutEffect, type RefObject } from "react"
import { planZoneFind } from "./plan/keyboard"
import { clearZoneKeyboardPlan, writeZoneKeyboardPlan } from "./use-zone-keyboard-fit"
import { zoneArtPxOf } from "./use-zone-plan"

/** Write the plan for a box `width` by `height` CSS px on `root`. */
export function writeZoneFindPlan(root: HTMLElement, width: number, height: number, band: boolean): void {
  const px = zoneArtPxOf(root)
  const plan = planZoneFind(Math.floor(width / px), Math.floor(height / px), band)
  writeZoneKeyboardPlan(root, plan.keys)
  root.style.setProperty("--zone-find-margin", String(plan.keys.margin))
  root.style.setProperty("--zone-find-inset", String(plan.keys.inset))
  root.dataset.findBand = plan.band > 0 ? "shown" : "hidden"
}

function clearZoneFindPlan(root: HTMLElement): void {
  clearZoneKeyboardPlan(root)
  for (const name of ["--zone-find-margin", "--zone-find-inset"]) root.style.removeProperty(name)
  delete root.dataset.findBand
}

/**
 * `at` is where the keyboard rises ("body" or "screen"), undefined while
 * it is down. `box` is the GAMES body; on "screen" the keyboard's own
 * holder, which stands from the tabs to the screen's foot.
 */
export function useZoneFindPlan(
  root: RefObject<HTMLElement | null>,
  body: RefObject<HTMLElement | null>,
  keys: RefObject<HTMLElement | null>,
  at: "body" | "screen" | undefined,
): void {
  useLayoutEffect(() => {
    const page = root.current
    if (page === null) return
    if (at === undefined) {
      clearZoneFindPlan(page)
      return
    }
    const box = at === "body" ? body.current : keys.current
    if (box === null) return
    const plan = () => {
      // The screen holder's width is the keyboard's; the page's is the box.
      const width = at === "body" ? box.clientWidth : page.clientWidth
      if (width === 0 || box.clientHeight === 0) return
      writeZoneFindPlan(page, width, box.clientHeight, at === "body")
    }
    plan()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(plan)
    observer.observe(box)
    if (at === "screen") observer.observe(page)
    return () => observer.disconnect()
  }, [root, body, keys, at])
}
