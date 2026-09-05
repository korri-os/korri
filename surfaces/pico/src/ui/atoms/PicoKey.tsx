/**
 * One key on the on-screen keyboard.
 *
 * A real button, not legacy's decorative span: legacy's keyboard drew keys and
 * typed nothing, which looks identical in a screenshot and is useless under a
 * thumb. The accessible name says what pressing does rather than repeating the
 * glyph, so a screen reader announces "Type S" instead of "S".
 */
export function PicoKey({
  cap,
  label,
  wide = false,
  onPress,
}: {
  readonly cap: string
  readonly label: string
  readonly wide?: boolean
  readonly onPress: () => void
}) {
  return (
    <button
      aria-label={label}
      className="pico-key"
      data-wide={wide ? "true" : undefined}
      onClick={onPress}
      type="button"
    >
      {cap}
    </button>
  )
}
