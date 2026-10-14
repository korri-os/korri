import { builtInController } from "../../fixtures/onboarding"
import { PicoControllerRow } from "./PicoControllerRow"

export const name = "Controller Row"
export const note = "A controller the host sees, and a lamp that lights when Korri hears it"

export default function PicoControllerRowPart() {
  return <PicoControllerRow controller={builtInController} />
}

export function LongName() {
  return <PicoControllerRow controller={{ ...builtInController, name: "Xbox Wireless Controller (Bluetooth, second pairing)" }} />
}

export function Heard() {
  return <PicoControllerRow controller={{ ...builtInController, heard: true }} />
}
