/**
 * Left and right for a part in Caliper, delivered the way the portal
 * delivers them. Fixture code: the product hears no key. The portal's
 * keyboard adapter (clients/portal/src/input/keyboard-adapter.ts) turns
 * ArrowLeft and ArrowRight into a direction, and its spatial focus
 * (clients/portal/src/input/spatial-focus.ts) hands that direction to a
 * focused element marked `data-korri-horizontal-control` as a
 * `korri-semantic-direction` event. A Caliper frame has no portal, so a
 * part with a stepper calls `useFixtureDirections` to do the same.
 */
import { useEffect } from "react"

const DIRECTION_EVENT = "korri-semantic-direction"
const ARROWS: Readonly<Record<string, "left" | "right">> = { ArrowLeft: "left", ArrowRight: "right" }

/** Send the portal's left or right to a stepping control. */
export function sendFixtureDirection(element: Element, direction: "left" | "right", repeat = false): void {
  element.dispatchEvent(new CustomEvent(DIRECTION_EVENT, { detail: { direction, repeat, releaseExpected: false } }))
}

/** While the part is shown, an arrow key on a focused stepper reaches it as the portal's event. */
export function useFixtureDirections(): void {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const direction = ARROWS[event.key]
      const active = document.activeElement
      if (direction === undefined || !(active instanceof HTMLElement)) return
      if (!active.hasAttribute("data-korri-horizontal-control")) return
      event.preventDefault()
      sendFixtureDirection(active, direction, event.repeat)
    }
    document.addEventListener("keydown", onKey)
    return () => document.removeEventListener("keydown", onKey)
  }, [])
}
