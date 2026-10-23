import "./PicoInviteNotices.css"
import type { PicoInviteNoticeView } from "../../pico-invite-view"
import { PicoInviteNotice } from "../molecules/PicoInviteNotice"

/**
 * The invites announced now, newest first, stacked in the top left corner
 * over whatever the screen shows.
 *
 * Nothing here can be pressed or focused, and a pointer goes through it to
 * the screen below, so a notice never blocks the person's way. The stack
 * may cover a share of the screen and no more: a notice that does not fit
 * whole is not drawn, oldest first. Each invite is still on home, under
 * PLAYING NOW, for as long as it stands.
 */
export function PicoInviteNotices({ notices }: { readonly notices: readonly PicoInviteNoticeView[] }) {
  return (
    <div aria-live="polite" className="pico-invite-notices" role="status">
      {notices.map(notice => (
        <PicoInviteNotice
          deviceLabel={notice.deviceLabel}
          gameTitle={notice.gameTitle}
          key={notice.id}
          personName={notice.personName}
        />
      ))}
    </div>
  )
}
