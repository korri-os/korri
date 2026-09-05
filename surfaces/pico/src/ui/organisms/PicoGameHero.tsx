import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoSub } from "../atoms/PicoSub"
import { PicoTitle } from "../atoms/PicoTitle"
import { PicoCart } from "../molecules/PicoCart"
import { PicoKeyArtStage } from "../molecules/PicoKeyArtStage"
import { PicoStatRun } from "../molecules/PicoStatRun"

/**
 * One game, large, on its own key art.
 *
 * Legacy drew this three times — a spotlight, a last-played hero and a plain
 * hero — and they were one role with three names. This is the role: the game
 * the screen is about, stated as big as the screen allows.
 *
 * The reason it leads is printed above the title. Korri publishes no featured
 * flag, so the rule is Pico's, and a rule the user cannot see reads as an
 * endorsement the device has no basis for.
 */
export function PicoGameHero({
  game,
  reason,
  stats,
  onOpen,
}: {
  readonly game: PicoShelfGame
  readonly reason?: string
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  readonly onOpen: () => void
}) {
  return (
    <section aria-label={game.title} className="pico-game-hero">
      <PicoKeyArtStage src={game.wideArtUrl} />
      <button
        aria-label={game.subtitle === undefined ? game.title : `${game.title}, ${game.subtitle}`}
        className="pico-game-hero-open"
        onClick={onOpen}
        type="button"
      >
        <span className="pico-game-hero-art">
          <PicoCart artUrl={game.artUrl} id={game.id} placement="still" title={game.title} />
        </span>
        <span className="pico-game-hero-info">
          {reason === undefined ? null : (
            <span className="pico-game-hero-reason">{reason}</span>
          )}
          <PicoTitle size="lg" text={game.title} />
          {game.subtitle === undefined ? null : <PicoSub text={game.subtitle} />}
          <PicoStatRun stats={stats} />
        </span>
      </button>
    </section>
  )
}
