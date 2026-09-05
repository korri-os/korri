import { PicoQueryField } from "./PicoQueryField"

export const name = "Query Field"
export const note = "The caret is the only moving thing, so it reads as the target"

export default function PicoQueryFieldPart() {
  return <PicoQueryField query="SPEL" />
}
