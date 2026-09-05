/**
 * A small pill stating a condition: SAVING, FAILED, NEW.
 *
 * Tone is a role, not a colour: `warn` is the same yellow the surface uses for
 * every problem, so a badge never introduces a meaning the rest of the screen
 * does not already have.
 */
export function PicoBadge({
  text,
  tone = "info",
}: {
  readonly text: string
  readonly tone?: "info" | "ok" | "warn"
}) {
  return (
    <span className="pico-badge" data-tone={tone}>
      {text}
    </span>
  )
}
