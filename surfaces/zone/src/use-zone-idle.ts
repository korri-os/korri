/**
 * The idle timer: a subscription, not state.
 *
 * Every change to the navigation restarts the clock; a key, a pointer or a
 * focus move on the screen restarts it too. After ZONE_IDLE_AFTER_MS with
 * none of those, IdleElapsed is dispatched and `update` decides whether the
 * idle layer may show.
 *
 * Waking keeps the cursor exactly where it was. A focus move is already a
 * message (FocusedCard and the like), and `update` treats any message while
 * idle as a wake and nothing more, so a focus move here wakes nothing of its
 * own: a Woke sent first would let the move land. A key wakes on the next
 * task, after the host has turned it into a focus move (or into nothing),
 * for the same reason. A pointer wakes at once.
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
    let wake: ReturnType<typeof setTimeout> | undefined
    const restart = () => {
      clearTimeout(timer)
      timer = setTimeout(fire, ZONE_IDLE_AFTER_MS)
    }
    const key = () => {
      restart()
      if (idle) wake = setTimeout(() => dispatch({ _tag: "Woke" }), 0)
    }
    const pointer = () => {
      restart()
      if (idle) dispatch({ _tag: "Woke" })
    }
    element.addEventListener("keydown", key)
    element.addEventListener("pointerdown", pointer)
    element.addEventListener("focusin", restart)
    return () => {
      clearTimeout(timer)
      clearTimeout(wake)
      element.removeEventListener("keydown", key)
      element.removeEventListener("pointerdown", pointer)
      element.removeEventListener("focusin", restart)
    }
  }, [root, nav, idle, dispatch])
}
