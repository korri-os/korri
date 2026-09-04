import { useState } from "react"
import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoTally } from "../atoms/PicoTally"
import { PicoCart } from "../molecules/PicoCart"

/**
 * The shelf: every game as a cartridge, the focused one held in the middle.
 *
 * Focus is the selection. The shelf tracks which cart has it so the hero and
 * the caption follow the d-pad, and the browser's own focus scrolling keeps
 * that cart on screen — no scroll maths, and it behaves identically for a
 * thumbstick, a Tab key, and a screen reader's cursor.
 *
 * The index is view state, not app state: nothing outside this section needs to
 * know where the cursor is, and hoisting it would make every neighbour re-render
 * on a movement that concerns only the shelf.
 */
export function PicoCartShelf({
  games,
  onLaunch,
}: {
  readonly games: readonly PicoShelfGame[]
  readonly onLaunch: (gameId: string) => void
}) {
  const [focusedIndex, setFocusedIndex] = useState(0)
  const focused = games[focusedIndex] ?? games[0]

  return (
    <section className="pico-cart-shelf">
      <ul className="pico-cart-shelf-strip">
        {games.map((game, index) => (
          <li className="pico-cart-shelf-slot" key={game.id}>
            <PicoCart
              artUrl={game.artUrl}
              onActivate={() => onLaunch(game.id)}
              onFocus={() => setFocusedIndex(index)}
              placement={index === focusedIndex ? "hero" : "side"}
              resumable={game.resumable ?? false}
              subtitle={game.subtitle}
              title={game.title}
            />
          </li>
        ))}
      </ul>
      <div className="pico-cart-shelf-caption">
        <h1 className="pico-cart-shelf-title">{focused?.title ?? ""}</h1>
        <p className="pico-cart-shelf-subtitle">{focused?.subtitle ?? ""}</p>
        <PicoTally position={focusedIndex + 1} total={games.length} />
      </div>
    </section>
  )
}
