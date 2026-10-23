import "./PicoInviteRow.css"
import type { SurfaceAction } from "@contracts/surface/korri-surface"
import { picoInvitePlaying, picoInviteWhere } from "../../pico-invite-view"
import { PicoButton } from "../atoms/PicoButton"
import { PicoCoverArt } from "../atoms/PicoCoverArt"

/**
 * One game playing now on another of the person's devices: its cover, its
 * title, who plays it and where, and Korri's actions for it.
 *
 * The actions are exactly the ones Korri published, in its order and with
 * its labels: play on stream and dismiss today. Every one is a quiet
 * button, because Pico does not know which one is the main one; a
 * destructive one is red. A dimmed one stays, as the treaty asks, and
 * Korri's reason for it, when it gives one, goes under the words.
 *
 * The words take the width first; the buttons follow them on the same line
 * while they fit, and wrap under them when they do not, so no button is
 * ever cut.
 */
export function PicoInviteRow({
  id,
  personName,
  gameTitle,
  deviceLabel,
  artUrl,
  actions,
  onAction,
}: {
  readonly id: string
  readonly personName: string
  readonly gameTitle: string
  readonly deviceLabel: string
  readonly artUrl?: string
  readonly actions: readonly SurfaceAction[]
  readonly onAction: (actionId: string) => void
}) {
  const why = actions.flatMap(action => !action.enabled && action.description !== undefined ? [action.description] : [])
  return (
    <article aria-label={`${picoInvitePlaying(personName)} ${gameTitle}`} className="pico-invite-row">
      <span className="pico-invite-row-art">
        <PicoCoverArt artUrl={artUrl} id={id} title={gameTitle} />
      </span>
      <span className="pico-invite-row-words">
        <span className="pico-invite-row-game">{gameTitle}</span>
        <span className="pico-invite-row-who">
          {picoInvitePlaying(personName)} {picoInviteWhere(deviceLabel)}
        </span>
        {why.map(reason => <span className="pico-invite-row-why" key={reason}>{reason}</span>)}
      </span>
      {actions.length === 0 ? null : (
        <span className="pico-invite-row-actions">
          {actions.map(action => (
            <PicoButton
              disabled={!action.enabled}
              key={action.id}
              label={action.label}
              onPress={() => onAction(action.id)}
              tone={action.destructive === true ? "danger" : "quiet"}
            />
          ))}
        </span>
      )}
    </article>
  )
}
