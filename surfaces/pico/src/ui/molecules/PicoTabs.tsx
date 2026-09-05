/**
 * A vertical list of categories with one selected: legacy's settings sidebar.
 *
 * Tabs, not a menu — selecting one changes what the panel beside it shows and
 * nothing else, and the ARIA says exactly that so a screen reader announces
 * "Plugins, tab, 2 of 3" rather than a bare button.
 */
export function PicoTabs({
  tabs,
  current,
  onSelect,
}: {
  readonly tabs: readonly string[]
  readonly current: number
  readonly onSelect: (index: number) => void
}) {
  return (
    <div className="pico-tabs" role="tablist" aria-orientation="vertical">
      {tabs.map((tab, index) => (
        <button
          aria-selected={index === current}
          className="pico-tabs-tab"
          key={tab}
          onClick={() => onSelect(index)}
          role="tab"
          type="button"
        >
          <span aria-hidden className="pico-tabs-cursor">▶</span>
          <span className="pico-tabs-label">{tab}</span>
        </button>
      ))}
    </div>
  )
}
