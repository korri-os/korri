import { type KeyboardEvent, type RefObject, useEffect, useRef } from "react"

/* The portal delivers left and right to the focused element marked
 * data-korri-horizontal-control as these DOM events instead of moving focus
 * (clients/portal/src/input/spatial-focus.ts). Arrow keys are handled too,
 * for a keyboard with no portal in front of it. A surface never sees a key
 * from a device. */
const DIRECTION_EVENT = "korri-semantic-direction"

export const wayOf = (direction: unknown): -1 | 1 | undefined =>
  direction === "left" || direction === "ArrowLeft" ? -1
    : direction === "right" || direction === "ArrowRight" ? 1
    : undefined

/**
 * Left and right on a control that steps a value in place: a choice, a
 * toggle, a range. While `active`, the element is marked for the portal and
 * each left or right calls `onStep`. Returns the attribute to spread on the
 * element and a key handler for the no-portal case.
 */
export function useZoneHorizontal<E extends HTMLElement>(
  node: RefObject<E | null>,
  active: boolean,
  kind: string,
  onStep: ((way: -1 | 1) => void) | undefined,
): {
  readonly "data-korri-horizontal-control": string | undefined
  readonly onKeyDown: ((event: KeyboardEvent<E>) => void) | undefined
} {
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

  return {
    "data-korri-horizontal-control": active ? kind : undefined,
    onKeyDown: active
      ? (event: KeyboardEvent<E>) => {
          const way = wayOf(event.key)
          if (way === undefined) return
          event.preventDefault()
          step.current?.(way)
        }
      : undefined,
  }
}
