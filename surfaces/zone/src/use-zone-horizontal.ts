import { type RefObject, useEffect, useRef } from "react"

/* The portal delivers left and right to the focused element marked
 * data-korri-horizontal-control as these DOM events instead of moving focus
 * (clients/portal/src/input/spatial-focus.ts). The surface never reads a
 * key: translating devices, their repeat and their release is the portal's. */
const DIRECTION_EVENT = "korri-semantic-direction"

const wayOf = (direction: unknown): -1 | 1 | undefined =>
  direction === "left" ? -1 : direction === "right" ? 1 : undefined

/**
 * Left and right on a control that steps a value in place: a choice, a
 * toggle, a range. While `active`, the element is marked for the portal and
 * each `korri-semantic-direction` left or right calls `onStep`. Returns the
 * attribute to spread on the element.
 */
export function useZoneHorizontal<E extends HTMLElement>(
  node: RefObject<E | null>,
  active: boolean,
  kind: string,
  onStep: ((way: -1 | 1) => void) | undefined,
): { readonly "data-korri-horizontal-control": string | undefined } {
  const step = useRef(onStep)
  useEffect(() => {
    step.current = onStep
  })

  useEffect(() => {
    const element = node.current
    if (element === null || !active) return
    const onDirection = (event: Event) => {
      const way = wayOf((event as CustomEvent<{ readonly direction?: string }>).detail?.direction)
      if (way !== undefined) step.current?.(way)
    }
    element.addEventListener(DIRECTION_EVENT, onDirection)
    return () => element.removeEventListener(DIRECTION_EVENT, onDirection)
  }, [node, active])

  return { "data-korri-horizontal-control": active ? kind : undefined }
}
