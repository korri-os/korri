import { PicoQueryField } from "./PicoQueryField"

export const name = "Query Field"
export const note = "The caret is the only moving thing, so it reads as the target"

export default function PicoQueryFieldPart() {
  return <PicoQueryField query="SPEL" />
}

export function Empty() {
  return <PicoQueryField query="" />
}

export function LongQuery() {
  return <PicoQueryField query="THE LEGEND OF ZELDA A LINK TO THE PAST" />
}
