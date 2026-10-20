import "./ZoneCoverWell.css"
import type { ReactNode } from "react"

/** The sets of three stripe colours a well with no art can wear. */
const STRIPE_SETS = 6

/**
 * The first letters of a title's two first words that are not articles,
 * so "The Legend of Zelda" reads LZ.
 */
export function initialsOf(title: string): string {
  return title
    .split(/\s+/)
    .filter(word => word.length > 0 && !/^(?:the|of|a|an|and)$/i.test(word))
    .map(word => word[0] ?? "")
    .join("")
    .slice(0, 2)
    .toUpperCase()
}

/** Which stripe set a title gets: the same title always gets the same one. */
export function stripeSetOf(title: string): number {
  let sum = 0
  for (const character of title) sum += character.codePointAt(0) ?? 0
  return sum % STRIPE_SETS
}

/**
 * The blue-edged dark well a cover sits in. The art is never cropped: it
 * keeps its own ratio and the well's black shows either side of it. With no
 * art, the well wears three Mania stripe bands in one of six colour orders,
 * chosen by the title, and the title's initials in outlined caps. Greyed,
 * the well drains its colour, as PAUSED greys the game behind it.
 *
 * `children` stand in the well instead of art or initials: the save card
 * puts a QR code or a line of text there. `noise` fills it with black and
 * white static, drawn anew every 100 ms and still under reduced motion,
 * while there is nothing to show yet.
 */
export function ZoneCoverWell({
  art,
  title,
  greyed = false,
  noise = false,
  children,
}: {
  readonly art?: string
  /** The title the well stands for; its initials show when there is no art. */
  readonly title: string
  readonly greyed?: boolean
  /** Nothing to show yet: MANIA MODE's used save window, two-colour static before its picture. */
  readonly noise?: boolean
  readonly children?: ReactNode
}) {
  const filled = noise ? "noise" : children !== undefined ? "node" : art === undefined ? "initials" : "art"
  return (
    <span
      className="zone-cover-well"
      data-filled={filled}
      data-greyed={greyed ? "true" : undefined}
      data-stripes={filled === "initials" ? stripeSetOf(title) : undefined}
    >
      {filled === "art" ? <img alt="" className="zone-cover-well-art" src={art} /> : null}
      {filled === "initials" ? <span className="zone-cover-well-initials">{initialsOf(title)}</span> : null}
      {filled === "node" ? children : null}
    </span>
  )
}
