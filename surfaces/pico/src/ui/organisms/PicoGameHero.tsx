import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoButton } from "../atoms/PicoButton"
import { PicoGameStage } from "./PicoGameStage"

/**
 * One game, large, with one thing to do: open it.
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
    <div className="pico-game-hero">
      <PicoGameStage
        artUrl={game.artUrl}
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
