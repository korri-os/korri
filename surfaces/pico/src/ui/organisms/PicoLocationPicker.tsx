import type { PicoShelfLocation } from "../../pico-shelf-game"
import { PicoRow } from "../atoms/PicoRow"
import { PicoCard } from "../molecules/PicoCard"

/**
 * Where should this game run?
 *
 * Asked only when Korri says there is a real choice. The locations are Korri's
 * own, in Korri's order and with Korri's labels; picking the first one for the
 * user would start a game on the wrong machine, which is the one launch
 * mistake that cannot be undone from the couch. A yellow card, because it is
 * a question, and the only thing on screen while it is asked.
 */
export function PicoLocationPicker({
  title,
  locations,
  onChoose,
}: {
  readonly title: string
  readonly locations: readonly PicoShelfLocation[]
  readonly onChoose: (locationId: string) => void
}) {
  return (
    <section aria-label="PLAY WHERE?" className="pico-location-picker">
      <PicoCard kicker="PLAY WHERE?" title={title} tone="ask">
        <ul className="pico-location-picker-list">
          {locations.map((location) => (
            <li className="pico-location-picker-item" key={location.id}>
              <PicoRow label={location.label} onPress={() => onChoose(location.id)} />
            </li>
          ))}
        </ul>
      </PicoCard>
    </section>
  )
}
