/**
 * A selectable token: one collection to narrow the library to.
 *
 * Pressed state is `aria-pressed` rather than a class, so the control announces
 * whether it is on without the caller having to add a label for it.
 */
export function PicoChip({
  label,
  pressed,
  onPress,
}: {
  readonly label: string
  readonly pressed: boolean
  readonly onPress: () => void
}) {
  return (
    <button aria-pressed={pressed} className="pico-chip" onClick={onPress} type="button">
      {label}
    </button>
  )
}
