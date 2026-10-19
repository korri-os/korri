import { ZoneCaps } from "./ZoneCaps"
import { ZonePanel } from "./ZonePanel"

export const name = "Panel"
export const note = "The black parallelogram with the blue offset edge; four tones"

/* Children go in as a prop so the part's root stays one sealed component
 * and Caliper can offer its knobs. */
const play = <ZoneCaps size="button">Play here</ZoneCaps>

export default function Default() {
  return <ZonePanel children={play} />
}

/** A sentence-length label: the panel grows with it and the lean stays. */
export function LongContent() {
  return (
    <ZonePanel
      children={<ZoneCaps size="button">The Legend of Zelda: Oracle of Seasons on hummingbird-lake</ZoneCaps>}
    />
  )
}

/** No drop edge: a tab or a foot panel that sits flat on the ground. */
export function NoEdge() {
  return <ZonePanel children={<ZoneCaps size="button">Options</ZoneCaps>} edge={false} />
}

export function Black() {
  return <ZonePanel children={play} tone="black" />
}

/** The dialog's tone: white caps keep their outline on blue. */
export function Blue() {
  return <ZonePanel children={<ZoneCaps size="button">Keep these settings?</ZoneCaps>} tone="blue" />
}

/** The HERE tag's tone: black ink, no lettering edge. */
export function Yellow() {
  return <ZonePanel children={<ZoneCaps size="button">Here</ZoneCaps>} tone="yellow" />
}

/** The keyboard slab's tone: black ink on white. */
export function White() {
  return <ZonePanel children={<ZoneCaps size="button">hummingbird-lake</ZoneCaps>} tone="white" />
}
