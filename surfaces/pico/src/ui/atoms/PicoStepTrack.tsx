import "./PicoStepTrack.css"

/**
 * Where one step stands. `skipped` is a step passed without being done, such
 * as Wi-Fi left offline: it is behind the player, but it is not complete.
 */
export type PicoStepState = "done" | "skipped" | "now" | "next"

export interface PicoStep {
  readonly label: string
  readonly state: PicoStepState
}

/**
 * Where a sequence stands: one segment per step. A done step's bar is solid
 * green; a skipped one is broken orange dashes, so it reads as passed but not
 * done without colour alone; the current one is in the accent; the rest are
 * on the plate. Each step names itself while its segment has room for the
 * word and keeps only its number when it does not; the status bar names the
 * current step anyway.
 *
 * Read as one phrase, like PicoTally: "Step 3 of 7, NAME. Skipped: WI-FI".
 */
export function PicoStepTrack({ steps }: { readonly steps: readonly PicoStep[] }) {
  const at = steps.findIndex((step) => step.state === "now")
  const skipped = steps.filter((step) => step.state === "skipped").map((step) => step.label)
  const where = at < 0 ? `${steps.length} steps` : `Step ${at + 1} of ${steps.length}, ${steps[at]?.label ?? ""}`
  return (
    <span
      aria-label={skipped.length === 0 ? where : `${where}. Skipped: ${skipped.join(", ")}`}
      className="pico-step-track"
      role="img"
    >
      {steps.map((step, index) => (
        <span aria-hidden className="pico-step-track-step" data-state={step.state} key={step.label}>
          <span className="pico-step-track-name">
            <b className="pico-step-track-number">{index + 1}</b>
            <span className="pico-step-track-label">{step.label}</span>
          </span>
        </span>
      ))}
    </span>
  )
}
