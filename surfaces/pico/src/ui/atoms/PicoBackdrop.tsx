/**
 * Which ground a screen sits on.
 *
 * `stars` drifts; `dither` scrolls in hard steps. `none` is explicit rather
 * than an omitted prop, so a screen that wants a plain ground says so and a
 * reader never has to work out whether the backdrop was forgotten.
 */
export type PicoBackdropField = "stars" | "dither" | "none"

/**
 * The moving ground behind a screen.
 *
 * A flat field is what made this surface look dead: the games in the reference
 * carts are never sitting on nothing. Both treatments are pure CSS gradients
 * animated by background-position, so there is no canvas, no timer, and nothing
 * to tear down — and both step rather than glide, because a smooth parallax is
 * the tell of a web page pretending to be a console.
 *
 * Decorative and inert: it never takes a pointer event and is hidden from
 * assistive technology.
 */
export function PicoBackdrop({
  field,
}: {
  readonly field: PicoBackdropField
}) {
  if (field === "none") return null
  return <div aria-hidden className="pico-backdrop" data-field={field} />
}
