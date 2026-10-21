/**
 * Shift store — a result tile (molecule).
 *
 * Harvested from legacy Shift's store: the tile itself is the action. It is
 * one native cover <button> that opens the entry, where Get lives, so a page
 * of results carries no per-item chrome. Absent art becomes the title
 * monogram, as everywhere in Shift.
 */
import type { SurfaceStoreEntry } from "@contracts/surface/korri-surface"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftCoverArt } from "../ui/atoms/ShiftCoverArt"

export interface ShiftStoreBrowseTileProps {
  readonly entry: SurfaceStoreEntry
  readonly onOpen?: (id: string) => void
}

export function ShiftStoreBrowseTile({ entry, onOpen }: ShiftStoreBrowseTileProps) {
  return (
    <button
      type="button"
      className="shift-store-tile"
      aria-label={entry.system === undefined ? entry.title : `${entry.title}, ${entry.system}`}
      onClick={() => onOpen?.(entry.id)}
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.storeBrowseTile, entry.id)}
    >
      <span className="shift-store-tile-art">
        <ShiftCoverArt src={entry.thumbnailUrl ?? ""} loading="lazy" title={entry.title} />
      </span>
      <span className="shift-store-tile-body">
        <span className="shift-store-tile-title">{entry.title}</span>
        {entry.system === undefined ? null : (
          <span className="shift-store-tile-system">{entry.system}</span>
        )}
      </span>
    </button>
  )
}
