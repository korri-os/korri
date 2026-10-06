import "../../pico-motion.css"
import "./PicoLaunchStage.css"
import { useLayoutEffect, useRef } from "react"
import { PicoRow } from "../atoms/PicoRow"
import type { PicoLaunchCart } from "../../pico-screen-view"
import { PicoCart } from "../molecules/PicoCart"

/** Cells in the meter: one per palette colour, the way PICO-8 counts. */
const METER = Array.from({ length: 16 }, (_, index) => index)

/**
 * The seconds between a press and a game, and the screen while one runs.
 *
 * Starting, the cartridge is pushed down into a slot at the bottom of the
 * screen while a sixteen-cell meter fills. Running, the cart sits seated and
 * the meter is full. The meter is decoration, not progress: Korri publishes no
 * percentage, so the cells fill on a clock and every fact on screen is text
 * Korri wrote. Without a known game there is no cartridge to seat, and the
 * slot stays empty rather than showing a stand-in.
 */
export function PicoLaunchStage({
  kicker,
  detail,
  gameTitle,
  cart,
  actions,
  phase = "starting",
}: {
  readonly kicker: string
  readonly detail?: string
  readonly gameTitle?: string
  readonly cart?: PicoLaunchCart
  readonly actions?: readonly {
    readonly id: string
    readonly label: string
    /** Where this action's work stands, in Korri's words, shown under it. */
    readonly detail?: string
    readonly disabled: boolean
    readonly onPress: () => void
  }[]
  readonly phase?: "starting" | "running"
}) {
  const controls = useRef<HTMLUListElement>(null)
  /** The row the cursor was last on, so a lost cursor returns near it. */
  const lastRow = useRef(0)
  // Seed the cursor, and put it back when the focused row leaves the list or
  // becomes disabled. Focus that is still on screen stays where the user put it.
  // Without a cursor, the host's next confirm would press a row nobody saw.
  useLayoutEffect(() => {
    const root = controls.current
    if (root === null) return
    const active = root.ownerDocument.activeElement
    const lost = active === root.ownerDocument.body || active === null || !active.isConnected ||
      (root.contains(active) && active.matches(":disabled"))
    if (!lost) return
    const rows = [...root.children]
    const enabled = (row: Element) => row.querySelector<HTMLButtonElement>("button:not([disabled])")
    // The next row down takes the cursor, as in any list; else the nearest above.
    const below = rows.slice(lastRow.current).map(enabled).find(button => button !== null)
    const above = rows.slice(0, lastRow.current).reverse().map(enabled).find(button => button !== null)
    ;(below ?? above)?.focus()
  })
  return (
    <section aria-live="polite" className="pico-launch-stage" data-phase={phase}>
      <div className="pico-launch-stage-words">
        <h1 className="pico-launch-stage-kicker">{kicker}</h1>
        <span aria-hidden className="pico-launch-stage-meter">
          {METER.map((cell) => (
            <i className="pico-launch-stage-cell" data-cell={cell} key={cell} />
          ))}
        </span>
        {gameTitle === undefined ? null : (
          <p className="pico-launch-stage-game">{gameTitle}</p>
        )}
        {detail === undefined ? null : (
          <p className="pico-launch-stage-detail">{detail}</p>
        )}
      </div>
      <div className="pico-launch-stage-slot">
        {cart === undefined ? null : (
          <span className="pico-launch-stage-cart">
            <PicoCart artUrl={cart.artUrl} id={cart.id} placement="still" title={cart.title} />
          </span>
        )}
        <span aria-hidden className="pico-launch-stage-mouth" />
      </div>
      {actions === undefined || actions.length === 0 ? null : (
        <ul
          className="pico-launch-stage-actions"
          ref={controls}
          onFocus={event => {
            const row = event.target.closest("li")
            if (row !== null) lastRow.current = [...event.currentTarget.children].indexOf(row)
          }}
        >
          {/* Where each piece of work stands goes under its row, like a
            * control's note, so a narrow screen never squeezes the name. */}
          {actions.map(action => (
            <li className="pico-launch-stage-choice" key={action.id}>
              <PicoRow label={action.label} disabled={action.disabled} onPress={action.onPress} />
              {action.detail === undefined ? null : (
                <p className="pico-launch-stage-note">{action.detail}</p>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
