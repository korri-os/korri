import "./ZoneShell.css"
import { useRef, type ReactNode } from "react"
import { useZoneTopStrip } from "../../use-zone-top-strip"

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
 * page fills each slot with its own controls.
 */
export function ZoneShell({
  strips = "tall",
  band = "wide",
  hintBack,
  hintConfirm,
  tabs,
  foot,
  footEnd,
  children,
}: {
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
  readonly children: ReactNode
}) {
  const hasFoot = foot !== undefined || footEnd !== undefined
  const shell = useRef<HTMLDivElement>(null)
  useZoneTopStrip(shell)
  return (
    <div className="zone-shell" data-band={band} data-strips={strips} ref={shell}>
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
