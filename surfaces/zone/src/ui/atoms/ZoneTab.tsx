import "./ZoneTab.css"
import { useRef } from "react"
import { markZoneTransition } from "../../use-zone-moment"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneBanner } from "./ZoneBanner"
import { ZoneCaps } from "./ZoneCaps"
import { ZoneUnderline } from "./ZoneUnderline"

/**
 * One tab in the top strip. The current tab (`active`) wears the hexagonal
 * banner, as Mania titles the screen you are on; the others are black
 * panels leaning at 45 degrees with stair edges, the label in white caps
 * and a sub-line, read and not drawn, naming what the tab holds. The
 * focused tab lifts and takes the pink underline; tabs sit shoulder to
 * shoulder, so there are no triangles.
 *
 * The panel's height is its caps and its padding (`--zone-tab-pad-y`, art
 * px), and its slant is that height, so a strip that needs room takes the
 * padding first: a shorter panel leans less far and the label keeps its
 * caps.
 *
 * A native button: the host moves DOM focus, and `onFocus` lets the page
 * follow it. `focused` is drawn from the prop, never from DOM state.
 */
export function ZoneTab({
  label,
  sub,
  active = false,
  focused = false,
  onSelect,
  onFocus,
}: {
  readonly label: string
  readonly sub?: string
  readonly active?: boolean
  readonly focused?: boolean
  readonly onSelect: () => void
  readonly onFocus?: () => void
}) {
  const banner = useRef<HTMLSpanElement>(null)
  const panel = useRef<HTMLSpanElement>(null)
  useZoneStairClip(banner, { left: "point", right: "point" })
  useZoneStairClip(panel)
  return (
    <button
      className="zone-tab"
      aria-current={active ? "page" : undefined}
      data-active={active ? "on" : "off"}
      data-focused={focused ? "on" : "off"}
      onClick={event => {
        // The page about to arrive plays the menu transition from this ground (M2).
        if (!active) markZoneTransition(event.currentTarget)
        onSelect()
      }}
      onFocus={onFocus}
      type="button"
    >
      {active ? (
        <span className="zone-tab-banner" ref={banner}>
          <ZoneBanner>{label}</ZoneBanner>
          {focused ? <ZoneUnderline /> : null}
        </span>
      ) : (
        <span className="zone-tab-panel" ref={panel}>
          <span className="zone-tab-words">
            <ZoneCaps>{label}</ZoneCaps>
            {sub === undefined ? null : <span className="zone-tab-sub">{sub}</span>}
          </span>
          {focused ? <ZoneUnderline /> : null}
        </span>
      )}
    </button>
  )
}
