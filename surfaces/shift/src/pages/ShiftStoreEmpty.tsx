/**
 * Shift store — the empty or waiting line (atom).
 *
 * The one line a store screen falls back to when it has nothing to list yet:
 * a search that found nothing, a provider list still loading, a refusal in
 * Korri's words. Harvested from legacy Shift's store.
 */
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"

export interface ShiftStoreEmptyProps {
  readonly message?: string
}

export function ShiftStoreEmpty({
  message = "Nothing found. Try another search.",
}: ShiftStoreEmptyProps) {
  return (
    <p
      className="shift-store-empty"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.storeEmpty)}
    >
      {message}
    </p>
  )
}
