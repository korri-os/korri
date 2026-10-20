/**
 * The plan for the screen Zone was given, kept current by a ResizeObserver.
 *
 * A measurement, not state: the plan is a pure function of the root's box,
 * read through useSyncExternalStore so no component keeps React state. The
 * root is measured on subscribe and on every resize; a resize that lands on
 * the same plan renders nothing.
 *
 * The root also wears the plan's art pixel: `data-px` and `--zone-px` (a
 * length, so every stylesheet below multiplies its art-px tokens by it).
 * Writing a custom property is a DOM mechanic, as useZoneFit's is; it
 * renders nothing.
 */
import { useCallback, useRef, useSyncExternalStore, type RefObject } from "react"
import { ZONE_ART_PX_LEAST } from "./plan/art-px"
import { plan, type ZonePlan } from "./plan/zone-select"

interface Store {
  current: ZonePlan
  readonly listeners: Set<() => void>
}

const same = (a: ZonePlan, b: ZonePlan): boolean =>
  (Object.keys(a) as (keyof ZonePlan)[]).every(key => a[key] === b[key])

export function useZonePlan(root: RefObject<HTMLElement | null>): ZonePlan {
  const store = useRef<Store | null>(null)
  if (store.current === null) store.current = { current: plan(0, 0), listeners: new Set() }

  const subscribe = useCallback((listener: () => void) => {
    const current = store.current!
    current.listeners.add(listener)
    const element = root.current
    const measure = (width: number, height: number) => {
      const next = plan(width, height)
      if (element !== null) writeArtPx(element, next.px)
      if (same(next, current.current)) return
      current.current = next
      for (const notify of [...current.listeners]) notify()
    }
    if (element !== null) measure(element.clientWidth, element.clientHeight)
    const observer = element !== null && typeof ResizeObserver !== "undefined"
      ? new ResizeObserver(entries => {
        for (const entry of entries) measure(entry.contentRect.width, entry.contentRect.height)
      })
      : undefined
    if (observer !== undefined && element !== null) observer.observe(element)
    return () => {
      current.listeners.delete(listener)
      observer?.disconnect()
    }
  }, [root])
  const getSnapshot = useCallback(() => store.current!.current, [])
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

/** The root's art pixel, as the stylesheets read it. */
function writeArtPx(element: HTMLElement, px: number): void {
  element.dataset.px = String(px)
  element.style.setProperty("--zone-px", `${px}px`)
}

/**
 * The art pixel an element stands on, read from its computed `--zone-px`
 * (registered as a length, so it computes to `3px`), for a hook that
 * measures its own box in CSS px and plans in art px. 1 where nothing is
 * computed, as in a test DOM.
 */
export function zoneArtPxOf(element: Element): number {
  const view = element.ownerDocument.defaultView
  if (view === null) return ZONE_ART_PX_LEAST
  const value = Number.parseFloat(view.getComputedStyle(element).getPropertyValue("--zone-px"))
  return Number.isFinite(value) && value >= ZONE_ART_PX_LEAST ? Math.round(value) : ZONE_ART_PX_LEAST
}
