import { fixtureBusy, fixtureCancel, fixtureStop } from "../../fixtures/fixture-dialogs"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneBusyStrip } from "./ZoneBusyStrip"

export const name = "Busy strip"
export const note = "Korri's kicker on a black parallelogram, the detail under it, its actions beside"

const busy = fixtureBusy._tag === "Busy" ? fixtureBusy : undefined
const kicker = busy?.kicker ?? ""
const detail = busy?.detail

/** Korri said only the kicker. */
export default function KickerOnly() {
  return <ZoneBusyStrip kicker={kicker} />
}

/** The kicker with its detail under it. */
export function KickerAndDetail() {
  return <ZoneBusyStrip detail={detail} kicker={kicker} />
}

/** One host action, CANCEL, beside the panel and focused. */
export function OneAction() {
  return <ZoneBusyStrip actions={[fixtureCancel]} detail={detail} focused={0} kicker={kicker} />
}

/** Two host actions; the second focused. */
export function TwoActions() {
  return <ZoneBusyStrip actions={[fixtureCancel, fixtureStop]} detail={detail} focused={1} kicker={kicker} />
}

/** A long detail in a column: the panel wraps its words; the action drops under it. */
export function LongDetail() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneBusyStrip
        actions={[fixtureCancel]}
        detail="Waiting for zao to answer. It was last seen on hummingbird-lake four minutes ago and may still be waking from sleep, which can take up to a minute on that device."
        focused={0}
        kicker={kicker}
      />
    </ZonePartFrame>
  )
}
