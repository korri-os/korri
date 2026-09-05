import type { PicoDetailView } from "../../pico-detail-view"
import { PicoButton } from "../atoms/PicoButton"
import { PicoStat } from "../atoms/PicoStat"
import { PicoDetailHead } from "../molecules/PicoDetailHead"

/**
 * One game, on its own screen.
 *
 * What legacy called the "page you land on after selecting a game". Everything
 * shown is a fact Korri stated: a game never played says so rather than showing
 * a zero it was not given, and the primary action's label comes from whether
 * Korri says the game resumes.
 */
export function PicoGameDetail({
  game,
  onPlay,
}: {
  readonly game: PicoDetailView
  readonly onPlay: () => void
}) {
  return (
    <section aria-label={game.title} className="pico-game-detail">
      <PicoDetailHead
        artUrl={game.artUrl}
        id={game.id}
        subtitle={game.subtitle}
        title={game.title}
      />
      <div className="pico-game-detail-stats">
        {game.stats.length === 0 ? (
          <span className="pico-game-detail-unplayed">NEVER PLAYED</span>
        ) : (
          game.stats.map((stat) => (
            <PicoStat caption={stat.caption} figure={stat.figure} key={stat.caption} />
          ))
        )}
      </div>
      <div className="pico-game-detail-actions">
        <PicoButton label={`▶ ${game.primaryLabel}`} onPress={onPlay} />
      </div>
    </section>
  )
}
