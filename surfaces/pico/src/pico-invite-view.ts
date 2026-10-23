import type { SurfaceAction, SurfaceModel } from "@contracts/surface/korri-surface"

/**
 * One invite as Pico draws it: another device of this person holds a live
 * session this device can play on. Every word is Korri's; the actions are
 * Korri's too (stream play today, and a dismiss), run through `runAction`
 * unchanged. Pico never infers a route from a label.
 */
export interface PicoInviteView {
  readonly id: string
  readonly personName: string
  readonly gameTitle: string
  readonly deviceLabel: string
  readonly artUrl?: string
  readonly actions: readonly SurfaceAction[]
}

/** What a notice says. It offers nothing to press: the home entry does. */
export interface PicoInviteNoticeView {
  readonly id: string
  readonly personName: string
  readonly gameTitle: string
  readonly deviceLabel: string
}

/** How long one notice stays on screen. The invite stays on home after it. */
export const PICO_INVITE_NOTICE_MS = 8000

/** "Simon is playing": the words before the game, in one place. */
export const picoInvitePlaying = (personName: string): string => `${personName} is playing`

/** "on Odin 2 Portal": where the game runs. */
export const picoInviteWhere = (deviceLabel: string): string => `on ${deviceLabel}`

/** The invites Korri publishes now, in its order. None in a host without
 * shared play. */
export function picoInvitesFrom(model: Pick<SurfaceModel, "invites">): readonly PicoInviteView[] {
  return (model.invites ?? []).map(invite => ({
    id: invite.id,
    personName: invite.personName,
    gameTitle: invite.gameTitle,
    deviceLabel: invite.deviceLabel,
    ...(invite.coverArtUrl === undefined ? {} : { artUrl: invite.coverArtUrl }),
    actions: invite.actions,
  }))
}

/** Whether Korri publishes this action, enabled, on an invite now. */
export function picoInviteActionPublished(model: Pick<SurfaceModel, "invites">, actionId: string): boolean {
  return (model.invites ?? []).some(invite => invite.actions.some(action => action.id === actionId && action.enabled))
}
