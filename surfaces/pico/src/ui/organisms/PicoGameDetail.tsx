import type { PicoDetailView } from "../../pico-detail-view"
import { PicoButton } from "../atoms/PicoButton"
import { PicoDetailHead } from "../molecules/PicoDetailHead"
import { PicoStatRun } from "../molecules/PicoStatRun"

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
      <PicoStatRun stats={game.stats} />
      <div className="pico-game-detail-actions">
        <PicoButton label={`▶ ${game.primaryLabel}`} onPress={onPlay} />
      </div>
    </section>
  )
}
