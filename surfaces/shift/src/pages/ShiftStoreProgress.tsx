/**
 * Shift store — a download's progress (atom).
 *
 * A real bar from Korri's bytes. Without a total there is no end to draw, so
 * the bar becomes a moving band that says only that work is under way; with
 * reduced motion the band holds still.
 */
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"

export interface ShiftStoreProgressProps {
  readonly label: string
  readonly done: number
  readonly total?: number
}

export function ShiftStoreProgress({ label, done, total }: ShiftStoreProgressProps) {
  const known = total !== undefined && total > 0
  const percent = known ? Math.min(100, Math.max(0, Math.round((done / total) * 100))) : undefined
  return (
    <div
      className="shift-store-progress"
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      {...(percent === undefined ? {} : { "aria-valuenow": percent })}
      data-indeterminate={known ? undefined : ""}
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.storeProgress)}
    >
      <span
        className="shift-store-progress-fill"
        style={percent === undefined ? undefined : { width: `${percent}%` }}
      />
    </div>
  )
}
