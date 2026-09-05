import type { PicoCollection } from "../../pico-library-view"
import { PicoCart } from "../molecules/PicoCart"

/**
 * The whole library at once, in the groups Korri delivered it in.
 *
 * The shelf shows one game well; this shows a hundred games at all. The treaty
 * says games sharing a section arrive consecutively and the surface groups on
 * the change — a row per section is the most direct reading of that, and it is
 * what legacy's shelf grid drew.
 */
export function PicoCartGrid({
  collections,
  onOpen,
}: {
  readonly collections: readonly PicoCollection[]
  readonly onOpen: (gameId: string) => void
}) {
  return (
    <ul aria-label="Library by collection" className="pico-cart-grid">
      {collections.map((collection) => (
        <li className="pico-cart-grid-collection" key={collection.title}>
          <h2 className="pico-cart-grid-title">{collection.title}</h2>
          <ul className="pico-cart-grid-row">
            {collection.games.map((game) => (
              <li className="pico-cart-grid-slot" key={game.id}>
                <PicoCart
                  artUrl={game.artUrl}
                  id={game.id}
                  onActivate={() => onOpen(game.id)}
                  placement="side"
                  resumable={game.resumable}
                  subtitle={game.subtitle}
                  title={game.title}
                />
              </li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  )
}
