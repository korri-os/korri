import type { PicoOverlayControlView } from "../../pico-overlay-view"
import { PicoRow } from "../atoms/PicoRow"

/**
 * One control Korri offers: a menu row, its current state on the right, and
 * underneath it the reason it cannot be used or what it will do.
 *
 * A disabled control stays in the list with Korri's reason. Hiding it would
 * leave the user wondering where it went.
 */
export function PicoControlRow({
  control,
  onActivate,
}: {
  readonly control: PicoOverlayControlView
  readonly onActivate: () => void
}) {
  const note = !control.enabled && control.disabledReason !== undefined
    ? control.disabledReason
    : control.description
  return (
    <li className="pico-control-row">
      <PicoRow
        danger={control.destructive}
        detail={control.stateLabel === undefined ? undefined : `◀ ${control.stateLabel} ▶`}
        disabled={!control.enabled}
        label={control.label}
        onPress={onActivate}
      />
      {note === undefined ? null : <p className="pico-control-row-reason">{note}</p>}
    </li>
  )
}
