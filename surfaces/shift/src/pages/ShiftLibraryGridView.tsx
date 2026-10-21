/**
 * Shift library — the additive cover grid (organism).
 *
 * The `.shift-lib-grid` of focusable cover tiles shared by the Grid variant,
 * the Filter Bar variant, and the Lens variant's flat modes. Source-agnostic:
 * takes flat games and reports selection by id.
 */
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftLibraryTile } from "./ShiftLibraryTile"
import type { ShiftLibraryGame } from "./shift-library-game"

export interface ShiftLibraryGridViewProps {
  readonly games: readonly ShiftLibraryGame[]
  readonly onSelect?: (id: string) => void
  /** Real DOM focus landed on a tile. */
  readonly onFocus?: (id: string) => void
}

export function ShiftLibraryGridView({
  games,
  onSelect,
  onFocus,
}: ShiftLibraryGridViewProps) {
  return (
    <div
      className="shift-lib-grid"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.libraryGridView)}
    >
      {games.map(game => (
        <ShiftLibraryTile key={game.id} game={game} onSelect={onSelect} onFocus={onFocus} />
      ))}
    </div>
  )
}
