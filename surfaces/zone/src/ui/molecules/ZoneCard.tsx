import "./ZoneCard.css"
import "../../zone-motion.css"
import { useRef } from "react"
import { useZoneFit } from "../../use-zone-fit"
import { formatZoneBytes } from "../../plan/bytes"
import { ZoneCoverWell } from "../atoms/ZoneCoverWell"
import type { ZoneTagItem } from "../atoms/ZoneTag"
import { ZoneTagRow } from "./ZoneTagRow"

/** A place a game can be played: this device, or a remote one by name. */
export interface ZonePlace {
  readonly label: string
  readonly here: boolean
}

/** What Korri publishes about a game that the card draws. */
export interface ZoneCardFacts {
  readonly title: string
  /** The subtitle: the system, worn as the first tag. */
  readonly system?: string
  readonly places: readonly ZonePlace[]
  readonly resumable?: boolean
  /** The cover art's URL. Absent: the well wears stripes and initials. */
  readonly art?: string
  /**
   * Korri's `download`: launching first fetches something to this device.
   * `bytes` is the size still to fetch, absent when Korri does not know it.
   */
  readonly download?: { readonly bytes?: number }
}

/** Where the card sits in the band: an end card dims the triangle beyond it. */
export type ZoneCardPosition = "first" | "middle" | "last" | "only"
/** The focused card is the big one; a side card is the plan's share of it. */
export type ZoneCardSize = "focus" | "side"

/** The download tag's words: DOWNLOAD and the size when Korri knows it. */
export const downloadLabel = (download: { readonly bytes?: number }): string =>
  download.bytes === undefined ? "Download" : `Download ${formatZoneBytes(download.bytes)}`

export const cardTags = ({ system, places, resumable, download }: ZoneCardFacts): readonly ZoneTagItem[] => [
  ...(system === undefined ? [] : [{ kind: "system", label: system } as const]),
  ...places.map(place => place.here
    ? { kind: "here", label: "Here" } as const
    : { kind: "remote", label: place.label } as const),
  ...(resumable === true ? [{ kind: "continue", label: "Continue" } as const] : []),
  ...(download === undefined ? [] : [{ kind: "download", label: downloadLabel(download) } as const]),
]

/**
 * The zone card: a black body with a hard drop edge, the cover in its well,
 * the title in two lines of white caps that shrink to fit, the orange, blue
 * and mint diagonal band, and the tag row. A game that needs a download
 * before its first launch closes the row with an orange DOWNLOAD tag and
 * the size when Korri knows it. Focused, it wears the pink edge
 * and the two red triangles that point at it; at an end of the band, the
 * triangle with nothing beyond it dims.
 *
 * A native button: the host puts DOM focus on it and the card reports the
 * focus through `onFocus`, and A (a click) through `onPress`. Its size comes from the band through
 * `--zone-card-h`; standing alone it takes the plan's focused height.
 */
export function ZoneCard({
  title,
  system,
  places,
  resumable = false,
  art,
  download,
  focused = false,
  position = "middle",
  size = "focus",
  onFocus,
  onPress,
}: ZoneCardFacts & {
  readonly focused?: boolean
  readonly position?: ZoneCardPosition
  readonly size?: ZoneCardSize
  readonly onFocus?: () => void
  /** A on the card. */
  readonly onPress?: () => void
}) {
  const card = useRef<HTMLButtonElement>(null)
  const heading = useRef<HTMLSpanElement>(null)
  useZoneFit(card, [heading], title)
  return (
    <button
      className="zone-card"
      data-focused={focused ? "true" : undefined}
      data-position={position}
      data-size={size}
      onClick={onPress}
      onFocus={onFocus}
      ref={card}
      type="button"
    >
      <i aria-hidden="true" className="zone-card-tri" data-side="left" />
      <i aria-hidden="true" className="zone-card-tri" data-side="right" />
      <ZoneCoverWell art={art} title={title} />
      <span className="zone-card-body">
        <span className="zone-card-title" ref={heading}>{title}</span>
        <i aria-hidden="true" className="zone-card-stripe" />
        <ZoneTagRow tags={cardTags({ title, system, places, resumable, download })} />
      </span>
    </button>
  )
}
