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
