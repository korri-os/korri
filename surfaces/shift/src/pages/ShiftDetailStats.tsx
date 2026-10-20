/**
 * Shift game detail — the play-history stats row (molecule).
 *
 * What the game still has to download, last-played (or "Never played"),
 * optional playtime, and the favourite badge for the detail info column.
 */
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftDetailFavoriteBadge } from "./ShiftDetailFavoriteBadge"

export interface ShiftDetailStatsProps {
  readonly lastPlayedLabel?: string
  readonly playtimeLabel?: string
  readonly favorite?: boolean
  /** What the game still has to fetch ("Waiting to download"). */
  readonly download?: string
}

export function ShiftDetailStats({
  lastPlayedLabel,
  playtimeLabel,
  favorite,
  download,
}: ShiftDetailStatsProps) {
  return (
    <div
      className="shift-detail-stats"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.detailStats)}
    >
      {download ? <span className="shift-detail-download">{download}</span> : null}
      <span>
        {lastPlayedLabel ? `Last played ${lastPlayedLabel}` : "Never played"}
      </span>
      {playtimeLabel ? <span>{playtimeLabel} played</span> : null}
      {favorite ? <ShiftDetailFavoriteBadge /> : null}
    </div>
  )
}
