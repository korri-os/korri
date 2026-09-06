import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoCart } from "../molecules/PicoCart"

/**
 * What the device shows when nobody is holding it.
 *
 * The library itself, drifting — not an advertisement, not a slideshow of
 * something Korri never mentioned. A cabinet's attract loop earns its place by
 * showing what you could be playing, and everything here is a game already on
 * the device.
 *
 * Marked as one image for assistive technology: it is decorative motion, and
 * announcing eight cart labels to someone who cannot see it drifting would be
 * noise. The screen underneath is what they are actually navigating.
 */
export function PicoAttract({ games }: { readonly games: readonly PicoShelfGame[] }) {
  const carts = games.slice(0, 6)
  return (
    <div aria-label="Attract" className="pico-attract" role="img">
      <span className="pico-attract-wordmark">PICO</span>
      <div className="pico-attract-rail">
        {[0, 1].map((copy) => (
          <div className="pico-attract-set" key={copy}>
            {carts.map((game) => (
              <span className="pico-attract-cart" key={game.id}>
                <PicoCart artUrl={game.artUrl} id={game.id} placement="still" title={game.title} />
              </span>
            ))}
          </div>
        ))}
      </div>
      <span className="pico-attract-hint">PRESS ANY BUTTON</span>
    </div>
  )
}
