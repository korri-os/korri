import type { SurfaceAction } from "@contracts/surface/korri-surface"
import type { PicoDetailView } from "../pico-detail-view"
import type { PicoShelfGame } from "../pico-shelf-game"
import { PicoKeyArtStage } from "../ui/molecules/PicoKeyArtStage"
import { PicoModal } from "../ui/organisms/PicoModal"
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
  actions,
  askingAction,
  placing,
  onPlay,
  onRunAction,
  onConfirmAction,
  onCancelAction,
  onChooseLocation,
  clockLabel,
}: {
  readonly game: PicoDetailView
  /** What Korri says can be done to this game. Usually empty. */
  readonly actions: readonly SurfaceAction[]
  /** A destructive game action awaiting the user's yes, when one is. */
  readonly askingAction?: SurfaceAction
  /** The game whose launch location is being chosen, when one is. */
  readonly placing?: PicoShelfGame
  readonly onPlay: () => void
  readonly onRunAction: (action: SurfaceAction) => void
  readonly onConfirmAction: () => void
  readonly onCancelAction: () => void
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
        <PicoGameDetailBody
          actions={actions}
          game={game}
          onPlay={onPlay}
          onRunAction={onRunAction}
        />
      ) : (
        <PicoLocationPicker
          locations={placing.locations ?? []}
          onChoose={onChooseLocation}
          title={placing.title}
        />
      )}
      {askingAction === undefined ? null : (
        <PicoModal
          confirmLabel={askingAction.label.toUpperCase()}
          message={
            askingAction.description ??
            `${game.title} — this cannot be undone.`
          }
          onCancel={onCancelAction}
          onConfirm={onConfirmAction}
          title={`${askingAction.label.toUpperCase()}?`}
        />
      )}
    </PicoScreenShell>
  )
}
