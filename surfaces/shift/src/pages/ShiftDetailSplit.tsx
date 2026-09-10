/**
 * Shift game detail — Rebalance B: Split.
 *
 * Balanced weight: the key art holds one full-height panel edge-to-edge, the
 * info column holds the other — title, tags, synopsis, stats, actions stacked
 * and left-aligned, editorial. A container aspect-ratio query stacks art-over-
 * info on a tall handheld and splits art | info on a wide lean-back frame, so
 * the balance follows the device shape, not a viewport breakpoint.
 */
import { useSurfaceAction } from "../host/surface-host"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftDetailActions } from "./ShiftDetailActions"
import { ShiftDetailArt } from "./ShiftDetailArt"
import { ShiftDetailHints } from "./ShiftDetailHints"
import { ShiftDetailStats } from "./ShiftDetailStats"
import { ShiftDetailSynopsis } from "./ShiftDetailSynopsis"
import { ShiftDetailTags } from "./ShiftDetailTags"
import { ShiftDetailTitle } from "./ShiftDetailTitle"
import { shiftDetailSynopsis } from "./shift-detail-copy"
import type { ShiftGameDetailView } from "./shift-game-detail-view"

export interface ShiftDetailSplitProps {
  readonly game: ShiftGameDetailView
  readonly onPlay?: (id: string) => void
  readonly onNewGame?: (id: string) => void
  readonly onFavorite?: (id: string) => void
  readonly onOptions?: (id: string) => void
  readonly onBack?: () => void
}

export function ShiftDetailSplit({
  game,
  onPlay,
  onNewGame,
  onFavorite,
  onBack,
  onOptions,
}: ShiftDetailSplitProps) {
  useSurfaceAction("back", () => onBack?.())
  useSurfaceAction("options", () => onOptions?.(game.id))

  const tags = [game.genre, game.developer].filter(Boolean).join(" · ")

  return (
    <div
      data-shift-detail
      data-shift-detail-game-id={game.id}
      className="shift-detail-split intrinsic"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.detailTemplate)}
    >
      <ShiftDetailArt artUrl={game.artUrl} />

      <div className="shift-detail-split-info">
        <ShiftDetailTitle title={game.title} />
        {tags ? <ShiftDetailTags tags={tags} /> : null}
        <ShiftDetailSynopsis>{shiftDetailSynopsis(game)}</ShiftDetailSynopsis>
        <ShiftDetailStats
          lastPlayedLabel={game.lastPlayedLabel}
          playtimeLabel={game.playtimeLabel}
          favorite={game.favorite}
        />
        <ShiftDetailActions
          game={game}
          onPlay={onPlay}
          onNewGame={onNewGame}
          onFavorite={onFavorite}
          onOptions={onOptions}
        />
        <ShiftDetailHints game={game} favoriteAvailable={Boolean(onFavorite)} />
      </div>
    </div>
  )
}
