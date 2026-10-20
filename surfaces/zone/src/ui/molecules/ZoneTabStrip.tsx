import "./ZoneTabStrip.css"
import { useRef } from "react"
import { useZoneFit } from "../../use-zone-fit"
import { ZoneTab } from "../atoms/ZoneTab"

export interface ZoneTabItem {
  readonly id: string
  readonly label: string
  /** What the tab holds, under its label: SETTINGS + WI-FI, IDENTITY. */
  readonly sub?: string
}

/**
 * The three tabs shoulder to shoulder, the current one as the banner. The
 * gap yields first when the strip is squeezed, then the tabs' type steps
 * down a whole cap, and at caps-1 the tabs take a second row. No tab is
 * ever dropped, shrunk or cut.
 */
export function ZoneTabStrip({
  tabs,
  active,
  focused,
  onSelect,
  onFocus,
}: {
  readonly tabs: readonly ZoneTabItem[]
  /** The current tab's id: it wears the banner. */
  readonly active: string
  /** The tab the cursor is on, when the cursor is on the tabs at all. */
  readonly focused?: string
  readonly onSelect: (id: string) => void
  readonly onFocus?: (id: string) => void
}) {
  const strip = useRef<HTMLDivElement>(null)
  useZoneFit(strip, [strip], tabs.map(tab => `${tab.label}|${tab.sub ?? ""}`).join("\n"))
  return (
    <div className="zone-tab-strip" ref={strip} role="tablist">
      {tabs.map(tab => (
        <ZoneTab
          active={tab.id === active}
          focused={tab.id === focused}
          key={tab.id}
          label={tab.label}
          onFocus={onFocus === undefined ? undefined : () => onFocus(tab.id)}
          onSelect={() => onSelect(tab.id)}
          sub={tab.sub}
        />
      ))}
    </div>
  )
}
