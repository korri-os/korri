/**
 * Shift library — a stack of titled shelves (organism).
 *
 * The `.shift-lib-shelf-stack` policy shared by the Shelves variant and the
 * Lens variant's By-Genre mode: it owns the wrapper and maps sections onto the
 * real `ShiftLibraryShelf`, so shelf-stack layout/ordering lives in one place.
 */
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftLibraryShelf } from "./ShiftLibraryShelf"
import type { ShiftLibrarySection } from "./shift-library-sections"

export interface ShiftLibraryShelfStackProps {
  readonly sections: readonly ShiftLibrarySection[]
  readonly onSelect?: (id: string) => void
  /** Real DOM focus landed on a tile. */
  readonly onFocus?: (id: string) => void
}

export function ShiftLibraryShelfStack({
  sections,
  onSelect,
  onFocus,
}: ShiftLibraryShelfStackProps) {
  return (
    <div
      className="shift-lib-shelf-stack"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.libraryShelfStack)}
    >
      {sections.map(section => (
        <ShiftLibraryShelf
          key={section.id}
          title={section.title}
          games={section.games}
          onSelect={onSelect}
          onFocus={onFocus}
        />
      ))}
    </div>
  )
}
