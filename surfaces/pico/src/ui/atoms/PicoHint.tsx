import "./PicoHint.css"
import { PicoPixelDisc } from "./PicoPixelDisc"

/**
 * The buttons Pico currently names in a hint: the two face buttons, and left
 * and right together, which the gameplay overlay names when a range is there to
 * adjust. A wider set would offer a design tool options no screen has asked
 * for; widen it when a screen needs the next one.
 */
export type PicoHintKey = "a" | "b" | "lr"

/**
 * One button hint: the button and what it does here.
 *
 * A face button's disc is drawn on a pixel grid rather than rounded by CSS — at
 * this size a smooth curve is the most obviously non-8-bit thing on the screen.
 * Its colour is the gamepad's, so the button is recognised before the letter is
 * read. Left and right are a plain plate with both arrows, because the d-pad
 * has no colour of its own on any handheld.
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
        {hintKey === "lr" ? null : (
          <span className="pico-hint-key-disc">
            <PicoPixelDisc />
          </span>
        )}
        <b className="pico-hint-key-glyph">{hintKey === "lr" ? "◀▶" : hintKey.toUpperCase()}</b>
      </span>
      <span className="pico-hint-label">{label}</span>
    </span>
  )
}
