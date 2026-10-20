/**
 * Shift game detail — the key-art panel (atom).
 *
 * The full-height cover image that holds one edge of the detail split.
 */
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftCoverArt } from "../ui/atoms/ShiftCoverArt"

export interface ShiftDetailArtProps {
  readonly artUrl: string
  /** With no art, draw this title's monogram instead of a bare panel. */
  readonly monogramTitle?: string
}

export function ShiftDetailArt({ artUrl, monogramTitle }: ShiftDetailArtProps) {
  return (
    <div
      className="shift-detail-split-art"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.detailArt)}
    >
      {artUrl ? <img src={artUrl} alt="" loading="lazy" />
        : monogramTitle === undefined ? null : <ShiftCoverArt src="" title={monogramTitle} />}
    </div>
  )
}
