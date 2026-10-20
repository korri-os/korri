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

/* A report to read before answering: one line per fact, a question that only
 * asks (yellow), not a warning. */
export function Report() {
  return (
    <PicoModal
      confirmLabel="Turn on"
      destructive={false}
      message={[
        "Warning: Plays sound over the network to paired speakers.",
        "Opens network ports: TCP 7000, 7001; UDP 5353.",
        "Runs in the background: chiptune-daemon.service.",
        "Also installs Audio core. Warning: Can change the system volume.",
        "Also uses PipeWire bridge. You approved it before.",
      ].join("\n")}
      onCancel={() => undefined}
      onConfirm={() => undefined}
      title="Turn on Chiptune?"
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
