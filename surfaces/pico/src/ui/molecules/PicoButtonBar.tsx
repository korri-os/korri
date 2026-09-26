import { PicoHint, type PicoHintKey } from "../atoms/PicoHint"

export interface PicoButtonBarHint {
  readonly hintKey: PicoHintKey
  readonly label: string
}

/**
 * The bottom of every screen: what the face buttons do here, and on the left
 * a short readout about the screen when it has one ("9 carts · 2 resumable").
 *
 * Hidden from assistive technology: every hint restates a control that is
 * already reachable and labelled, and the readout restates what the screen
 * shows.
 */
export function PicoButtonBar({
  hints,
  readout,
}: {
  readonly hints: readonly PicoButtonBarHint[]
  readonly readout?: string
}) {
  return (
    <footer aria-hidden className="pico-button-bar">
      {readout === undefined ? null : (
        <span className="pico-button-bar-readout">{readout}</span>
      )}
      <span className="pico-button-bar-hints">
        {hints.map((hint) => (
          <PicoHint hintKey={hint.hintKey} key={hint.hintKey} label={hint.label} />
        ))}
      </span>
    </footer>
  )
}
