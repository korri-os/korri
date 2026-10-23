import { dimmedInvite, fixtureInvite, inviteViews, longInvite } from "../../fixtures/named-states"
import { PicoInviteRow } from "./PicoInviteRow"

export const name = "Invite Row"
export const note = "One game playing now on another device, with Korri's actions for it"

const [turtles] = inviteViews([fixtureInvite])
const [zelda] = inviteViews([longInvite])
const [dimmed] = inviteViews([dimmedInvite])

export default function PicoInviteRowPart() {
  return (
    <PicoInviteRow
      actions={fixtureInvite.actions}
      artUrl={turtles?.artUrl}
      deviceLabel={fixtureInvite.deviceLabel}
      gameTitle={fixtureInvite.gameTitle}
      id={fixtureInvite.id}
      onAction={() => undefined}
      personName={fixtureInvite.personName}
    />
  )
}

/** No cover art, and the longest names: initials stand in, words wrap. */
export function LongNamesNoArtwork() {
  return (
    <PicoInviteRow
      actions={zelda?.actions ?? []}
      deviceLabel={longInvite.deviceLabel}
      gameTitle={longInvite.gameTitle}
      id={longInvite.id}
      onAction={() => undefined}
      personName={longInvite.personName}
    />
  )
}

/** Korri dims the route: the button stays, flat, and does nothing. */
export function RouteDimmed() {
  return (
    <PicoInviteRow
      actions={dimmed?.actions ?? []}
      artUrl={dimmed?.artUrl}
      deviceLabel={dimmedInvite.deviceLabel}
      gameTitle={dimmedInvite.gameTitle}
      id={dimmedInvite.id}
      onAction={() => undefined}
      personName={dimmedInvite.personName}
    />
  )
}
