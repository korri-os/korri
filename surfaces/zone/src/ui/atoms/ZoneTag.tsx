import "./ZoneTag.css"

/**
 * What a tag says about a game or a save, and so what colour it wears:
 * the system in blue, HERE in yellow, a remote device in red, SAVED in
 * mint, OPEN in orange, CONTINUE in white. White caps keep the black
 * outline; a light tag takes black ink.
 */
export type ZoneTagKind = "system" | "here" | "remote" | "saved" | "open" | "continue"

/** Where a tag sits in a row, which decides which of its ends is cut. */
export type ZoneTagJoin = "only" | "first" | "middle" | "last"

export interface ZoneTagItem {
  readonly kind: ZoneTagKind
  readonly label: string
}

/**
 * A small coloured tag with caps, as under the TIME ATTACK card's title.
 * Joined tags meet on a slant: a first tag is cut on its right, a last tag
 * on its left, a middle tag on both, so a row reads as one strip.
 *
 * The tag never grows past its row: its caps shrink with the row's
 * `--zone-fit`, then truncate.
 */
export function ZoneTag({ kind, label, join = "only" }: ZoneTagItem & { readonly join?: ZoneTagJoin }) {
  return (
    <span className="zone-tag" data-join={join} data-kind={kind}>
      {label}
    </span>
  )
}
