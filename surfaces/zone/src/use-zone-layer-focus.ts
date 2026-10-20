/**
 * Focus into a layer when it opens, and back to where it was when it closes.
 *
 * Zone's models hold which control of a layer is lit (an index or an id);
 * the layer's shapes draw it with `data-focused`. This hook is the DOM half:
 * whenever `litKey` changes while the layer is open, it puts DOM focus on the
 * lit control inside `layer`, so the host's d-pad starts from there. While
 * the layer is closed it remembers the last control focused outside it, and
 * with `returns` it gives focus back to that control when the layer closes
 * or unmounts, if the control is still on screen and not inert.
 *
 * `litKey` undefined means the layer is closed (or has nothing to hold
 * focus). A ref and two listeners: no React state.
 */
import { type RefObject, useEffect, useLayoutEffect, useRef } from "react"

/** The control a layer draws as lit: `data-focused`, but not `"off"`. */
const LIT = "[data-focused]:not([data-focused=\"off\"]):not([data-focused=\"false\"])"
const FOCUSABLE = "button, [tabindex]"

function litControl(layer: HTMLElement): HTMLElement | undefined {
  const lit = layer.querySelector<HTMLElement>(LIT)
  if (lit === null) return undefined
  return lit.matches(FOCUSABLE) ? lit : lit.querySelector<HTMLElement>(FOCUSABLE) ?? undefined
}

function giveBack(target: HTMLElement | null): void {
  if (target !== null && target.isConnected && target.closest("[inert]") === null) target.focus()
}

export function useZoneLayerFocus(
  layer: RefObject<HTMLElement | null>,
  litKey: string | undefined,
  returns = false,
): void {
  const prior = useRef<HTMLElement | null>(null)
  const wasOpen = useRef(false)
  const open = litKey !== undefined

  // While closed, remember the last control focused outside the layer.
  useEffect(() => {
    if (open) return
    const remember = (event: FocusEvent) => {
      const target = event.target
      if (target instanceof HTMLElement && layer.current?.contains(target) !== true) prior.current = target
    }
    document.addEventListener("focusin", remember)
    return () => document.removeEventListener("focusin", remember)
  }, [open, layer])

  // Open, or the lit control moved: focus follows the model.
  useLayoutEffect(() => {
    const node = layer.current
    if (!open || node === null) return
    const active = document.activeElement
    if (!wasOpen.current && active instanceof HTMLElement && active !== document.body && !node.contains(active)) {
      prior.current = active
    }
    wasOpen.current = true
    const control = litControl(node)
    if (control !== undefined && document.activeElement !== control) control.focus()
  }, [open, litKey, layer])

  // Closed after being open: focus goes back.
  useLayoutEffect(() => {
    if (open || !wasOpen.current) return
    wasOpen.current = false
    if (returns) giveBack(prior.current)
  }, [open, returns])

  useEffect(() => () => {
    if (returns && wasOpen.current) giveBack(prior.current)
  }, [returns])
}
