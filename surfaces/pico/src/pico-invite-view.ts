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

/** A corner of the screen the notice stack can stand in. */
export type PicoNoticeCorner = "top-left" | "top-right" | "bottom-left" | "bottom-right"

const CORNERS: readonly PicoNoticeCorner[] = ["top-left", "top-right", "bottom-left", "bottom-right"]

/** A box on screen, in CSS pixels. */
export interface PicoBox {
  readonly left: number
  readonly top: number
  readonly right: number
  readonly bottom: number
}

/**
 * Where the notice stack stands, so the focused control stays in sight: the
 * first corner, in reading order from the top left, whose stack would not
 * touch the focused control (with `inset` of room round it for its hard
 * shadow); where every corner would, the one that covers least of it. With
 * nothing focused, the top left: there the stack covers words, the header
 * and titles, before it covers controls.
 */
export function picoNoticeCorner(
  frame: PicoBox,
  size: { readonly width: number; readonly height: number },
  inset: number,
  focused: PicoBox | undefined,
): PicoNoticeCorner {
  if (focused === undefined) return "top-left"
  const target = {
    left: focused.left - inset,
    top: focused.top - inset,
    right: focused.right + inset,
    bottom: focused.bottom + inset,
  }
  const covered = (corner: PicoNoticeCorner): number => {
    const left = corner.endsWith("left") ? frame.left + inset : frame.right - inset - size.width
    const top = corner.startsWith("top") ? frame.top + inset : frame.bottom - inset - size.height
    const width = Math.min(left + size.width, target.right) - Math.max(left, target.left)
    const height = Math.min(top + size.height, target.bottom) - Math.max(top, target.top)
    return width > 0 && height > 0 ? width * height : 0
  }
  let best: PicoNoticeCorner = "top-left"
  let least = Number.POSITIVE_INFINITY
  for (const corner of CORNERS) {
    const area = covered(corner)
    if (area === 0) return corner
    if (area < least) {
      least = area
      best = corner
    }
  }
  return best
}
