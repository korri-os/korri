import type { ReactNode } from "react"
import { PicoTitle } from "../atoms/PicoTitle"
import { PicoTabs } from "../molecules/PicoTabs"

/**
 * Categories down the side, one category's contents beside them: the layout
 * every settings-like screen in legacy shared.
 *
 * The seam between list and detail is a column split, not a stack, even on a
 * handheld — legacy found that a 4:3 screen has the width for it and lacks the
 * height for anything else.
 */
export function PicoPanelScreen({
  tabs,
  current,
  onSelect,
  title,
  children,
  footer,
}: {
  readonly tabs: readonly string[]
  readonly current: number
  readonly onSelect: (index: number) => void
  readonly title: string
  readonly children: ReactNode
  readonly footer?: string
}) {
  return (
    <div className="pico-panel-screen">
      <PicoTabs current={current} onSelect={onSelect} tabs={tabs} />
      <section aria-label={title} className="pico-panel-screen-detail">
        <div className="pico-panel-screen-title">
          <PicoTitle level={2} size="md" text={title} tone="accent" />
        </div>
        <div className="pico-panel-screen-body">{children}</div>
        {footer === undefined ? null : (
          <div className="pico-panel-screen-footer">{footer}</div>
        )}
      </section>
    </div>
  )
}
