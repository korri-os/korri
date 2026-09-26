import type { SurfaceAction } from "@contracts/surface/korri-surface"
import type { PicoDetailView } from "../../pico-detail-view"
import { PicoButton } from "../atoms/PicoButton"
import { PicoGameActions } from "./PicoGameActions"
import { PicoGameStage } from "./PicoGameStage"

/**
 * A game's own screen: the same stage the shelf shows, plus the one verb that
 * matters and whatever else Korri offers to do with the game.
 *
 * Play (or Continue) is the only `go` button; Korri's own game actions sit
 * below it as a quieter list, so the thing people open a game to do is the
 * thing the cursor lands on.
 */
export function PicoGameDetail({
  game,
  actions,
  onPlay,
  onRunAction,
}: {
  readonly game: PicoDetailView
  readonly actions: readonly SurfaceAction[]
  readonly onPlay: () => void
  readonly onRunAction: (action: SurfaceAction) => void
}) {
  return (
    <div className="pico-game-detail">
      <PicoGameStage
        artUrl={game.artUrl}
        id={game.id}
        resumable={game.primaryLabel === "CONTINUE"}
        stats={game.stats}
        subtitle={game.subtitle}
        title={game.title}
      >
        <div className="pico-game-detail-actions">
          <PicoButton label={`▶ ${game.primaryLabel}`} onPress={onPlay} />
        </div>
        <PicoGameActions actions={actions} onRun={onRunAction} />
      </PicoGameStage>
    </div>
  )
}
