import { picoLabelFor } from "../../pico-label"
import { PicoCoverArt } from "../atoms/PicoCoverArt"

/**
 * Where a cart sits on the shelf. The focused cart is the hero; its neighbours
 * peek in from the sides.
 */
/**
 * `hero` and `side` are shelf positions and are buttons. `still` is a cart as
 * artwork — on a game's own screen it is what the screen is about, not a
 * control, so it renders as a plain figure and takes no focus. One component,
 * because a second cart would be the same label drawn twice.
 */
export type PicoCartPlacement = "hero" | "side" | "still"

/**
 * One game, drawn as a cartridge.
 *
 * A game with no art still gets a label: two palette colours and a dither
 * derived from its id, so a shelf of unillustrated games reads as a rack of
 * different cartridges rather than a row of identical rectangles. The choice
 * travels as data attributes rather than an inline style, which keeps the
 * colours in the stylesheet where they can be themed and reviewed.
 *
 * Every cart is focusable, including the ones peeking at the edges: focus is
 * how a d-pad moves the shelf, so a cart that could not take focus would be
 * unreachable on the hardware Pico is built for. Focusing a cart is what makes
 * it the hero — the shelf listens rather than the cart deciding.
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
  const Tag = placement === "still" ? "figure" : "button"
  return (
    <Tag
      aria-label={subtitle === undefined ? title : `${title}, ${subtitle}`}
      className="pico-cart"
      data-accent={label.accent}
      data-dither={label.dither}
      data-fill={label.fill}
      data-ink={label.ink}
      data-placement={placement}
      {...(placement === "still"
        ? {}
        : { onClick: onActivate, onFocus, type: "button" as const })}
    >
      <span className="pico-cart-window">
        <PicoCoverArt artUrl={artUrl} title={title} />
      </span>
      <span aria-hidden className="pico-cart-notch" />
      {resumable ? (
        <span aria-hidden className="pico-cart-resume">
          ▸
        </span>
      ) : null}
    </Tag>
  )
}
