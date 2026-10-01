import "./PicoControlRow.css"
import { picoRangeLabel, type PicoOverlayControlView } from "../../pico-overlay-view"
import { usePicoRange } from "../../use-pico-range"
import { PicoRow } from "../atoms/PicoRow"
import { PicoSlider } from "../atoms/PicoSlider"

/**
 * One control Korri offers: a menu row, its current state on the right, and
 * underneath it the reason it cannot be used or what it will do.
 *
 * A range is a slider instead of a row: left and right adjust it, and Korri
 * hears the new value once the adjustment settles (see usePicoRange).
 *
 * A disabled control stays in the list with Korri's reason. Hiding it would
 * leave the user wondering where it went.
 */
export function PicoControlRow({
  control,
  onActivate,
  onAdjust,
}: {
  readonly control: PicoOverlayControlView
  readonly onActivate: () => void
  /** A range settled on a new value. */
  readonly onAdjust: (value: number) => void
}) {
  const range = usePicoRange(control.range, control.enabled, onAdjust)
  const note = !control.enabled && control.disabledReason !== undefined
    ? control.disabledReason
    : control.description
  const value = range.value ?? control.range?.value
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
          onLeave={range.flush}
          onRelease={range.release}
          onStep={range.step}
          step={control.range.step}
          value={value}
          valueText={picoRangeLabel(value, control.range)}
        />
      )}
      {note === undefined ? null : <p className="pico-control-row-reason">{note}</p>}
    </li>
  )
}
