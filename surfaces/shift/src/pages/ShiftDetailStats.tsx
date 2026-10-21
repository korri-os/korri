/**
 * Shift game detail — the play-history stats row (molecule).
 *
 * What the game still has to download, last-played (or "Never played"),
 * optional playtime, and the favourite badge for the detail info column.
 */
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { SAVE_MARK_LABELS, type ShiftSaveState, SPLIT_NOTICE } from "../save-sync-labels"
import { ShiftDetailFavoriteBadge } from "./ShiftDetailFavoriteBadge"

export interface ShiftDetailStatsProps {
  readonly lastPlayedLabel?: string
  readonly playtimeLabel?: string
  readonly favorite?: boolean
  /** What the game still has to fetch ("Waiting to download"). */
  readonly download?: string
  /** The game's save mark. */
  readonly saves?: ShiftSaveState
}

export function ShiftDetailStats({
  lastPlayedLabel,
  playtimeLabel,
  favorite,
  download,
  saves,
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
      {saves ? (
        <span className="shift-detail-saves" data-saves={saves}>
          {saves === "Split" ? SPLIT_NOTICE : SAVE_MARK_LABELS[saves]}
        </span>
      ) : null}
    </div>
  )
}
