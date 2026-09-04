import { PicoBackdrop } from "./PicoBackdrop"

export const name = "Backdrop"
export const note = "The moving ground — drifting stars or a stepped dither weave"

export default function PicoBackdropPart() {
  return <PicoBackdrop field="stars" />
}
