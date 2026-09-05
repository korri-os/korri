import {
  PICO_ORDER_LABELS,
  type PicoLibraryView,
  type PicoOrder,
} from "../../pico-library-view"
import { PicoChip } from "../atoms/PicoChip"
import { PicoKeyboard } from "../molecules/PicoKeyboard"
import { PicoQueryField } from "../molecules/PicoQueryField"
import { PicoResultRow } from "../molecules/PicoResultRow"

/**
 * Find a game: type at it, narrow it to a collection, pick one.
 *
 * Results and keyboard share the screen rather than taking turns, because
 * hiding the results while typing means pressing keys blind — the whole value
 * of an incremental search is watching the list shrink.
 */
export function PicoLibraryBrowser({
  library,
  section,
  order,
  onOrder,
  onType,
  onBackspace,
  onClear,
  onSection,
  onOpen,
}: {
  readonly library: PicoLibraryView
  readonly section: string
  readonly order: PicoOrder
  readonly onOrder: (order: PicoOrder) => void
  readonly onType: (character: string) => void
  readonly onBackspace: () => void
  readonly onClear: () => void
  readonly onSection: (section: string) => void
  readonly onOpen: (gameId: string) => void
}) {
  return (
    <div className="pico-library-browser">
      <div className="pico-library-browser-find">
        <PicoQueryField query={library.query} />
        <div className="pico-library-browser-collections">
          {library.sections.map((candidate) => (
            <PicoChip
              key={candidate}
              label={candidate.toUpperCase()}
              onPress={() => onSection(candidate)}
              pressed={candidate === section}
            />
          ))}
        </div>
        <div className="pico-library-browser-orders">
          {library.orders.map((candidate) => (
            <PicoChip
              key={candidate}
              label={PICO_ORDER_LABELS[candidate]}
              onPress={() => onOrder(candidate)}
              pressed={candidate === order}
            />
          ))}
        </div>
        <PicoKeyboard onBackspace={onBackspace} onClear={onClear} onType={onType} />
      </div>
      <div className="pico-library-browser-results">
        {library.results.length === 0 ? (
          <p className="pico-library-browser-empty">
            <span className="pico-library-browser-empty-kicker">NOTHING MATCHES</span>
            <span>Try fewer letters, or a different collection.</span>
          </p>
        ) : (
          <ul className="pico-library-browser-list">
            {library.results.map((game) => (
              <PicoResultRow game={game} key={game.id} onOpen={() => onOpen(game.id)} />
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}
