/**
 * The full words of a save screen, for when its container is too short to
 * show them (intrinsic-design rule 6: a readout may shrink or relocate, but
 * its information must survive somewhere).
 *
 * On a short container the save screens cut the lede and the host's detail
 * to one line, or drop the lede, and clamp long names. The footer then
 * offers Details, which opens this sheet with every line whole. Each line is
 * its own focus stop, so directional input scrolls the sheet line by line
 * on any height.
 */
import {
  SHIFT_DESIGN_PARTS,
  shiftDesignPartAttrs,
} from "../../shift-design-parts"
import { ShiftSheetBody } from "./ShiftSheetBody"
import { ShiftSheetHeader } from "./ShiftSheetHeader"
import { ShiftSheetPanel } from "./ShiftSheetPanel"
import { ShiftSheetRoot } from "./ShiftSheetRoot"
import { ShiftSheetTitle } from "./ShiftSheetTitle"

export interface ShiftSaveDetailsSheetProps {
  readonly open: boolean
  readonly title: string
  /** Every line the screen shows or may cut, whole, in reading order. */
  readonly lines: readonly string[]
  readonly onClose: () => void
}

export function ShiftSaveDetailsSheet({ open, title, lines, onClose }: ShiftSaveDetailsSheetProps) {
  return (
    <ShiftSheetRoot open={open} onClose={onClose} label={`Details: ${title}`}>
      <ShiftSheetPanel>
        <ShiftSheetHeader>
          <ShiftSheetTitle>{title}</ShiftSheetTitle>
        </ShiftSheetHeader>
        <ShiftSheetBody>
          <div className="shift-save-details" {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.saveDetails)}>
            {lines.map(line => (
              <p key={line} className="shift-save-details-line" tabIndex={0}>
                {line}
              </p>
            ))}
          </div>
        </ShiftSheetBody>
      </ShiftSheetPanel>
    </ShiftSheetRoot>
  )
}

/** The footer's Details control. CSS shows it only where a line can be cut. */
export function ShiftSaveDetailsButton({ onOpen }: { readonly onOpen: () => void }) {
  return (
    <button type="button" className="shift-saves-action shift-saves-details" onClick={onOpen}>
      Details
    </button>
  )
}
