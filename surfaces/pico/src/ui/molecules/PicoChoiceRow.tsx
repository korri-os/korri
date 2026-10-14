import "./PicoChoiceRow.css"
import { PicoRow } from "../atoms/PicoRow"

/**
 * A choice with a line under it that says what choosing it means. The note
 * takes the quiet ink of whatever it sits on: a card's note colour inside a
 * card, the quiet ink on the ground.
 *
 * PicoSettingRow and PicoControlRow draw the same shape, but each is bound to
 * the view it shows; this one takes plain words.
 */
export function PicoChoiceRow({
  label,
  note,
  detail,
  disabled = false,
  onPress,
}: {
  readonly label: string
  readonly note?: string
  /** A short value on the right: what is set now. */
  readonly detail?: string
  readonly disabled?: boolean
  readonly onPress: () => void
}) {
  return (
    <li className="pico-choice-row">
      <PicoRow detail={`${detail === undefined ? "" : `${detail} `}▶`} disabled={disabled} label={label} onPress={onPress} />
      {note === undefined ? null : <p className="pico-choice-row-note">{note}</p>}
    </li>
  )
}
