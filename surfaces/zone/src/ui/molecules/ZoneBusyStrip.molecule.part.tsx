import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import { fixtureBusy, fixtureCancel, fixtureStop } from "../../fixtures/fixture-dialogs"
import {
  statusProgressKnown,
  statusProgressNearlyDone,
  statusProgressNoTotal,
  statusProgressUnpacking,
} from "../../fixtures/fixture-layers"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneBusyStrip } from "./ZoneBusyStrip"

export const name = "Busy strip"
export const note = "Korri's kicker on a black parallelogram, the detail under it, its progress as a red slanted bar, its actions beside"

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

/** Unexported: a fixture status drawn as the status layer draws it. */
function Progress({ status }: { readonly status: SurfaceStatus }) {
  if (status._tag !== "Busy") return null
  return (
    <ZoneBusyStrip
      actions={status.actions}
      detail={status.detail}
      focused={(status.actions?.length ?? 0) > 0 ? 0 : undefined}
      kicker={status.kicker}
      progress={status.progress}
    />
  )
}

/** Korri knows the end: about half the red bar filled, 612.0 MB / 1.2 GB at its right end. */
export function ProgressKnown() {
  return <Progress status={statusProgressKnown} />
}

/** Korri knows how far, not how far it goes: the stripe moves along the bar, the size done beside it. */
export function ProgressNoTotal() {
  return <Progress status={statusProgressNoTotal} />
}

/** One step from the end: the fill all but meets the bar's slanted end. */
export function ProgressNearlyDone() {
  return <Progress status={statusProgressNearlyDone} />
}

/** Unpacking: no size done and no end, so the stripe alone and no action. */
export function ProgressUnpacking() {
  return <Progress status={statusProgressUnpacking} />
}

/** A known total in a column: the sizes drop under the bar before the bar starves. */
export function ProgressNarrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Progress status={statusProgressKnown} />
    </ZonePartFrame>
  )
}
