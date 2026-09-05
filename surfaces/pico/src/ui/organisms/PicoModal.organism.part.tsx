import { PicoModal } from "./PicoModal"

export const name = "Modal"
export const note = "A question in Korri's words; only CANCEL is Pico's"

export default function PicoModalPart() {
  return (
    <PicoModal
      confirmLabel="FORGET"
      message="Every game, save and setting on this device is removed."
      onCancel={() => undefined}
      onConfirm={() => undefined}
      title="FORGET EVERYTHING?"
    />
  )
}
