import { useState } from "react"
import type { PicoHomeView } from "../pico-home-view"
import type { PicoShelfGame } from "../pico-shelf-game"
import { PicoNotice } from "../ui/molecules/PicoNotice"
import { PicoCartShelf } from "../ui/organisms/PicoCartShelf"
import { PicoLocationPicker } from "../ui/organisms/PicoLocationPicker"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

/** Hints for a shelf the user can act on. */
const SHELF_HINTS = [
  { hintKey: "a", label: "PLAY" },
  { hintKey: "b", label: "BACK" },
] as const

/** With nothing to act on, the only honest hint left is the way out. */
const QUIET_HINTS = [{ hintKey: "b", label: "BACK" }] as const

/**
 * Pico's home screen.
 *
 * Loading, empty, failed, the shelf, and the launch-location question are five
 * views of one thing, so they share one frame and differ only in the body — the
 * chrome never jumps between states. The failure message is Korri's own copy,
 * passed through untouched, because the surface cannot tell a disk error from a
 * network one and guessing would put a wrong explanation in front of the user.
 *
 * Which game is being placed is view state: it exists only between a press and
 * a choice, and Korri neither knows nor needs to know about the question.
 */
export function PicoHome({
  view,
  onLaunch,
  onRetry,
  clockLabel,
}: {
  readonly view: PicoHomeView
  readonly onLaunch: (gameId: string, launchLocationId?: string) => void
  readonly onRetry: () => void
  readonly clockLabel?: string
}) {
  const [placing, setPlacing] = useState<PicoShelfGame | undefined>(undefined)

  const requestLaunch = (game: PicoShelfGame) => {
    if (game.locations === undefined || game.locations.length === 0) {
      onLaunch(game.id)
      return
    }
    setPlacing(game)
  }

  const chooseLocation = (locationId: string) => {
    if (placing === undefined) return
    onLaunch(placing.id, locationId)
    setPlacing(undefined)
  }

  const shelfGames = view._tag === "Shelf" ? view.games : []
  const placingLocations = placing?.locations ?? []

  return (
    <PicoScreenShell
      clockLabel={clockLabel}
      hints={view._tag === "Shelf" ? SHELF_HINTS : QUIET_HINTS}
      label="PICO ▸ LIBRARY"
    >
      {placing !== undefined ? (
        <PicoLocationPicker
          locations={placingLocations}
          onChoose={chooseLocation}
          title={placing.title}
        />
      ) : null}
      {placing === undefined && view._tag === "Shelf" ? (
        <PicoCartShelf
          games={shelfGames}
          onLaunch={(gameId) => {
            const game = shelfGames.find((candidate) => candidate.id === gameId)
            if (game !== undefined) requestLaunch(game)
          }}
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
          kicker="SHELF JAMMED"
          message={view.message}
          onRetry={onRetry}
          retryLabel="TRY AGAIN"
          tone="warn"
        />
      ) : null}
    </PicoScreenShell>
  )
}
