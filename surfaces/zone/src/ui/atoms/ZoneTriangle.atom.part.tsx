import { ZoneTriangle } from "./ZoneTriangle"

export const name = "Triangle"
export const note = "The red cursor, pointing at its target and nudging toward it"

export default function Default() {
  return <ZoneTriangle direction="right" />
}

export function Left() {
  return <ZoneTriangle direction="left" />
}

export function Right() {
  return <ZoneTriangle direction="right" />
}

/** The end triangle at the first or last card: dimmed, and still. */
export function Dimmed() {
  return <ZoneTriangle direction="left" dimmed />
}

/** The narrow band's smaller cursor. */
export function Narrow() {
  return <ZoneTriangle direction="right" size="narrow" />
}

/** Beside a button: sized by the type around it. */
export function Inline() {
  return <ZoneTriangle direction="right" size="inline" />
}

/** What reduced motion shows: the triangle at its near distance, not breathing. */
export function ReducedMotion() {
  return <ZoneTriangle direction="right" nudge={false} />
}
