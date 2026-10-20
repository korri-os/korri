import "./PicoGameActions.css"
import type { SurfaceAction } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { PicoControlRow } from "../molecules/PicoControlRow"

/**
 * What Korri says can be done to this game, beside starting it.
 *
 * Absent entirely when Korri offers none — most games have none, and an empty
 * ACTIONS heading on every screen teaches the user to stop reading it.
 *
 * Reuses the overlay's control row rather than growing a second row that looks
 * the same. One difference is deliberate: a game action carries no
 * `disabledReason` — that field belongs to gameplay controls — so an inert
 * action is dimmed and explains itself through its description, or not at all.
 * Writing a reason here would be Pico inventing one.
 *
 * Korri replaces actions as their work moves on: DOWNLOAD becomes CANCEL
 * DOWNLOAD the moment it is pressed. The cursor stays where the person left
 * it, on the row that took the pressed one's place or the nearest one,
 * because with no cursor the host's next confirm would press nothing.
 */
export function PicoGameActions({
  actions,
  onRun,
}: {
  readonly actions: readonly SurfaceAction[]
  readonly onRun: (action: SurfaceAction) => void
}) {
  const list = useRef<HTMLUListElement>(null)
  /** The row the cursor was last on, and whether the cursor was in the list. */
  const lastRow = useRef(0)
  const holding = useRef(false)
  useLayoutEffect(() => {
    const root = list.current
    if (root === null || !holding.current) return
    const active = root.ownerDocument.activeElement
    const lost = active === root.ownerDocument.body || active === null || !active.isConnected ||
      (root.contains(active) && active.matches(":disabled"))
    if (!lost) return
    const rows = [...root.children]
    const enabled = (row: Element) => row.querySelector<HTMLButtonElement>("button:not([disabled])")
    const at = rows.slice(lastRow.current).map(enabled).find(button => button !== null)
    const above = rows.slice(0, lastRow.current).reverse().map(enabled).find(button => button !== null)
    ;(at ?? above)?.focus()
  })
  if (actions.length === 0) return null
  return (
    <section aria-label="Actions" className="pico-game-actions">
      <h2 className="pico-game-actions-title">ACTIONS</h2>
      <ul
        className="pico-game-actions-list"
        onBlur={event => {
          // Focus that went somewhere else is the person's move; focus that
          // went nowhere went with a row Korri replaced. A question a row
          // asked (Remove download?) hands the cursor back when it closes.
          const next = event.relatedTarget
          if (next === null || event.currentTarget.contains(next)) return
          if (next instanceof Element && next.closest("[role='dialog']") !== null) return
          holding.current = false
        }}
        onFocus={event => {
          holding.current = true
          const row = event.target.closest("li")
          if (row !== null) lastRow.current = [...event.currentTarget.children].indexOf(row)
        }}
        ref={list}
      >
        {actions.map((action) => (
          <PicoControlRow
            control={{
              id: action.id,
              label: action.label,
              ...(action.description === undefined ? {} : { description: action.description }),
              enabled: action.enabled,
              destructive: action.destructive === true,
            }}
            key={action.id}
            /* A game action is a command; it is never a range. */
            onActivate={() => onRun(action)}
          />
        ))}
      </ul>
    </section>
  )
}
