/**
 * One row in a save list: a game in the split review, or a snapshot in a save
 * history. The settings row's family (a reading position with the surface's
 * focus halo), with room for the marks a save carries: Current, Interrupted,
 * Saves split.
 *
 * A row is a real button only when the host gave it a real action. Otherwise
 * it is a readable focus anchor, so the whole list stays reachable by
 * directional input. A host action that is present but disabled stays
 * focusable and says why through its description.
 */
import { ChevronRight } from "lucide-react"
import {
  SHIFT_DESIGN_PARTS,
  shiftDesignPartAttrs,
} from "../../shift-design-parts"

export interface ShiftSaveRowMark {
  readonly label: string
  readonly tone: "accent" | "attention" | "quiet"
}

export interface ShiftSaveRowProps {
  readonly index: number
  readonly label: string
  /** Facts under the label, each on its own line when space is short. */
  readonly facts?: readonly string[]
  readonly marks?: readonly ShiftSaveRowMark[]
  readonly focused?: boolean
  readonly onFocus?: () => void
  /** Present makes the row a button. */
  readonly onSelect?: () => void
  /** Shown when the host's action is present but not enabled. */
  readonly disabledReason?: string
}

export function ShiftSaveRow({
  index,
  label,
  facts = [],
  marks = [],
  focused = false,
  onFocus,
  onSelect,
  disabledReason,
}: ShiftSaveRowProps) {
  const name = [label, ...facts, ...marks.map(mark => mark.label)].join(". ")
  const body = (
    <>
      <span className="shift-save-row-text">
        <span className="shift-save-row-label" title={label}>{label}</span>
        {facts.length > 0 ? (
          <span className="shift-save-row-facts">
            {facts.map(fact => <span key={fact}>{fact}</span>)}
          </span>
        ) : null}
        {disabledReason ? (
          <span className="shift-save-row-reason">{disabledReason}</span>
        ) : null}
      </span>
      <span className="shift-save-row-tail">
        {marks.map(mark => (
          <span key={mark.label} className="shift-save-mark" data-tone={mark.tone}>
            {mark.label}
          </span>
        ))}
        {onSelect ? <ChevronRight className="shift-save-row-chevron" aria-hidden /> : null}
      </span>
    </>
  )
  const common = {
    className: "shift-save-row",
    "data-save-index": index,
    "data-focused": focused || undefined,
    "data-unavailable": disabledReason ? "true" : undefined,
    "aria-label": name,
    onFocus,
    ...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.saveRow, label),
  }
  return onSelect ? (
    <button type="button" onClick={onSelect} {...common}>{body}</button>
  ) : (
    <div tabIndex={0} role="group" aria-disabled={disabledReason ? true : undefined} {...common}>
      {body}
    </div>
  )
}
