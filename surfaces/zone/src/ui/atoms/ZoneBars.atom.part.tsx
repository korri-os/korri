import { ZoneBars } from "./ZoneBars"

export const name = "Bars"
export const note = "Four signal bars, one lit per quarter of strength, never none while connected"

/** 72%: three lit. */
export default function Default() {
  return <ZoneBars strengthPercent={72} />
}

/** Full strength: four lit. */
export function Full() {
  return <ZoneBars strengthPercent={100} />
}

/** 9%: still one lit, as a connection it is. */
export function Weak() {
  return <ZoneBars strengthPercent={9} />
}

/** No reading: one lit. */
export function NoReading() {
  return <ZoneBars />
}

/** Offline: none lit. */
export function Offline() {
  return <ZoneBars connected={false} />
}
