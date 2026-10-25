import "./PicoGameHero.css"
import { picoDownloadLabel } from "../../pico-detail-view"
import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoButton } from "../atoms/PicoButton"
import { PicoGameStage } from "./PicoGameStage"

/**
 * One game, large, with one thing to do: open it.
 *
 * The reason it leads is printed above the title. Korri publishes no featured
 * flag, so the rule is Pico's, and a rule the user cannot see reads as an
 * endorsement the device has no basis for.
 *
 * With `stage="elsewhere"` another display shows the game, so the hero here
 * is its one thing to do: OPEN.
 */
export function PicoGameHero({
  game,
  reason,
  stats,
  onOpen,
  stage = "here",
}: {
  readonly game: PicoShelfGame
  readonly reason?: string
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  readonly onOpen: () => void
  /** Where the game's stage stands: on this screen, or on another display. */
  readonly stage?: "here" | "elsewhere"
}) {
  if (stage === "elsewhere") {
    return (
      <div className="pico-game-hero" data-stage={stage}>
        <span className="pico-game-hero-open">
          <PicoButton label="OPEN" onPress={onOpen} />
        </span>
      </div>
    )
  }
  return (
    <div className="pico-game-hero">
      <PicoGameStage
        artUrl={game.artUrl}
        download={picoDownloadLabel(game)}
        id={game.id}
        kicker={reason}
        resumable={game.resumable}
        stats={stats}
        subtitle={game.subtitle}
        title={game.title}
      >
        <span className="pico-game-hero-open">
          <PicoButton label="OPEN" onPress={onOpen} />
        </span>
      </PicoGameStage>
    </div>
  )
}
