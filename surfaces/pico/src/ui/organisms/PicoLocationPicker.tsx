import type { PicoShelfLocation } from "../../pico-shelf-game"
import { PicoButton } from "../atoms/PicoButton"

/**
 * Where should this game run?
 *
 * Korri publishes locations only when there is a genuine choice, so this screen
 * appears only when the answer is unknown — and it takes the whole body rather
 * than floating over the shelf, because a modal on a handheld has to solve
 * focus trapping and a full-body panel simply does not have the problem.
 *
 * Korri orders the list (local device first, then stable host order); Pico
 * preserves that order rather than sorting by a rule of its own.
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
    <section className="pico-location-picker">
      <h1 className="pico-location-picker-title">{title}</h1>
      <p className="pico-location-picker-prompt">PLAY WHERE?</p>
      <ul className="pico-location-picker-list">
        {locations.map((location) => (
          <li className="pico-location-picker-item" key={location.id}>
            <PicoButton
              label={location.label}
              onPress={() => onChoose(location.id)}
            />
          </li>
        ))}
      </ul>
    </section>
  )
}
