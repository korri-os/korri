/**
 * Shift library — one titled shelf (organism).
 *
 * A `.shift-lib-shelf` section: a title over a horizontal track of cover tiles.
 * Shared by the Shelves variant and the Lens variant's By-Genre mode, which
 * both map their sections onto this one real part.
 */
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftLibraryShelfTitle } from "./ShiftLibraryShelfTitle"
import { ShiftLibraryTile } from "./ShiftLibraryTile"
import type { ShiftLibraryGame } from "./shift-library-game"

export interface ShiftLibraryShelfProps {
  readonly title: string
  readonly games: readonly ShiftLibraryGame[]
  readonly onSelect?: (id: string) => void
  /** Real DOM focus landed on a tile. */
  readonly onFocus?: (id: string) => void
}

export function ShiftLibraryShelf({
  title,
  games,
  onSelect,
  onFocus,
}: ShiftLibraryShelfProps) {
  return (
    <section
      className="shift-lib-shelf"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.libraryShelf)}
    >
      <ShiftLibraryShelfTitle title={title} />
      <div className="shift-lib-shelf-track">
        {games.map(game => (
          <ShiftLibraryTile key={game.id} game={game} onSelect={onSelect} onFocus={onFocus} />
        ))}
      </div>
    </section>
  )
}
