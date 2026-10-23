import "./PicoInviteNotice.css"
import { picoInvitePlaying, picoInviteWhere } from "../../pico-invite-view"
import { PicoCard } from "./PicoCard"

/**
 * One invite, announced: "Simon is playing Turtles in Time", on a small blue
 * card, because it tells. It drops in the way every card lands.
 *
 * It offers nothing to press and takes no focus. The person may be in the
 * middle of choosing a game, and a notice that took the cursor would choose
 * for them. The invite's actions are on home, under PLAYING NOW.
 */
export function PicoInviteNotice({
  personName,
  gameTitle,
  deviceLabel,
}: {
  readonly personName: string
  readonly gameTitle: string
  readonly deviceLabel: string
}) {
  return (
    <div className="pico-invite-notice">
      <PicoCard kicker={picoInvitePlaying(personName)} title={gameTitle} tone="tell">
        <p className="pico-invite-notice-where">{picoInviteWhere(deviceLabel)}</p>
      </PicoCard>
    </div>
  )
}
