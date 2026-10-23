import "../../pico-motion.css"
import "./PicoPlayingNow.css"
import { useEffect, useLayoutEffect, useRef } from "react"
import type { PicoInviteView } from "../../pico-invite-view"
import { PicoInviteRow } from "../molecules/PicoInviteRow"

/**
 * PLAYING NOW: the games playing now on the person's other devices, one row
 * each, with Korri's actions for each. With no invite it draws nothing, so
 * home has no empty state for it.
 *
 * It takes no more than a share of home's height. More rows than fit
 * scroll inside it, a cut row shows there are more, and the host's focus
 * brings the chosen button into view.
 *
 * The cursor never falls off the screen when Korri takes a row away (a
 * dismiss, a session that ended): it goes to the next row down, else the
 * nearest above. When the last row goes, `onLostFocus` tells the owner, so
 * home can give the cursor back to the shelf.
 */
export function PicoPlayingNow({
  invites,
  onAction,
  onLostFocus,
}: {
  readonly invites: readonly PicoInviteView[]
  readonly onAction: (actionId: string) => void
  readonly onLostFocus?: () => void
}) {
  const list = useRef<HTMLUListElement>(null)
  /* Whether the cursor is in this list, and the row it was last on. Refs:
   * both are read in focus events and neither draws anything. */
  const holding = useRef(false)
  const lastRow = useRef(0)

  // Any focus that lands outside the list means the cursor has left it.
  // A removed button moves no focus anywhere, so the flag survives it.
  useEffect(() => {
    const note = (event: FocusEvent) => {
      holding.current = event.target instanceof Node && list.current?.contains(event.target) === true
    }
    document.addEventListener("focusin", note)
    return () => document.removeEventListener("focusin", note)
  }, [])

  useLayoutEffect(() => {
    if (!holding.current) return
    const active = document.activeElement
    const root = list.current
    const lost = active === null || active === document.body || !active.isConnected
      || (root !== null && root.contains(active) && active.matches(":disabled"))
    if (!lost) return
    const rows = root === null ? [] : [...root.children]
    const enabled = (row: Element) => row.querySelector<HTMLButtonElement>("button:not([disabled])")
    const below = rows.slice(lastRow.current).map(enabled).find(button => button !== null)
    const above = rows.slice(0, lastRow.current).reverse().map(enabled).find(button => button !== null)
    const next = below ?? above
    if (next !== undefined && next !== null) {
      next.focus()
      return
    }
    holding.current = false
    onLostFocus?.()
  })

  if (invites.length === 0) return null

  return (
    <section aria-label="Playing now" className="pico-playing-now">
      <h2 className="pico-playing-now-heading">
        <span aria-hidden className="pico-playing-now-live" />
        PLAYING NOW
        {/* How many, so a list cut by its share of the screen says so. */}
        {invites.length < 2 ? null : <span className="pico-playing-now-count">{invites.length} GAMES</span>}
      </h2>
      <ul
        className="pico-playing-now-list"
        onFocus={event => {
          const row = event.target.closest("li")
          if (row !== null) lastRow.current = [...event.currentTarget.children].indexOf(row)
        }}
        ref={list}
      >
        {invites.map(invite => (
          <li className="pico-playing-now-item" key={invite.id}>
            <PicoInviteRow
              actions={invite.actions}
              artUrl={invite.artUrl}
              deviceLabel={invite.deviceLabel}
              gameTitle={invite.gameTitle}
              id={invite.id}
              onAction={onAction}
              personName={invite.personName}
            />
          </li>
        ))}
      </ul>
    </section>
  )
}
