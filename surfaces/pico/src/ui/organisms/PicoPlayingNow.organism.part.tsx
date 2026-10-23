import { fixtureInvite, fixtureInvites, inviteViews, manyInvites } from "../../fixtures/named-states"
import { PicoPlayingNow } from "./PicoPlayingNow"

export const name = "Playing Now"
export const note = "Home's entry for games playing now on your devices; absent with none"

export default function PicoPlayingNowPart() {
  return <PicoPlayingNow invites={inviteViews([fixtureInvite])} onAction={() => undefined} />
}

export function OneInvite() {
  return <PicoPlayingNow invites={inviteViews([fixtureInvite])} onAction={() => undefined} />
}

export function ThreeInvites() {
  return <PicoPlayingNow invites={inviteViews(fixtureInvites)} onAction={() => undefined} />
}

/** More rows than fit its share of the screen: they scroll, a cut row shows. */
export function ManyInvites() {
  return <PicoPlayingNow invites={inviteViews(manyInvites)} onAction={() => undefined} />
}

// Intentionally null: home has no empty state for this entry.
export function NoInvites() {
  return <PicoPlayingNow invites={[]} onAction={() => undefined} />
}
