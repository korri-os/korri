/**
 * Sections as cartridge spines stood in a column; the chosen one is pulled
 * out towards the card it opens.
 *
 * Tabs, not a menu — selecting one changes what the card beside it shows and
 * nothing else, and the ARIA says exactly that so a screen reader announces
 * "Plugins, tab, 2 of 3" rather than a bare button. Each spine is a plastic
 * from the shell colours, in turn, and the card takes the same one.
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
          data-shell={picoTabShell(index)}
          key={tab}
          onClick={() => onSelect(index)}
          role="tab"
          type="button"
        >
          {tab}
        </button>
      ))}
    </div>
  )
}

/** The plastic a section's spine and card share: blue, pink, orange, repeat. */
export function picoTabShell(index: number): 0 | 1 | 2 {
  return (index % 3) as 0 | 1 | 2
}
