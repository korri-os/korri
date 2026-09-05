import type { PicoShelfGame } from "../../pico-shelf-game"

/**
 * One search result: title, provenance, and a mark when it resumes.
 *
 * A row rather than a cart, because a result list is read down the left edge
 * and a grid of carts makes the eye hunt. The cart is the shelf's idiom; the
 * list is the search's.
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
        <span aria-hidden className="pico-result-row-mark">
          {game.resumable === true ? "▸" : "·"}
        </span>
        <span className="pico-result-row-title">{game.title}</span>
        {game.subtitle === undefined ? null : (
          <span className="pico-result-row-meta">{game.subtitle}</span>
        )}
      </button>
    </li>
  )
}
