import "./ZoneTag.css"

/**
 * What a tag says about a game or a save, and so what colour it wears:
 * the system in blue, HERE in yellow, a remote device in red, SAVED in
 * mint, OPEN in orange, CONTINUE in black (TIME ATTACK's third tag),
 * DOWNLOAD in orange (OPEN is only ever on a save or the running strip,
 * never on a game card). A dark tag wears white caps ringed in black one
 * art px wide; a light tag takes black ink with no ring.
 */
export type ZoneTagKind = "system" | "here" | "remote" | "saved" | "open" | "continue" | "download"

/** Where a tag sits in a row, which decides which of its ends is cut. */
export type ZoneTagJoin = "only" | "first" | "middle" | "last"

export interface ZoneTagItem {
  readonly kind: ZoneTagKind
  readonly label: string
  /** The label is Korri's (a system, a device's name): the look decides its face and case. */
  readonly korri?: boolean
}

/**
 * A small coloured tag with caps, as under the TIME ATTACK card's title.
 * Joined tags meet on a slant: a first tag is cut on its right, a last tag
 * on its left, a middle tag on both, so a row reads as one strip.
 *
 * The tag never grows past its row. Its caps stand on whole caps steps
 * only: whatever size the holder asks for is rounded down to caps-1, 2 or
 * 3, never between. A row that is still too narrow drops its last tags
 * (ZoneTagRow); a tag standing alone truncates with "…".
 */
export function ZoneTag({ kind, label, korri = false, join = "only" }: ZoneTagItem & { readonly join?: ZoneTagJoin }) {
  return (
    <span className="zone-tag" data-face={korri ? "korri" : undefined} data-join={join} data-kind={kind}>
      {label}
    </span>
  )
}
