import { PicoTally } from "./PicoTally"

export const name = "Tally"
export const note = "Position in the shelf; readable at any library size"

export default function PicoTallyPart() {
  return <PicoTally position={3} total={48} />
}
