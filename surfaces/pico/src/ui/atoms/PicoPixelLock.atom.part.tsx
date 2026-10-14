import { PicoPixelLock } from "./PicoPixelLock"

export const name = "Pixel Lock"
export const note = "Whether a network asks for a password, drawn on the 8×8 grid"

export default function PicoPixelLockPart() {
  return <PicoPixelLock locked />
}

export function Open() {
  return <PicoPixelLock locked={false} />
}
