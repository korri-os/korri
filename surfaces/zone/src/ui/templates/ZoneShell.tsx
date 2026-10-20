import "./ZoneShell.css"
import { useRef, type ReactNode } from "react"
import { ZONE_MENU_MS, zoneMenuFrame, zoneWipeClip } from "../../plan/motion"
import { takeZoneTransition, useZoneMoment } from "../../use-zone-moment"
import { useZoneTopStrip } from "../../use-zone-top-strip"
import type { ZoneGroundName } from "../organisms/ZoneGround"

/** The plan's two discrete answers. Height picks the strips; width picks the type. */
export type ZoneStrips = "tall" | "short"
export type ZoneBand = "wide" | "narrow"

/**
 * The frame every Zone page shares: a top strip with a keycap hint in each
 * corner and the tabs between them, a body, and a foot strip.
 *
 * The body is the only row that gives way. The strips take the height the
 * plan gives them (`strips`) first, so the way back and the tabs are always
 * on screen. The body is a size container, so a page inside it lays itself
 * out against the room it actually has.
 *
 * The top strip follows the shell's own width (`planTopStrip`, written as
 * `data-top`): on a narrow shell its gaps and type shrink to their least,
 * then the hints keep only their keycaps, then the strip takes two rows,
 * the hints over the tabs. No tab and no hint is ever hidden.
 *
 * The shell arranges slots and sizes their type; it supplies no words. A
 * page fills each slot with its own controls. `ground` names the palette
 * the shell stands on (`data-ground`); without it the shell takes the one
 * around it, which the root sets from the page shown.
 */
export function ZoneShell({
  ground,
  strips = "tall",
  band = "wide",
  hintBack,
  hintConfirm,
  tabs,
  foot,
  footEnd,
  page,
  children,
}: {
  /** The screen group's palette, for a shell drawn without the root around it. */
  readonly ground?: ZoneGroundName
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
  /** The top-left keycap hint: B BACK. */
  readonly hintBack?: ReactNode
  /** The top-right keycap hint: A CONFIRM. */
  readonly hintConfirm?: ReactNode
  readonly tabs?: ReactNode
  /** The foot's left run: the timer and the device tag. */
  readonly foot?: ReactNode
  /** The foot's right run: the Wi-Fi badge and the build string. */
  readonly footEnd?: ReactNode
  /** Names the page in the body: a new one, after a tab is chosen, plays the menu transition (M2). */
  readonly page?: string
  readonly children: ReactNode
}) {
  const hasFoot = foot !== undefined || footEnd !== undefined
  const shell = useRef<HTMLDivElement>(null)
  useZoneTopStrip(shell)
  // M2: a tab was just chosen. The ground it left wipes away from the top
  // right, the banner drops in, the body slides in from the right. A page
  // that arrives for any other reason arrives still.
  const entrance = useRef<{ readonly page: string | undefined; readonly count: number; readonly from: string; readonly first: boolean }>(
    { page, count: 0, from: "", first: true },
  )
  if (entrance.current.first || entrance.current.page !== page) {
    const from = takeZoneTransition(ZONE_MENU_MS)
    const was = entrance.current
    entrance.current = from === undefined
      ? { ...was, page, first: false }
      : { page, count: was.count + 1, from, first: false }
  }
  useZoneMoment(shell, {
    key: entrance.current.count === 0 ? undefined : String(entrance.current.count),
    duration: ZONE_MENU_MS,
    frame: (t, size) => {
      const frame = zoneMenuFrame(t, size.wArt, size.drawing.motion)
      return {
        "--zone-shell-wipe": zoneWipeClip(frame.wipe, size.wArt, size.hArt, size.drawing.edges),
        "--zone-shell-wipe-ground": entrance.current.from === "" ? "transparent" : entrance.current.from,
        "--zone-shell-banner-y": frame.bannerY,
        "--zone-shell-body-x": frame.rowsX[0] ?? 0,
      }
    },
  })
  return (
    <div className="zone-shell" data-band={band} data-ground={ground} data-strips={strips} ref={shell}>
      <i aria-hidden="true" className="zone-shell-wipe" />
      <header className="zone-shell-top">
        <div className="zone-shell-hint-back">{hintBack}</div>
        <nav className="zone-shell-tabs">{tabs}</nav>
        <div className="zone-shell-hint-confirm">{hintConfirm}</div>
      </header>
      <main className="zone-shell-body">{children}</main>
      {hasFoot ? (
        <footer className="zone-shell-foot">
          <div className="zone-shell-foot-start">{foot}</div>
          <div className="zone-shell-foot-end">{footEnd}</div>
        </footer>
      ) : null}
    </div>
  )
}
