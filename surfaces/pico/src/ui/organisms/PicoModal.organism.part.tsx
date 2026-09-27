import { PicoModal } from "./PicoModal"

export function LongMessage() {
  return (
    <PicoModal
      confirmLabel="FORGET"
      message="Every game, save and setting on this device is removed. Make sure you have stored your identity backup outside this device. Korri cannot recover a lost key. Without a backup, the identity is unrecoverable after device loss."
      onCancel={() => undefined}
      onConfirm={() => undefined}
      title="FORGET EVERYTHING ON THIS DEVICE?"
    />
  )
}

// Focus trapping and cancel/confirm are interactions, not additional props.
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
