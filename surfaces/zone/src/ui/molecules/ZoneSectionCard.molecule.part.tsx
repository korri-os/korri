import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneSectionCard } from "./ZoneSectionCard"

export const name = "Section card"
export const note = "The spine that opens a section in the band: Korri's caption up it, the stripe down its edge"

/** A short caption. */
export default function Default() {
  return <ZoneSectionCard caption="Continue" />
}

/** A device's name. */
export function DeviceName() {
  return <ZoneSectionCard caption="This device" />
}

/** One long word with no break: it truncates up the spine. */
export function LongCaption() {
  return <ZoneSectionCard caption="Supercalifragilisticexpialidocious-handheld" />
}

/** In a side card's box. */
export function InCardBox() {
  return (
    <ZonePartFrame shape="card">
      <ZoneSectionCard caption="zao" />
    </ZonePartFrame>
  )
}
