import { picoDetailViewFromGame } from "../pico-detail-view"
import { picoCollectionsFrom, picoHeroPick } from "../pico-library-view"
import type { PicoScreenView } from "../pico-screen-view"
import type { PicoShelfGame } from "../pico-shelf-game"
import { PicoNotice } from "../ui/molecules/PicoNotice"
import { PicoCartGrid } from "../ui/organisms/PicoCartGrid"
import { PicoCartShelf } from "../ui/organisms/PicoCartShelf"
import { PicoGameHero } from "../ui/organisms/PicoGameHero"
import { PicoResumeList } from "../ui/organisms/PicoResumeList"
import { PicoLaunchStage } from "../ui/organisms/PicoLaunchStage"
import { PicoLocationPicker } from "../ui/organisms/PicoLocationPicker"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

/** Hints for a shelf the user can act on. */
const SHELF_HINTS = [
  { hintKey: "a", label: "PLAY" },
  { hintKey: "b", label: "BACK" },
] as const

/** With nothing to act on, the only honest hint left is the way out. */
const QUIET_HINTS = [{ hintKey: "b", label: "BACK" }] as const

/* The mode is in the breadcrumb rather than a badge of its own: the user is
 * already reading that line to know where they are. */
const MODE_LABELS: Record<PicoHomeMode, string> = {
  shelf: "PICO ▸ LIBRARY",
  grid: "PICO ▸ LIBRARY · GRID",
  hero: "PICO ▸ LIBRARY · HERO",
}

/**
 * Pico's home screen.
 *
 * Every state it can be in — reading the library, empty, failed to read,
 * showing the shelf, asking where to play, starting a game, running one,
 * failing to start one — shares one frame and differs only in the body, so the
 * chrome never jumps. Which state is showing was decided upstream; this file
 * renders the answer and does not re-derive it.
 *
 * Failure copy is Korri's own, passed through untouched: the surface cannot
 * tell a missing file from an unreachable host, and guessing would put a wrong
 * explanation in front of the user.
 */
export type PicoHomeMode = "shelf" | "grid" | "hero"

export function PicoHome({
  view,
  mode,
  placing,
  onOpenGame,
  onChooseLocation,
  onRetry,
  onDismiss,
  clockLabel,
}: {
  readonly view: PicoScreenView
  /** How the library is laid out. Cycled by the treaty's `menu` button. */
  readonly mode: PicoHomeMode
  /** The game whose launch location is being chosen, when one is. */
  readonly placing?: PicoShelfGame
  /** Selecting a cart opens the game's own screen; launching happens there. */
  readonly onOpenGame: (gameId: string) => void
  readonly onChooseLocation: (locationId: string) => void
  readonly onRetry: () => void
  readonly onDismiss: () => void
  readonly clockLabel?: string
}) {
  const asking = placing !== undefined && view._tag === "Shelf"

  /* Working states get the weave; everything else sits on the starfield. A
   * screen that is waiting should look busier than one that is merely idle. */
  const backdrop =
    view._tag === "Busy" || view._tag === "Running" ? "dither" : "stars"

  return (
    <PicoScreenShell
      backdrop={backdrop}
      clockLabel={clockLabel}
      hints={view._tag === "Shelf" && !asking ? SHELF_HINTS : QUIET_HINTS}
      label={MODE_LABELS[mode]}
    >
      {asking && placing !== undefined ? (
        <PicoLocationPicker
          locations={placing.locations ?? []}
          onChoose={onChooseLocation}
          title={placing.title}
        />
      ) : null}

      {view._tag === "Shelf" && !asking && mode === "shelf" ? (
        <PicoCartShelf games={view.games} onOpen={onOpenGame} />
      ) : null}

      {view._tag === "Shelf" && !asking && mode === "hero" ? (
        <div className="pico-home-hero">
          {(() => {
            const pick = picoHeroPick(view.games)
            if (pick === undefined) return null
            return (
              <PicoGameHero
                game={pick.game}
                onOpen={() => onOpenGame(pick.game.id)}
                reason={pick.reason}
                stats={picoDetailViewFromGame(pick.game).stats}
              />
            )
          })()}
          <PicoResumeList
            games={view.games.filter((game) => game.resumable === true)}
            onOpen={onOpenGame}
          />
        </div>
      ) : null}

      {view._tag === "Shelf" && !asking && mode === "grid" ? (
        <PicoCartGrid
          collections={picoCollectionsFrom(view.games)}
          onOpen={onOpenGame}
        />
      ) : null}

      {view._tag === "Loading" ? (
        <PicoNotice
          kicker="READING CARTS"
          message="Korri is looking through your library."
          tone="info"
        />
      ) : null}

      {view._tag === "Empty" ? (
        <PicoNotice
          kicker="NO CARTS"
          message="Nothing to play yet. Add games to your library and they appear here."
          tone="info"
        />
      ) : null}

      {view._tag === "Failed" ? (
        <PicoNotice
          actions={[{ label: "TRY AGAIN", onPress: onRetry }]}
          kicker="SHELF JAMMED"
          message={view.message}
          tone="warn"
        />
      ) : null}

      {view._tag === "Busy" ? (
        <PicoLaunchStage detail={view.detail} kicker={view.kicker} />
      ) : null}

      {view._tag === "Running" ? (
        <PicoLaunchStage gameTitle={view.gameTitle} kicker={view.kicker} />
      ) : null}

      {view._tag === "Problem" ? (
        <PicoNotice
          actions={
            view.canRetry
              ? [
                  { label: "TRY AGAIN", onPress: onRetry },
                  { label: "OK", onPress: onDismiss },
                ]
              : [{ label: "OK", onPress: onDismiss }]
          }
          kicker={view.kicker}
          message={
            view.gameTitle === undefined
              ? view.reason
              : `${view.gameTitle} — ${view.reason}`
          }
          tone="warn"
        />
      ) : null}
    </PicoScreenShell>
  )
}
