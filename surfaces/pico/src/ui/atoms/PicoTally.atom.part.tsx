import { PicoTally } from "./PicoTally"

export const name = "Tally"
export const note = "Position in the shelf; readable at any library size"

export default function PicoTallyPart() {
  return <PicoTally position={3} total={48} />
}

export function OnlyGame() {
  return <PicoTally position={1} total={1} />
}

export function LargeLibrary() {
  return <PicoTally position={1234} total={2048} />
}
