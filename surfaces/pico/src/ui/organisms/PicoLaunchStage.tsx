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
  phase = "starting",
}: {
  readonly kicker: string
  readonly detail?: string
  readonly gameTitle?: string
  readonly cart?: PicoLaunchCart
  readonly phase?: "starting" | "running"
}) {
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
    </section>
  )
}
