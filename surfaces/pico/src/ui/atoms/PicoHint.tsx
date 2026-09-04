/**
 * The buttons Pico currently names in a hint.
 *
 * Exactly the two the surface uses today. A wider set would offer a design tool
 * options no screen has asked for, and the honest moment to widen it is when a
 * screen needs the third.
 */
export type PicoHintKey = "a" | "b"

/**
 * One button hint: the glyph and what it does here.
 *
 * Hints state what the buttons already do — they are not controls. Pico never
 * makes one clickable, because a hint that can be pressed is a button wearing a
 * hint's clothes, and a handheld user would never find it with a thumb.
 */
export function PicoHint({
  hintKey,
  label,
}: {
  readonly hintKey: PicoHintKey
  readonly label: string
}) {
  return (
    <span className="pico-hint">
      <b className="pico-hint-key">{hintKey.toUpperCase()}</b>
      <span className="pico-hint-label">{label}</span>
    </span>
  )
}
