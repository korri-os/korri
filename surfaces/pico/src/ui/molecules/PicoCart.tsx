import { picoLabelFor } from "../../pico-label"
import { PicoCoverArt } from "../atoms/PicoCoverArt"

/**
 * Where a cart is.
 *
 * `side` and `hero` sit on a shelf and are buttons: `hero` is the chosen one,
 * lifted off the shelf with a shadow under it. `still` is a cart as the thing
 * a screen is about — the big cartridge on a game's own screen — and renders
 * as a figure that takes no focus. `tile` is a shelf-sized cart drawn inside
 * another control, such as a search result, which owns the focus itself. One
 * component, because a second cart would be the same plastic drawn twice.
 */
export type PicoCartPlacement = "hero" | "side" | "still" | "tile"

/**
 * One game, drawn as a cartridge: plastic shell, a notch at the top, grip
 * ridges at the bottom, and the cover as the sticker in its window.
 *
 * The cartridge takes the shape of its art. A tall box makes a tall cart and a
 * wide store header a short one, so a shelf of mixed platforms reads as a real
 * shelf rather than a grid of cropped squares. The shell colour comes from the
 * game's id, carried as a data attribute so the colour stays in the
 * stylesheet.
 *
 * Every shelf cart is focusable, including the ones at the edges: focus is how
 * a d-pad moves along the shelf, so a cart that could not take focus would be
 * unreachable on the hardware Pico is built for.
 */
export function PicoCart({
  id,
  title,
  subtitle,
  artUrl,
  placement,
  resumable = false,
  onFocus,
  onActivate,
}: {
  readonly id: string
  readonly title: string
  readonly subtitle?: string
  readonly artUrl?: string
  readonly placement: PicoCartPlacement
  /** Marks a cart that continues a session rather than starting a new one. */
  readonly resumable?: boolean
  readonly onFocus?: () => void
  readonly onActivate?: () => void
}) {
  const label = picoLabelFor(id)
  const name = subtitle === undefined ? title : `${title}, ${subtitle}`
  const Tag = placement === "still" ? "figure" : placement === "tile" ? "span" : "button"
  return (
    <Tag
      className="pico-cart"
      data-placement={placement}
      data-shell={label.shell}
      {...(placement === "tile"
        ? { "aria-hidden": true }
        : placement === "still"
          ? { "aria-label": name }
          : { "aria-label": name, onClick: onActivate, onFocus, type: "button" as const })}
    >
      <span className="pico-cart-window">
        <PicoCoverArt artUrl={artUrl} id={id} title={title} />
      </span>
      {resumable ? (
        <span aria-hidden className="pico-cart-resume">
          ▶
        </span>
      ) : null}
    </Tag>
  )
}
