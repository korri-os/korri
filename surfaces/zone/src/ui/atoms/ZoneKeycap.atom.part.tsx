import { ZoneKeycap } from "./ZoneKeycap"

export const name = "Keycap"
export const note = "The round coloured keycap and its word on a black slanted tab"

export default function Default() {
  return <ZoneKeycap button="B" word="Back" />
}

export function B() {
  return <ZoneKeycap button="B" word="Back" />
}

export function A() {
  return <ZoneKeycap button="A" word="Confirm" />
}

/** X alone: a row that names the key beside its own label. */
export function X() {
  return <ZoneKeycap button="X" />
}

export function Y() {
  return <ZoneKeycap button="Y" word="Game opts." />
}

/** A long word: the tab grows with it and keeps its tail. */
export function LongWord() {
  return <ZoneKeycap button="A" word="Play on hummingbird-lake" />
}
