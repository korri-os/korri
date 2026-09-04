import { PicoPixelDisc } from "./PicoPixelDisc"

/**
 * The buttons Pico currently names in a hint.
 *
 * Exactly the two the surface uses today. A wider set would offer a design tool
 * options no screen has asked for, and the honest moment to widen it is when a
 * screen needs the third.
 */
export type PicoHintKey = "a" | "b"

/**
 * One button hint: the face button and what it does here.
 *
 * The disc is drawn on a pixel grid rather than rounded by CSS — at this size a
 * smooth curve is the most obviously non-8-bit thing on the screen. The colour
 * is the gamepad's, so the button is recognised before the letter is read.
 *
 * Hints state what the buttons already do; they are not controls. Pico never
 * makes one clickable, because a hint that can be pressed is a button wearing a
 * hint's clothes, and a thumb would never find it.
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
      <span className="pico-hint-key" data-key={hintKey}>
        <span className="pico-hint-key-disc">
          <PicoPixelDisc />
        </span>
        <b className="pico-hint-key-glyph">{hintKey.toUpperCase()}</b>
      </span>
      <span className="pico-hint-label">{label}</span>
    </span>
  )
}
