import "./PicoChoicePicker.css"
import { useEffect, useId, useRef } from "react"
import { PicoRow } from "../atoms/PicoRow"
import { PicoCard } from "../molecules/PicoCard"

/**
 * Choose one of Korri's values for a setting that has none chosen yet, such
 * as which runner setting to add. Stepping through them in the row would
 * choose for the player, so the list opens here instead, every value a row,
 * in Korri's order and Korri's words, then CANCEL.
 *
 * A yellow card, because it is a question, over a solid screen. The cursor
 * starts on the first value and returns to the row that opened the list when
 * it closes. The card scrolls with the cursor when the list is longer than
 * the screen. It reads no keys: Back reaches it as the host's semantic
 * action, and the surface's update closes the question (PickingChoice).
 */
export function PicoChoicePicker({
  title,
  description,
  options,
  onChoose,
  onCancel,
}: {
  readonly title: string
  readonly description?: string
  readonly options: readonly { readonly value: string; readonly label: string }[]
  readonly onChoose: (value: string) => void
  readonly onCancel: () => void
}) {
  const titleId = useId()
  const dialog = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const node = dialog.current
    const opener = document.activeElement
    node?.querySelector<HTMLButtonElement>("button")?.focus()
    return () => {
      if (opener instanceof HTMLElement && opener.isConnected && (document.activeElement === document.body || node?.contains(document.activeElement))) {
        opener.focus()
      }
    }
  }, [])
  return (
    <div className="pico-choice-picker">
      {/* data-block-exit keeps the host's directional focus inside the list. */}
      <div aria-labelledby={titleId} aria-modal className="pico-choice-picker-dialog" data-block-exit ref={dialog} role="dialog">
        <PicoCard kicker="CHOOSE ONE" title={title} titleId={titleId} tone="ask">
          {description === undefined ? null : <p className="pico-choice-picker-description">{description}</p>}
          <ul className="pico-choice-picker-list">
            {options.map((option) => (
              <li key={option.value}>
                <PicoRow label={option.label} onPress={() => onChoose(option.value)} />
              </li>
            ))}
          </ul>
          <PicoRow label="CANCEL" onPress={onCancel} />
        </PicoCard>
      </div>
    </div>
  )
}
