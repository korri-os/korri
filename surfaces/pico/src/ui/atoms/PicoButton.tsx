/**
 * A pressable control.
 *
 * The label is a string rather than children: every button in a bitmap
 * interface is a short word, and a string is a prop a design tool can actually
 * drive — children would leave this atom with no controls at all.
 *
 * The focus ring is not decoration here. Pico is driven by a d-pad through DOM
 * focus, so the ring is the cursor: it is styled, never removed, and it is the
 * one thing about this atom that must survive any restyle.
 */
export function PicoButton({
  label,
  onPress,
}: {
  readonly label: string
  readonly onPress: () => void
}) {
  return (
    <button className="pico-button" onClick={onPress} type="button">
      {label}
    </button>
  )
}
