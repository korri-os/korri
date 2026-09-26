import { useEffect, useRef, useState } from "react"
import { picoStatsFor } from "../../pico-detail-view"
import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoTally } from "../atoms/PicoTally"
import { PicoCart } from "../molecules/PicoCart"
import { PicoGameStage } from "./PicoGameStage"

/**
 * The shelf: every game as a small cartridge standing on one line, and the
 * chosen one large on the stage above it.
 *
 * Carts on the shelf all have the same width and take their height from their
 * art, so a tall box stands tall and a wide header lies low, all on one
 * baseline. Focus is the selection: the stage follows the d-pad, and the
 * shelf scrolls only as far as it must to keep the chosen cart in view.
 *
 * The index is view state, not app state: nothing outside this section needs
 * to know where the cursor is.
 */
export function PicoCartShelf({
  games,
  onOpen,
}: {
  readonly games: readonly PicoShelfGame[]
  readonly onOpen: (gameId: string) => void
}) {
  const [focusedIndex, setFocusedIndex] = useState(0)
  const rackRef = useRef<HTMLUListElement>(null)
  const focused = games[focusedIndex] ?? games[0]

  useEffect(() => {
    const rack = rackRef.current
    const slot = rack?.children[focusedIndex]
    if (rack === null || !(slot instanceof HTMLElement)) return
    /* Sets the rack's own scroll rather than calling scrollIntoView, which
     * walks up and moves every scrollable ancestor with it — a surface that can
     * scroll the page it is embedded in misbehaves inside any host that stacks
     * it with anything else. One cart of margin either side, so the neighbour
     * shows there is more. */
    const margin = slot.offsetWidth
    const left = slot.offsetLeft - rack.scrollLeft
    const right = left + slot.offsetWidth
    if (right > rack.clientWidth - margin) {
      rack.scrollLeft += right - (rack.clientWidth - margin)
    } else if (left < margin) {
      rack.scrollLeft -= margin - left
    }
  }, [focusedIndex])

  if (focused === undefined) return null

  return (
    <section className="pico-cart-shelf">
      <div className="pico-cart-shelf-stage">
        <PicoGameStage
          artUrl={focused.artUrl}
          id={focused.id}
          kicker={focused.section?.toUpperCase()}
          resumable={focused.resumable}
          stats={picoStatsFor(focused)}
          subtitle={focused.subtitle}
          title={focused.title}
        >
          <span className="pico-cart-shelf-tally">
            <PicoTally position={focusedIndex + 1} total={games.length} />
          </span>
        </PicoGameStage>
      </div>
      <ul aria-label="Shelf" className="pico-cart-shelf-rack" ref={rackRef}>
        {games.map((game, index) => (
          <li className="pico-cart-shelf-slot" key={game.id}>
            <PicoCart
              artUrl={game.artUrl}
              id={game.id}
              onActivate={() => onOpen(game.id)}
              onFocus={() => setFocusedIndex(index)}
              placement={index === focusedIndex ? "hero" : "side"}
              resumable={game.resumable ?? false}
              subtitle={game.subtitle}
              title={game.title}
            />
          </li>
        ))}
      </ul>
    </section>
  )
}
