/**
 * The idle timer: a subscription, not state.
 *
 * Every change to the navigation restarts the clock; a key, a pointer or a
 * focus move on the screen restarts it too, and wakes the idle layer if it
 * is showing. After ZONE_IDLE_AFTER_MS with none of those, IdleElapsed is
 * dispatched and `update` decides whether the idle layer may show.
 *
 * Known gap: a host button that changes nothing (menu while already on the
 * tabs) restarts nothing, because it reaches the model and not the DOM.
 */
import { useEffect, type RefObject } from "react"
import type { ZoneMessage } from "./state/messages"
import { ZONE_IDLE_AFTER_MS, type ZoneNavigation } from "./state/navigation"

export function useZoneIdle(
  root: RefObject<HTMLElement | null>,
  nav: ZoneNavigation,
  dispatch: (message: ZoneMessage) => void,
): void {
  const idle = nav.layers.at(-1) === "idle"
  useEffect(() => {
    const element = root.current
    if (element === null) return
    const fire = () => dispatch({ _tag: "IdleElapsed" })
    let timer = setTimeout(fire, ZONE_IDLE_AFTER_MS)
    const touch = () => {
      clearTimeout(timer)
      timer = setTimeout(fire, ZONE_IDLE_AFTER_MS)
      if (idle) dispatch({ _tag: "Woke" })
    }
    element.addEventListener("keydown", touch)
    element.addEventListener("pointerdown", touch)
    element.addEventListener("focusin", touch)
    return () => {
      clearTimeout(timer)
      element.removeEventListener("keydown", touch)
      element.removeEventListener("pointerdown", touch)
      element.removeEventListener("focusin", touch)
    }
  }, [root, nav, idle, dispatch])
}
