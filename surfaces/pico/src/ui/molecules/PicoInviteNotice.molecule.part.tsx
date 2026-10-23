import { fixtureInvite, longInvite } from "../../fixtures/named-states"
import { PicoInviteNotice } from "./PicoInviteNotice"

export const name = "Invite Notice"
export const note = "One invite, announced: who is playing what, and where. Nothing to press"

export default function PicoInviteNoticePart() {
  return (
    <PicoInviteNotice
      deviceLabel={fixtureInvite.deviceLabel}
      gameTitle={fixtureInvite.gameTitle}
      personName={fixtureInvite.personName}
    />
  )
}

/** A 64-character name with no break, a long title and a long device label. */
export function LongNames() {
  return (
    <PicoInviteNotice
      deviceLabel={longInvite.deviceLabel}
      gameTitle={longInvite.gameTitle}
      personName={longInvite.personName}
    />
  )
}
