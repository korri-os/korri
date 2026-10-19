import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneGround } from "./ZoneGround"

export const name = "Ground"
export const note = "Orbit rings, dots and two bands; one drawing that slices to its box"

export default function Default() {
  return <ZoneGround />
}

/** A column: the drawing's middle shows, the bands cross it. */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneGround />
    </ZonePartFrame>
  )
}

/** A strip: a band through the drawing's middle, the big ring's top and bottom cut. */
export function WideShort() {
  return (
    <ZonePartFrame shape="wide-short">
      <ZoneGround />
    </ZonePartFrame>
  )
}
