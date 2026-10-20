/**
 * Shift store — one entry (template).
 *
 * Harvested from legacy Shift's store detail: the committed game-detail split
 * atoms, so an entry feels native to Shift, with the play-history verbs
 * swapped for getting it. The provider's words fill the synopsis; absent
 * art leaves the panel bare rather than inventing any.
 */
import type { SurfaceStoreDetails } from "@contracts/surface/korri-surface"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftDetailArt } from "./ShiftDetailArt"
import { ShiftDetailHint } from "./ShiftDetailHint"
import { ShiftDetailSynopsis } from "./ShiftDetailSynopsis"
import { ShiftDetailTags } from "./ShiftDetailTags"
import { ShiftDetailTitle } from "./ShiftDetailTitle"
import { ShiftStoreEmpty } from "./ShiftStoreEmpty"
import { ShiftStoreGet, shiftStoreGetPrimaryLabel } from "./ShiftStoreGet"

export interface ShiftStoreDetailProps {
  readonly details: Exclude<SurfaceStoreDetails, { readonly _tag: "Closed" }>
  readonly providerTitle?: string
  readonly onAction: (actionId: string) => void
  readonly onPlay: (gameId: string) => void
}

export function ShiftStoreDetail({ details, providerTitle, onAction, onPlay }: ShiftStoreDetailProps) {
  const entry = details.entry
  const tags = [providerTitle, entry.system].filter(Boolean).join(" · ")
  const primary = details._tag === "Ready" ? shiftStoreGetPrimaryLabel(details.get)
    : details._tag === "Problem" ? details.actions[0]?.label : undefined
  return (
    <div
      data-shift-detail
      data-shift-store-entry={entry.id}
      className="shift-detail-split intrinsic"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.storeDetail)}
    >
      <ShiftDetailArt artUrl={entry.thumbnailUrl ?? ""} />
      <div className="shift-detail-split-info">
        <ShiftDetailTitle title={entry.title} />
        {tags ? <ShiftDetailTags tags={tags} /> : null}
        {details._tag === "Loading" ? (
          <ShiftStoreEmpty message="Reading this game…" />
        ) : details._tag === "Problem" ? (
          <div className="shift-store-status">
            <ShiftStoreEmpty message={details.message} />
            {details.actions.map(action => (
              <button key={action.id} type="button" className="shift-store-retry" onClick={() => onAction(action.id)}>
                {action.label}
              </button>
            ))}
          </div>
        ) : (
          <>
            {details.description === undefined ? null : (
              <ShiftDetailSynopsis>{details.description}</ShiftDetailSynopsis>
            )}
            <ShiftStoreGet get={details.get} onAction={onAction} onPlay={onPlay} />
          </>
        )}
        <div className="shift-detail-buttonbar" {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.detailHints)}>
          {primary === undefined ? null : <ShiftDetailHint glyph="A" label={primary} />}
          <ShiftDetailHint glyph="B" label="Back" />
        </div>
      </div>
    </div>
  )
}
