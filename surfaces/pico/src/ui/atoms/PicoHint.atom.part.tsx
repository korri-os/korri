import { PicoHint } from "./PicoHint"

export const name = "Hint"
export const note = "States what a button already does; never interactive"

export default function PicoHintPart() {
  return <PicoHint hintKey="a" label="PLAY" />
}
