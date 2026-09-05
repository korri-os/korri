import type { SurfaceAction } from "@contracts/surface/korri-surface"
import { PicoControlRow } from "../molecules/PicoControlRow"

/**
 * What Korri says can be done to this game, beside starting it.
 *
 * Absent entirely when Korri offers none — most games have none, and an empty
 * ACTIONS heading on every screen teaches the user to stop reading it.
 *
 * Reuses the overlay's control row rather than growing a second row that looks
 * the same. One difference is deliberate: a game action carries no
 * `disabledReason` — that field belongs to gameplay controls — so an inert
 * action is dimmed and explains itself through its description, or not at all.
 * Writing a reason here would be Pico inventing one.
 */
export function PicoGameActions({
  actions,
  onRun,
}: {
  readonly actions: readonly SurfaceAction[]
  readonly onRun: (action: SurfaceAction) => void
}) {
  if (actions.length === 0) return null
  return (
    <section aria-label="Actions" className="pico-game-actions">
      <h2 className="pico-game-actions-title">ACTIONS</h2>
      <ul className="pico-game-actions-list">
        {actions.map((action) => (
          <PicoControlRow
            control={{
              id: action.id,
              label: action.label,
              ...(action.description === undefined ? {} : { description: action.description }),
              enabled: action.enabled,
              destructive: action.destructive === true,
            }}
            key={action.id}
            onActivate={() => onRun(action)}
          />
        ))}
      </ul>
    </section>
  )
}
