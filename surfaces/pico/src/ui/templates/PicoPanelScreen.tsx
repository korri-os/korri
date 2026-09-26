import type { ReactNode } from "react"
import { PicoCard } from "../molecules/PicoCard"
import { PicoTabs, picoTabShell } from "../molecules/PicoTabs"

/**
 * Sections as spines down the side, the chosen section's contents in a card
 * of the same plastic beside them, so a section and what it holds read as one
 * object.
 *
 * The split is a column, not a stack, while there is width for it; a narrow
 * pane stands the spines in a row above the card instead.
 */
export function PicoPanelScreen({
  tabs,
  current,
  onSelect,
  title,
  children,
}: {
  readonly tabs: readonly string[]
  readonly current: number
  readonly onSelect: (index: number) => void
  readonly title: string
  readonly children: ReactNode
}) {
  return (
    <div className="pico-panel-screen">
      <PicoTabs current={current} onSelect={onSelect} tabs={tabs} />
      <section aria-label={title} className="pico-panel-screen-detail">
        <PicoCard shell={picoTabShell(current)} tone="tell">
          {children}
        </PicoCard>
      </section>
    </div>
  )
}
