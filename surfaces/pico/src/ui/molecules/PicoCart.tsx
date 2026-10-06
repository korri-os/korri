import "../../pico-motion.css"
import "./PicoCart.css"
import { memo } from "react"
import { picoCartLookFor } from "../../pico-label"
import { PicoCoverArt } from "../atoms/PicoCoverArt"

/**
 * Where a cart is.
 *
 * `side` and `hero` sit on a shelf and are buttons: `hero` is the chosen one,
 * lifted off the shelf with a shadow under it. The button is as tall as its
 * row, and the cart stands at the bottom of it. `still` is a cart as the thing
 * a screen is about — the big cartridge on a game's own screen — and renders
 * as a figure that takes no focus. `tile` is a shelf-sized cart drawn inside
 * another control, such as a search result, which owns the focus itself. One
 * component, because a second cart would be the same plastic drawn twice.
 */
export type PicoCartPlacement = "hero" | "side" | "still" | "tile"

/**
 * Where the player stands with a game, as the plastic shows it: `resume` has
 * a save waiting, `new` has never been played, `played` is neither.
 */
export type PicoCartProgress = "resume" | "new" | "played"

/**
 * One game, drawn as a cartridge: plastic shell, a notch at the top, grip
 * ridges at the bottom, and the cover as the sticker in its window.
 *
 * The cartridge takes the shape of its art. The shell's colour and silhouette
 * come from the system named in the subtitle, so a GB cart and an NES cart look
 * like different plastic; the label colour comes from the game's id. Both are
 * carried as data attributes so the colour stays in the stylesheet.
 *
 * Every shelf cart is focusable, including the ones at the edges: focus is how
 * a d-pad moves along the shelf, so a cart that could not take focus would be
 * unreachable on the hardware Pico is built for.
 *
 * The host moves focus to the control whose centre is nearest in the pressed
 * direction. Carts take their height from their art, so a button the size of
 * its cart would put each centre at a different height, and Down from a tall
 * cart would step sideways onto a short neighbour instead of leaving the row.
 * A button the height of its row gives every cart in the row one centre.
 *
 * A shelf can hold thousands of carts, so a cart is drawn again only when one
 * of its values changes. Its callbacks are not compared: a caller must pass
 * callbacks that do the same thing whenever the values are the same, by
 * closing over stable functions or reading the latest state through a ref.
 */
export const PicoCart = memo(function PicoCart({
  id,
  title,
  subtitle,
  artUrl,
  placement,
  resumable = false,
  progress,
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
  /** Overrides what `resumable` implies, when the caller knows play history. */
  readonly progress?: PicoCartProgress
  readonly onFocus?: () => void
  readonly onActivate?: () => void
}) {
  const look = picoCartLookFor(id, subtitle)
  const state: PicoCartProgress = progress ?? (resumable ? "resume" : "played")
  const name = subtitle === undefined ? title : `${title}, ${subtitle}`
  const Tag = placement === "still" ? "figure" : "span"
  const cart = (
    <Tag
      className="pico-cart"
      data-placement={placement}
      data-progress={state}
      data-shell={look.shell}
      data-system={look.system}
      {...(placement === "tile"
        ? { "aria-hidden": true }
        : placement === "still"
          ? { "aria-label": name }
          : {})}
    >
      <span className="pico-cart-window">
        <PicoCoverArt artUrl={artUrl} id={id} subtitle={subtitle} title={title} />
      </span>
      {state === "resume" ? (
        <span aria-hidden className="pico-cart-resume">
          ▶
        </span>
      ) : null}
      {state === "new" && placement !== "still" ? (
        <span aria-hidden className="pico-cart-new">
          NEW
        </span>
      ) : null}
    </Tag>
  )
  if (placement === "still" || placement === "tile") return cart
  return (
    <button aria-label={name} className="pico-cart-button" onClick={onActivate} onFocus={onFocus} type="button">
      {cart}
    </button>
  )
}, sameValues)

/** Equal when every prop that is not a callback is equal. */
function sameValues<Props extends object>(before: Props, after: Props): boolean {
  const keys = new Set([...Object.keys(before), ...Object.keys(after)])
  for (const key of keys) {
    const a = (before as Record<string, unknown>)[key]
    const b = (after as Record<string, unknown>)[key]
    if (typeof a === "function" && typeof b === "function") continue
    if (!Object.is(a, b)) return false
  }
  return true
}
