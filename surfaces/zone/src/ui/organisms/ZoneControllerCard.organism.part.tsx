import { controllerPad, controllersFour } from "../../fixtures/fixture-layers"
import { ZoneControllerCard } from "./ZoneControllerCard"

export const name = "Controller card"
export const note = "COMPETITION's character card: WAITING on stripes, then the controller's name and seat"

/** The empty seat: WAITING on a white slab across the stripes. */
export default function Waiting() {
  return <ZoneControllerCard />
}

/** Heard: the red frame, the stripe well with initials, 1P, the name between triangles. */
export function Heard() {
  return <ZoneControllerCard name={controllerPad.name} seat={1} />
}

/** The second seat: 2P in red. */
export function SecondSeat() {
  return <ZoneControllerCard name={controllersFour[1]?.name} seat={2} />
}

/** The longest name Korri reports: it wraps along the foot. */
export function LongName() {
  return <ZoneControllerCard name={controllersFour[3]?.name} seat={4} />
}
