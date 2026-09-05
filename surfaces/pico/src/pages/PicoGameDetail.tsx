import type { PicoDetailView } from "../pico-detail-view"
import type { PicoShelfGame } from "../pico-shelf-game"
import { PicoKeyArtStage } from "../ui/molecules/PicoKeyArtStage"
import { PicoGameDetail as PicoGameDetailBody } from "../ui/organisms/PicoGameDetail"
import { PicoLocationPicker } from "../ui/organisms/PicoLocationPicker"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

const DETAIL_HINTS = [
  { hintKey: "a", label: "PLAY" },
  { hintKey: "b", label: "BACK" },
] as const

const QUIET_HINTS = [{ hintKey: "b", label: "BACK" }] as const

/**
 * A game's own screen: the second of the two routes legacy actually shipped.
 *
 * Same shell as home so the chrome does not jump; the body is the game. When a
 * launch needs a location the question replaces the body, exactly as it does
 * over the shelf, so the user learns one way of being asked.
 */
export function PicoGameDetail({
  game,
  placing,
  onPlay,
  onChooseLocation,
  clockLabel,
}: {
  readonly game: PicoDetailView
  readonly placing?: PicoShelfGame
  readonly onPlay: () => void
  readonly onChooseLocation: (locationId: string) => void
  readonly clockLabel?: string
}) {
  return (
    <PicoScreenShell
      backdrop="stars"
      clockLabel={clockLabel}
      hints={placing === undefined ? DETAIL_HINTS : QUIET_HINTS}
      label="PICO ▸ GAME"
    >
      <PicoKeyArtStage src={game.wideArtUrl} />
      {placing === undefined ? (
        <PicoGameDetailBody game={game} onPlay={onPlay} />
      ) : (
        <PicoLocationPicker
          locations={placing.locations ?? []}
          onChoose={onChooseLocation}
          title={placing.title}
        />
      )}
    </PicoScreenShell>
  )
}
