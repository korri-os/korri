import type { PicoShelfGame } from "../../pico-shelf-game"
import { PicoResultRow } from "../molecules/PicoResultRow"

/**
 * Everything Korri says can be picked up where it was left.
 *
 * Absent entirely when nothing resumes, rather than an empty heading: a run
 * called "Resume" with nothing under it tells the user their saves are missing.
 */
export function PicoResumeList({
  games,
  onOpen,
}: {
  readonly games: readonly PicoShelfGame[]
  readonly onOpen: (gameId: string) => void
}) {
  if (games.length === 0) return null
  return (
    <div className="pico-resume-list">
      <h2 className="pico-resume-list-title">RESUME</h2>
      <ul aria-label="Resume" className="pico-resume-list-run">
        {games.map((game) => (
          <PicoResultRow game={game} key={game.id} onOpen={() => onOpen(game.id)} />
        ))}
      </ul>
    </div>
  )
}
