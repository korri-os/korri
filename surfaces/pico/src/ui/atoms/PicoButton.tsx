/**
 * What a button says about itself: `go` is the one thing the screen wants
 * pressed, `quiet` is a real choice that is not the main one, and `danger`
 * cannot be undone. One per screen should be `go`.
 */
export type PicoButtonTone = "go" | "quiet" | "danger"

/**
 * A fat plastic button with a hard shadow.
 *
 * Focus is the d-pad cursor, so it is never removed: the shadow colour-cycles
 * the way PICO-8's own cursor flashes. Pressing moves the body into its shadow.
 */
export function PicoButton({
  label,
  tone = "go",
  onPress,
}: {
  readonly label: string
  readonly tone?: PicoButtonTone
  readonly onPress: () => void
}) {
  return (
    <button className="pico-button" data-tone={tone} onClick={onPress} type="button">
      {label}
    </button>
  )
}
