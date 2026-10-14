import "./PicoControllerCheck.css"
import type { PicoPadsView } from "../../pico-onboarding-view"
import { PicoRow } from "../atoms/PicoRow"
import { PicoControllerRow } from "../molecules/PicoControllerRow"
import { PicoSetupHeading } from "../molecules/PicoSetupHeading"
import { PicoSettingRow } from "../molecules/PicoSettingRow"

/**
 * Which controllers will play. Every controller the host sees is listed with
 * a dark lamp; a button pressed on it lights the lamp. Hearing "any button"
 * is the host's job: the surface never sees a key. NEXT waits for one lit
 * controller, unless the host lists none to hear.
 *
 * How many players there are is Korri's player-seats setting, drawn and
 * changed the way Settings does: A on the row moves to the next value.
 */
export function PicoControllerCheck({
  view,
  onSeats,
  onDismissProblem,
  onContinue,
}: {
  readonly view: PicoPadsView
  /** A on the seats row: Korri's next value. */
  readonly onSeats: () => void
  readonly onDismissProblem: () => void
  readonly onContinue: () => void
}) {
  const heard = view.controllers.filter((controller) => controller.heard).length
  const seats = view.seats
  const refused = seats !== undefined && typeof seats.state === "object"
  return (
    <section aria-label="Controllers" className="pico-controller-check">
      <PicoSetupHeading
        message={view.controllers.length === 0
          ? "Korri sees no controllers yet. Connect one now, or later from its own menu."
          : heard === 0
            ? "Press a button on each controller you will play with. Its lamp lights when Korri hears it."
            : "Another controller joins when you press a button on it."}
        title={view.controllers.length === 0 ? "No controllers yet"
          : heard === 0 ? "Press any button"
          : heard === 1 ? "1 controller ready" : `${heard} controllers ready`}
      />
      {view.controllers.length === 0 ? null : (
        <ul className="pico-controller-check-list">
          {view.controllers.map((controller) => (
            <PicoControllerRow controller={controller} key={controller.id} />
          ))}
        </ul>
      )}
      {seats === undefined ? null : (
        <ul className="pico-controller-check-seats">
          <PicoSettingRow onActivate={onSeats} row={seats} />
        </ul>
      )}
      <div className="pico-controller-check-next">
        {refused ? <PicoRow label="OK" onPress={onDismissProblem} /> : null}
        <PicoRow disabled={!view.ready} label="NEXT" onPress={onContinue} />
      </div>
    </section>
  )
}
