import { ZoneVeil } from "./ZoneVeil"

export const name = "Veil"
export const note = "The Mega Drive dither over an inert page: one art px of black on every other pixel"

/** The veil over the part's ground: half the pixels black, none blended. */
export default function Default() {
  return <ZoneVeil />
}
