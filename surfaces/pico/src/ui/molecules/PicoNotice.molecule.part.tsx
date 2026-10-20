import { PicoNotice } from "./PicoNotice"

export const name = "Notice"
export const note = "Loading, empty, and failed are one view with a tone"

export default function PicoNoticePart() {
  return (
    <PicoNotice
      kicker="NOTHING HERE YET"
      message="Add games to your library and they will show up on the shelf."
      tone="info"
    />
  )
}

export function RetryableFailure() {
  return (
    <PicoNotice
      actions={[
        { label: "Try again", onPress: () => undefined },
        { label: "Back", onPress: () => undefined },
      ]}
      kicker="COULD NOT START"
      message="The device did not respond."
      title="Hollow Knight"
      tone="warn"
    />
  )
}

export function Loading() {
  return <PicoNotice kicker="LOADING LIBRARY" message="Waiting for your games." tone="info" />
}

/** A question: yellow, with Korri's two answers as rows. */
export function Ask() {
  return (
    <PicoNotice
      actions={[
        { label: "Download", onPress: () => undefined },
        { label: "Cancel", onPress: () => undefined },
      ]}
      kicker="Needs a download"
      message="FEX rootfs and Proton-CachyOS, 1.5 GB"
      title="Hollow Knight"
      tone="ask"
    />
  )
}

/** A kicker and its answers, with nothing more to say under it. */
export function AskWithoutMessage() {
  return (
    <PicoNotice
      actions={[{ label: "Download", onPress: () => undefined }, { label: "Cancel", disabled: true, onPress: () => undefined }]}
      kicker="Needs a download"
      tone="ask"
    />
  )
}
