/**
 * The plan for the screen Zone was given, kept current by a ResizeObserver.
 *
 * A measurement, not state: the plan is a pure function of the root's box,
 * read through useSyncExternalStore so no component keeps React state. The
 * root is measured on subscribe and on every resize; a resize that lands on
 * the same plan renders nothing.
 */
import { useCallback, useRef, useSyncExternalStore, type RefObject } from "react"
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
