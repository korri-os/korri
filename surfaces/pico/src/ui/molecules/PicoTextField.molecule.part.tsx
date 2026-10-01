import { PicoTextField } from "./PicoTextField"

export const name = "Text Field"
export const note = "The end of the value stays in view; a secret is one star per character"

export default function PicoTextFieldPart() {
  return <PicoTextField label="Name" text="RG353M Den 0" />
}

export function Empty() {
  return <PicoTextField label="Name" placeholder="This device" text="" />
}

export function LongValue() {
  return <PicoTextField label="Name" text="The handheld that lives in the living room drawer" />
}

export function Secret() {
  return <PicoTextField label="SteamGridDB API key" masked text="a1b2c3d4e5" />
}
