import type { SurfaceAction } from "@contracts/surface/korri-surface"
import type { PicoDetailView } from "../pico-detail-view"
import type { PicoShelfGame } from "../pico-shelf-game"
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
 * A game's own screen.
 *
 * The header reads "library › the game": Back leads to the library, and the
 * game is where you are. A launch location question and a destructive
 * action's confirmation both take the whole body while they are asked.
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
  readonly actions: readonly SurfaceAction[]
  readonly askingAction?: SurfaceAction
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
      clockLabel={clockLabel}
      hints={placing === undefined ? DETAIL_HINTS : QUIET_HINTS}
      label={game.title}
      place="LIBRARY ›"
    >
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
