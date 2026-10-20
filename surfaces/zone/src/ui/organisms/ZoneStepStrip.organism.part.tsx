import { ZoneStepStrip } from "./ZoneStepStrip"

export const name = "Step strip"
export const note = "First boot: the banner names the step, four diamond pips under its lip count the way"

const STEPS = ["Controllers", "Wi-Fi", "Name", "Identity"]

/** The first step: one gold pip. */
export default function First() {
  return <ZoneStepStrip current={0} steps={STEPS} />
}

/** The second step: WI-FI, two gold pips. */
export function Second() {
  return <ZoneStepStrip current={1} steps={STEPS} />
}

/** The last step: IDENTITY, every pip gold. */
export function Last() {
  return <ZoneStepStrip current={3} steps={STEPS} />
}
