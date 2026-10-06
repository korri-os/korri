import "../../pico-motion.css"
import "./PicoTextField.css"
import { useLayoutEffect, useRef } from "react"

/**
 * A value being typed on Pico's keyboard, with the blinking block caret.
 *
 * The end of the value is where typing happens, so the end is what stays on
 * screen: a value wider than the field is cut at its start, and a mark says
 * the start is there. A secret is drawn as one star per character. Empty, the
 * field shows Korri's placeholder in the quiet ink.
 *
 * It is not an `<input>`: the keys are Pico's own buttons, and a native field
 * would summon the device's keyboard or steal the cursor. It still says what
 * it is, a text box named for its setting.
 *
 * Whether the start is cut is a measurement of the drawn text, not a decision,
 * so it is written to the DOM (`data-cut`) after layout and CSS shows the mark.
 * No state: a measurement that re-rendered would draw twice per key.
 */
export function PicoTextField({
  label,
  text,
  masked = false,
  placeholder,
}: {
  /** The setting's name, for assistive technology. */
  readonly label: string
  readonly text: string
  readonly masked?: boolean
  readonly placeholder?: string
}) {
  const line = useRef<HTMLSpanElement>(null)
  const shown = masked ? "*".repeat([...text].length) : text
  useLayoutEffect(() => {
    const node = line.current
    const content = node?.firstElementChild
    const field = node?.parentElement
    if (!node || !field || !(content instanceof HTMLElement)) return
    // The line packs its content to the end, so a cut start overflows to the
    // left, where scrollWidth does not count it. Compare the content itself.
    if (content.offsetWidth > node.clientWidth) field.dataset.cut = "true"
    else delete field.dataset.cut
  })
  return (
    <div aria-label={label} className="pico-text-field" role="textbox">
      <span aria-hidden className="pico-text-field-cut">…</span>
      <span className="pico-text-field-line" ref={line}>
        {shown === "" ? (
          <span className="pico-text-field-placeholder">{placeholder ?? ""}</span>
        ) : (
          <span className="pico-text-field-text">{shown}</span>
        )}
      </span>
      <span aria-hidden className="pico-text-field-caret">█</span>
    </div>
  )
}
