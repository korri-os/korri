/**
 * A d-pad for walking a part in Caliper with a keyboard. Fixture code: the
 * product gets its input from the host.
 */
import { useEffect, useRef } from "react"


type Way = "up" | "down" | "left" | "right"
const WAYS: Readonly<Record<string, Way>> = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" }

const FOCUSABLE = "button:not([disabled]), [tabindex='0'], input, textarea, select"

function visible(element: HTMLElement): boolean {
  const rect = element.getBoundingClientRect()
  return rect.width > 0 && rect.height > 0 && element.closest("[hidden]") === null
}

/** The nearest control in the pressed direction, by the portal's idea of
 * near: far along the way counts less than far across it. */
function nearest(from: HTMLElement, candidates: readonly HTMLElement[], way: Way): HTMLElement | undefined {
  const a = from.getBoundingClientRect()
  const ax = a.left + a.width / 2
  const ay = a.top + a.height / 2
  let best: { readonly element: HTMLElement; readonly score: number } | undefined
  for (const element of candidates) {
    if (element === from) continue
    const b = element.getBoundingClientRect()
    const bx = b.left + b.width / 2
    const by = b.top + b.height / 2
    const along = way === "up" ? a.top - b.bottom : way === "down" ? b.top - a.bottom : way === "left" ? a.left - b.right : b.left - a.right
    const ahead = way === "up" ? by < ay : way === "down" ? by > ay : way === "left" ? bx < ax : bx > ax
    if (!ahead || along < -Math.min(a.height, a.width) / 2) continue
    const across = way === "up" || way === "down"
      ? Math.max(0, b.left - a.right, a.left - b.right)
      : Math.max(0, b.top - a.bottom, a.top - b.bottom)
    const score = Math.max(0, along) + across * 3
    if (best === undefined || score < best.score) best = { element, score }
  }
  return best?.element
}

/**
 * Drive a part the way the portal drives Pico, for walking a preview only:
 * the arrow keys are the d-pad and move focus to the nearest control that
 * way, Enter is A (a focused button clicks itself), and Escape or Backspace
 * is B. Mirrors clients/portal/src/input/keyboard-adapter.ts; the surface
 * itself never sees keys, and nothing outside src/fixtures uses this.
 *
 * A key typed into a native field (the identity dialog's password) stays
 * with the field, except Escape. The frame holds one part, so the whole
 * document is the screen.
 */
export function useFixtureDpad(handlers: { readonly onBack: () => void }): void {
  const latest = useRef(handlers)
  latest.current = handlers
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const typing = event.target instanceof HTMLElement && event.target.matches("input, textarea, select")
      if (event.key === "Escape" || (event.key === "Backspace" && !typing)) {
        event.preventDefault()
        latest.current.onBack()
        return
      }
      const way = WAYS[event.key]
      if (way === undefined || typing) return
      event.preventDefault()
      const candidates = [...document.body.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(visible)
      const active = document.activeElement
      const from = active instanceof HTMLElement && active !== document.body ? active : undefined
      const next = from === undefined ? candidates[0] : nearest(from, candidates, way)
      next?.focus()
    }
    document.addEventListener("keydown", onKey)
    return () => document.removeEventListener("keydown", onKey)
  }, [])
}
