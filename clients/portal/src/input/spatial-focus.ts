/**
 * Directional focus movement — the host half of "surfaces are focus-driven".
 *
 * A surface renders native focusable controls and reacts to focus; deciding
 * which control a press moves to is the host's job, because only the host knows
 * which devices exist and how they map. This adapter turns semantic directions
 * into real DOM focus and a semantic confirm into a real click on the focused
 * control, so a surface never handles a key, a button index, or a coordinate.
 *
 * Selection is geometric, not DOM-order: the nearest candidate in the pressed
 * direction wins, with distance along that axis dominating so a long rail does
 * not jump to a nearer element on another row.
 */
import type { InputBus } from "./bus"
import type { Direction, InputSource } from "./types"

const FOCUSABLE_SELECTOR =
  "a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])"

/** Off-axis distance is penalised so movement stays in the pressed direction. */
const OFF_AXIS_PENALTY = 3

/** Semantic DOM event understood by focused choice/range controls. */
const SEMANTIC_DIRECTION_EVENT = "korri-semantic-direction"
const SEMANTIC_DIRECTION_END_EVENT = "korri-semantic-direction-end"

function delegateHorizontalDirection(
  direction: Direction,
  repeat: boolean,
  releaseExpected: boolean,
  source: InputSource | undefined,
  gestureId: number | undefined,
): boolean {
  if (direction !== "left" && direction !== "right") return false
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) return false
  if (!active.hasAttribute("data-korri-horizontal-control")) return false
  active.dispatchEvent(
    new CustomEvent(SEMANTIC_DIRECTION_EVENT, {
      detail: { direction, repeat, releaseExpected, source, gestureId },
    }),
  )
  return true
}

function delegateHorizontalDirectionEnd(
  direction: Direction,
  source: InputSource | undefined,
  gestureId: number | undefined,
): boolean {
  if (direction !== "left" && direction !== "right") return false
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) return false
  if (!active.hasAttribute("data-korri-horizontal-control")) return false
  active.dispatchEvent(
    new CustomEvent(SEMANTIC_DIRECTION_END_EVENT, {
      detail: { direction, source, gestureId },
    }),
  )
  return true
}

function isAvailable(element: HTMLElement): boolean {
  if (element.matches(":disabled")) return false
  if (element.closest('[hidden], [inert], [aria-hidden="true"]')) return false
  for (
    let ancestor: HTMLElement | null = element;
    ancestor;
    ancestor = ancestor.parentElement
  ) {
    const style = getComputedStyle(ancestor)
    if (
      style.display === "none" ||
      style.visibility === "hidden" ||
      style.visibility === "collapse"
    ) return false
  }
  return true
}

function isVisible(element: HTMLElement): boolean {
  const rect = element.getBoundingClientRect()
  return rect.width > 0 && rect.height > 0 && isAvailable(element)
}

function center(element: Element): { x: number; y: number } {
  const rect = element.getBoundingClientRect()
  return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
}

/**
 * A focus trap: when the focused element sits inside a container marked
 * `data-block-exit` (Shift's sheet), candidates outside it are ignored so
 * directional input cannot wander onto the surface behind an open panel.
 */
function scopeFor(active: Element | null): ParentNode {
  const blocked = active?.closest("[data-block-exit]")
  if (blocked) return blocked
  // A removed control loses its trap ancestry. Recover inside the last visible
  // (including nested) panel, never on a control behind it, even if it is empty.
  if (!active) {
    const panels = Array.from(
      document.querySelectorAll<HTMLElement>("[data-block-exit]"),
    )
    return panels.reverse().find(isVisible) ?? document
  }
  return document
}

export function focusInDirection(direction: Direction): boolean {
  const active =
    document.activeElement instanceof HTMLElement &&
    document.activeElement !== document.body
      ? document.activeElement
      : null

  const scope = scopeFor(active)
  const candidates = Array.from(
    scope.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
  ).filter(element => element !== active && isVisible(element))
  if (candidates.length === 0) return false

  // Match the legacy focus engine: initial focus uses the browser's native
  // reveal, while directional moves below explicitly own nearest-edge scroll.
  if (!active) {
    candidates[0]?.focus()
    return true
  }

  const from = center(active)
  let best: { element: HTMLElement; score: number } | null = null

  for (const candidate of candidates) {
    const to = center(candidate)
    const dx = to.x - from.x
    const dy = to.y - from.y
    const along =
      direction === "left" ? -dx
      : direction === "right" ? dx
      : direction === "up" ? -dy
      : dy
    // Must actually lie in the pressed direction. The epsilon keeps elements
    // that merely share an edge from counting as "ahead".
    if (along <= 1) continue
    const across =
      direction === "left" || direction === "right"
        ? Math.abs(dy)
        : Math.abs(dx)
    const score = along + across * OFF_AXIS_PENALTY
    if (!best || score < best.score) best = { element: candidate, score }
  }

  if (!best) return false
  // This is the legacy non-Mario-camera path. Suppress the browser's implicit
  // focus scroll, then reveal the chosen control exactly once with nearest-edge
  // behavior so grids and shelves follow controller selection without jumping.
  best.element.focus({ preventScroll: true })
  best.element.scrollIntoView({ block: "nearest", inline: "nearest" })
  return true
}

/**
 * Wire directional movement and confirm to the focused control. Returns a
 * disposer. Confirm clicks rather than dispatching a synthetic key so a plain
 * `<button>` in any surface responds without extra wiring.
 */
export function createSpatialFocusController(bus: InputBus): () => void {
  const offDirection = bus.onAction("direction", action => {
    if (delegateHorizontalDirection(
      action.direction,
      action.repeat ?? false,
      action.releaseExpected ?? false,
      action.source,
      action.gestureId,
    )) return
    focusInDirection(action.direction)
  })
  const offDirectionEnd = bus.onAction("direction-end", action => {
    delegateHorizontalDirectionEnd(action.direction, action.source, action.gestureId)
  })
  const offConfirm = bus.onAction("confirm", () => {
    const active = document.activeElement
    if (active && active !== document.body && active !== document.documentElement) {
      // Preserve editable controls and custom focus owners; recovery is only
      // for document focus lost at an unmount, not a new surface autofocus rule.
      if (active instanceof HTMLElement && isAvailable(active)) active.click()
      return
    }
    const candidate = Array.from(
      scopeFor(null).querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
    ).find(isVisible)
    if (!candidate) return
    candidate.focus()
    // Focus handlers can replace the page. Do not activate stale DOM or replay
    // this press on its replacement. Native click capture still owns wake-only
    // overlays, exactly as it does when confirm starts with a focused control.
    if (document.activeElement === candidate && isVisible(candidate)) {
      candidate.click()
    }
  })
  return () => {
    offDirection()
    offDirectionEnd()
    offConfirm()
  }
}
