import { PicoNotice } from "./PicoNotice"

export const name = "Notice"
export const note = "Loading, empty, and failed are one view with a tone"

export default function PicoNoticePart() {
  return (
    <PicoNotice
      kicker="NOTHING HERE YET"
      message="Add games to your library and they will show up on the shelf."
      tone="info"
    />
  )
}
