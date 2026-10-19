import "./ZoneTab.css"
import "../../zone-motion.css"
import { ZoneBanner } from "./ZoneBanner"
import { ZoneCaps } from "./ZoneCaps"
import { ZonePanel } from "./ZonePanel"
import { ZoneUnderline } from "./ZoneUnderline"

/**
 * One tab in the top strip. The current tab (`active`) wears the hexagonal
 * banner, as Mania titles the screen you are on; the others are black
 * slanted panels with the label and, under it, a sub-line naming what the
 * tab holds. The focused tab lifts and takes the pink underline; tabs sit
 * shoulder to shoulder, so there are no triangles.
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
  return (
    <button
      className="zone-tab"
      aria-current={active ? "page" : undefined}
      data-active={active ? "on" : "off"}
      data-focused={focused ? "on" : "off"}
      onClick={onSelect}
      onFocus={onFocus}
      type="button"
    >
      {active ? (
        <span className="zone-tab-banner">
          <ZoneBanner>{label}</ZoneBanner>
          {focused ? <ZoneUnderline /> : null}
        </span>
      ) : (
        <ZonePanel edge={false}>
          <span className="zone-tab-words">
            <ZoneCaps>{label}</ZoneCaps>
            {sub === undefined ? null : <span className="zone-tab-sub">{sub}</span>}
          </span>
          {focused ? <ZoneUnderline /> : null}
        </ZonePanel>
      )}
    </button>
  )
}
