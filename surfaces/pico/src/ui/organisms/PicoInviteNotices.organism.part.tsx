import { fixtureInvite, fixtureInvites, inviteNoticeViews, manyInvites } from "../../fixtures/named-states"
import { PicoInviteNotices } from "./PicoInviteNotices"

export const name = "Invite Notices"
export const note = "Invites announced now, newest first, in the top corner; they take no focus"

export default function PicoInviteNoticesPart() {
  return <PicoInviteNotices notices={inviteNoticeViews([fixtureInvite])} />
}

export function OneInvite() {
  return <PicoInviteNotices notices={inviteNoticeViews([fixtureInvite])} />
}

/** Three arrive together: each gets its own notice. */
export function SeveralInvites() {
  return <PicoInviteNotices notices={inviteNoticeViews(fixtureInvites)} />
}

/** More than half the screen holds: the oldest give way whole. */
export function ManyInvites() {
  return <PicoInviteNotices notices={inviteNoticeViews(manyInvites)} />
}

/** Nothing announced: the live region stays, empty, for the next one. */
export function NoInvites() {
  return <PicoInviteNotices notices={[]} />
}
