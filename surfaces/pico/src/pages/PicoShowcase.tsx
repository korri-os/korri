import "./PicoShowcase.css"
import type { PicoShowcaseView } from "../pico-showcase-view"
import { PicoTally } from "../ui/atoms/PicoTally"
import { PicoEmptySlot } from "../ui/molecules/PicoEmptySlot"
import { PicoGameStage } from "../ui/organisms/PicoGameStage"
import { PicoLaunchStage } from "../ui/organisms/PicoLaunchStage"

/**
 * The display that shows, when Pico has more than one.
 *
 * It draws one thing at a time and holds no control: the game the cursor
 * chose, on the same stage the shelf uses, as large as the display allows;
 * a game starting, running or paused, as the launch stage draws it; or an
 * empty slot. The cursor, the buttons and every word that asks for a choice
 * stand on the other display, so the host's focus never has to come here.
 */
export function PicoShowcase({ view }: { readonly view: PicoShowcaseView }) {
  switch (view._tag) {
    case "Game": {
      const game = view.game
      return (
        <div className="pico-showcase">
          <PicoGameStage
            artUrl={game.artUrl}
            download={game.download}
            id={game.id}
            kicker={game.kicker}
            resumable={game.resumable}
            stats={game.stats}
            subtitle={game.subtitle}
            title={game.title}
          >
            {game.position === undefined ? null : (
              <PicoTally position={game.position.index} total={game.position.total} />
            )}
          </PicoGameStage>
        </div>
      )
    }
    case "Launch":
      return (
        <div className="pico-showcase">
          <PicoLaunchStage
            cart={view.cart}
            detail={view.detail}
            gameTitle={view.gameTitle}
            kicker={view.kicker}
            phase={view.phase}
            progress={view.progress}
          />
        </div>
      )
    case "Idle":
      return (
        <div className="pico-showcase">
          <PicoEmptySlot />
        </div>
      )
  }
}
