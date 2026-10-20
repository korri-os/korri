/**
 * A take of M4 (the dialog opening and closing) for Caliper and the motion
 * recorder. Fixture code: the product hears no key.
 *
 *  - `t` (or `T`) opens the layer's dialog when it is shut and shuts it
 *    when it is open, so a recorded key sequence plays M4 both ways.
 *  - `window.seek(ms)` holds every running animation on the page at `ms`
 *    (the Web Animations API's currentTime), so a reviewer can hold any
 *    frame of the open or the close; `window.play()` lets them run again.
 *
 * The open state lives in a module store read with useSyncExternalStore:
 * no React state.
 */
import { useEffect, useSyncExternalStore } from "react"

let open = true
const listeners = new Set<() => void>()
const subscribe = (listener: () => void) => {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

declare global {
  interface Window {
    seek?: (ms: number) => void
    play?: () => void
  }
}

/** Whether the take's dialog is open; `t` toggles it. */
export function useFixtureTake(): boolean {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "t" && event.key !== "T") return
      open = !open
      for (const listener of listeners) listener()
    }
    window.seek = (ms: number) => {
      for (const animation of document.getAnimations()) {
        animation.pause()
        animation.currentTime = ms
      }
    }
    window.play = () => {
      for (const animation of document.getAnimations()) animation.play()
    }
    document.addEventListener("keydown", onKey)
    return () => document.removeEventListener("keydown", onKey)
  }, [])
  return useSyncExternalStore(subscribe, () => open)
}
