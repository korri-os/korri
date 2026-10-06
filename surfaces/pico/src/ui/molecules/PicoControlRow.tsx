import "./PicoControlRow.css"
import {
  picoRangeLabel,
  type PicoOverlayControlView,
  type PicoRangeRelease,
  type PicoRangeStepRequest,
} from "../../pico-overlay-view"
import { PicoRow } from "../atoms/PicoRow"
import { PicoSlider } from "../atoms/PicoSlider"

/**
 * One control Korri offers: a menu row, its current state on the right, and
 * underneath it the reason it cannot be used or what it will do.
 *
 * A range is a slider instead of a row: left and right adjust it. The owner
 * keeps the value until Korri hears it (src/state/overlay.ts), so the slider
 * shows `value` when given and Korri's value otherwise.
 *
 * A disabled control stays in the list with Korri's reason. Hiding it would
 * leave the user wondering where it went.
 */
export function PicoControlRow({
  control,
  value: held,
  onActivate,
  onStep,
  onRelease,
  onLeave,
}: {
  readonly control: PicoOverlayControlView
  /** A range's value ahead of Korri. Absent: Korri's. */
  readonly value?: number
  readonly onActivate: () => void
  /** A range's adjustments. A control without a range needs none. */
  readonly onStep?: (request: PicoRangeStepRequest) => void
  readonly onRelease?: (ended: PicoRangeRelease) => void
  readonly onLeave?: () => void
}) {
  const note = !control.enabled && control.disabledReason !== undefined
    ? control.disabledReason
    : control.description
  const value = held ?? control.range?.value
  return (
    <li className="pico-control-row">
      {control.range === undefined || value === undefined ? (
        <PicoRow
          danger={control.destructive}
          detail={control.stateLabel === undefined ? undefined : `◀ ${control.stateLabel} ▶`}
          disabled={!control.enabled}
          label={control.label}
          onPress={onActivate}
        />
      ) : (
        <PicoSlider
          disabled={!control.enabled}
          label={control.label}
          max={control.range.max}
          min={control.range.min}
          onLeave={() => onLeave?.()}
          onRelease={(ended) => onRelease?.(ended)}
          onStep={(request) => onStep?.(request)}
          step={control.range.step}
          value={value}
          valueText={picoRangeLabel(value, control.range)}
        />
      )}
      {note === undefined ? null : <p className="pico-control-row-reason">{note}</p>}
    </li>
  )
}
