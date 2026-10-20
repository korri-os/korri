import { fixtureBackup200, fixtureDraft40 } from "../../fixtures/fixture-dialogs"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneQrWell } from "./ZoneQrWell"

export const name = "QR well"
export const note = "A QR code on white paper in a black well with the blue edge, as large as the well allows"

/** A short string: a small symbol, scaled up to the well. */
export default function ShortString() {
  return <ZoneQrWell label="Identity backup" value={fixtureDraft40} />
}

/** Two hundred characters: a dense symbol, still square and still crisp. */
export function LongString() {
  return <ZoneQrWell label="Identity backup" value={fixtureBackup200} />
}

/** A thumbnail well: the code takes the shorter side and keeps its quiet zone. */
export function TinyWell() {
  return (
    <ZonePartFrame shape="tiny">
      <ZoneQrWell label="Identity backup" value={fixtureBackup200} />
    </ZonePartFrame>
  )
}
