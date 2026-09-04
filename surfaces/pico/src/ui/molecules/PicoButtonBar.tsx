import { PicoHint, type PicoHintKey } from "../atoms/PicoHint"

export interface PicoButtonBarHint {
  readonly hintKey: PicoHintKey
  readonly label: string
}

/**
 * The bottom chrome: what the buttons do on this screen.
 *
 * Aria-hidden as a whole. Every action it names is reachable by focusing the
 * control that performs it, so announcing the bar as well would read the same
 * affordance twice to someone who cannot see the glyphs anyway.
 */
export function PicoButtonBar({
  hints,
}: {
  readonly hints: readonly PicoButtonBarHint[]
}) {
  return (
    <footer aria-hidden className="pico-button-bar">
      {hints.map((hint) => (
        <PicoHint hintKey={hint.hintKey} key={hint.hintKey} label={hint.label} />
      ))}
    </footer>
  )
}
