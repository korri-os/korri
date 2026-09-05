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
  // Doubled so the rail can travel a full width and meet itself.
  const rail = [...games.slice(0, 6), ...games.slice(0, 6)]
  return (
    <div aria-label="Attract" className="pico-attract" role="img">
      <span className="pico-attract-wordmark">PICO</span>
      <div className="pico-attract-rail">
        {rail.map((game, index) => (
          <span className="pico-attract-cart" key={`${game.id}-${index}`}>
            <PicoCart artUrl={game.artUrl} id={game.id} placement="still" title={game.title} />
          </span>
        ))}
      </div>
      <span className="pico-attract-hint">PRESS ANY BUTTON</span>
    </div>
  )
}
