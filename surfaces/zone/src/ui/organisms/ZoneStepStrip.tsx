import "./ZoneStepStrip.css"
import { ZoneBanner } from "../atoms/ZoneBanner"

/** Where a step stands against the one you are on. */
export type ZoneStepState = "done" | "here" | "ahead"

/** A pip's diamond, 5 art px across inside its black outline: on (gold) and off (the ground). */
const PIP_EDGE = "M3 0h1v1h-1zM2 1h1v1h-1zM4 1h1v1h-1zM1 2h1v1h-1zM5 2h1v1h-1zM0 3h1v1h-1zM6 3h1v1h-1zM1 4h1v1h-1zM5 4h1v1h-1zM2 5h1v1h-1zM4 5h1v1h-1zM3 6h1v1h-1z"
const PIP_FACE = "M3 1h1v1h-1zM2 2h3v1h-3zM1 3h5v1h-5zM2 4h3v1h-3zM3 5h1v1h-1z"

/**
 * First boot's step strip: the one banner names the step you are on
 * (CONTROLLERS, WI-FI, NAME, IDENTITY), and under its lip four diamond
 * pips count the way, gold for the step you are on and the steps done,
 * outlined black for the rest. The pips hang under the banner and take no
 * room in the strip; the steps' count is read out with them.
 */
export function ZoneStepStrip({ steps, current }: {
  /** The steps' names, in order. */
  readonly steps: readonly string[]
  /** The step you are on, from 0. */
  readonly current: number
}) {
  const here = Math.min(Math.max(0, current), Math.max(0, steps.length - 1))
  const stateOf = (index: number): ZoneStepState => index < here ? "done" : index === here ? "here" : "ahead"
  return (
    <span aria-label="Setup steps" className="zone-step-strip" role="group">
      <ZoneBanner>{steps[here] ?? ""}</ZoneBanner>
      <span aria-label={`Step ${here + 1} of ${steps.length}`} className="zone-step-strip-pips" role="img">
        {steps.map((step, index) => (
          <svg aria-hidden="true" className="zone-step-pip" data-state={stateOf(index)} key={step} viewBox="0 0 7 7">
            <path className="zone-step-pip-face" d={PIP_FACE} />
            <path className="zone-step-pip-edge" d={PIP_EDGE} />
          </svg>
        ))}
      </span>
    </span>
  )
}
