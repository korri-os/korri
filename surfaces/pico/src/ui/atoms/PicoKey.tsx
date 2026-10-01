import "../../pico-motion.css"
import "./PicoKey.css"
/**
 * One key on the on-screen keyboard.
 *
 * A real button, not legacy's decorative span: legacy's keyboard drew keys and
 * typed nothing, which looks identical in a screenshot and is useless under a
 * thumb. The accessible name says what pressing does rather than repeating the
 * glyph, so a screen reader announces "Type S" instead of "S".
 *
 * A key that switches a mode (capitals) passes `lit`: the key is a toggle,
 * says so to assistive technology, and is drawn green while the mode is on.
 */
export function PicoKey({
  cap,
  label,
  wide = false,
  lit,
  onPress,
}: {
  readonly cap: string
  readonly label: string
  readonly wide?: boolean
  /** Present only on a toggle: whether its mode is on. */
  readonly lit?: boolean
  readonly onPress: () => void
}) {
  return (
    <button
      aria-label={label}
      aria-pressed={lit}
      className="pico-key"
      data-lit={lit === true ? "true" : undefined}
      data-wide={wide ? "true" : undefined}
      onClick={onPress}
      type="button"
    >
      {cap}
    </button>
  )
}
