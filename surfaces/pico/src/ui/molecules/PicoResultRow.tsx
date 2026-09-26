import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoCart } from "./PicoCart"

/**
 * One game in a list of results: its cartridge standing on a short shelf
 * line, with the title and where it comes from written underneath on the
 * ground, not printed on the plastic.
 *
 * The whole tile is one button. The cartridge inside is a picture of the game,
 * so it takes no focus of its own. Titles wrap to two lines and then stop;
 * the full title is in the button's name.
 */
export function PicoResultRow({
  game,
  onOpen,
}: {
  readonly game: PicoShelfGame
  readonly onOpen: () => void
}) {
  return (
    <li className="pico-result-row-item">
      <button
        aria-label={game.subtitle === undefined ? game.title : `${game.title}, ${game.subtitle}`}
        className="pico-result-row"
        onClick={onOpen}
        type="button"
      >
        <span className="pico-result-row-art">
          <PicoCart
            artUrl={game.artUrl}
            id={game.id}
            placement="tile"
            resumable={game.resumable ?? false}
            title={game.title}
          />
        </span>
        <span className="pico-result-row-title">{game.title}</span>
        {game.subtitle === undefined ? null : (
          <span className="pico-result-row-meta">{game.subtitle}</span>
        )}
      </button>
    </li>
  )
}
