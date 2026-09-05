import type { SurfaceAction } from "@contracts/surface/korri-surface"
import type { PicoDetailView } from "../../pico-detail-view"
import { PicoButton } from "../atoms/PicoButton"
import { PicoDetailHead } from "../molecules/PicoDetailHead"
import { PicoStatRun } from "../molecules/PicoStatRun"
import { PicoGameActions } from "./PicoGameActions"

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
  actions,
  onPlay,
  onRunAction,
}: {
  readonly game: PicoDetailView
  /** What Korri says can be done to this game. Usually empty. */
  readonly actions: readonly SurfaceAction[]
  readonly onPlay: () => void
  readonly onRunAction: (action: SurfaceAction) => void
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
      <PicoGameActions actions={actions} onRun={onRunAction} />
    </section>
  )
}
