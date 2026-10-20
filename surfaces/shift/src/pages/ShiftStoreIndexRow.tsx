/**
 * Shift store — an index row (molecule).
 *
 * Harvested from legacy Shift's store: the whole row is one native <button>
 * with a quiet trailing chevron. The store lists its providers with it.
 */
import { ChevronRight } from "lucide-react"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"

export interface ShiftStoreIndexRowProps {
  readonly id: string
  readonly title: string
  readonly meta?: string
  readonly onOpen?: (id: string) => void
}

export function ShiftStoreIndexRow({ id, title, meta, onOpen }: ShiftStoreIndexRowProps) {
  return (
    <button
      type="button"
      className="shift-store-index-row"
      aria-label={title}
      onClick={() => onOpen?.(id)}
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.storeIndexRow, id)}
    >
      <span className="shift-store-index-body">
        <span className="shift-store-index-title">{title}</span>
        {meta === undefined ? null : <span className="shift-store-index-meta">{meta}</span>}
      </span>
      <ChevronRight className="shift-store-index-chevron" aria-hidden="true" />
    </button>
  )
}
