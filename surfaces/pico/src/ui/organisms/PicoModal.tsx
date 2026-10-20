import "../../pico-motion.css"
import "./PicoModal.css"
import { useEffect, useId, useRef } from "react"
import { PicoRow } from "../atoms/PicoRow"
import { PicoCard } from "../molecules/PicoCard"

/**
 * A question over the screen, in Korri's words.
 *
 * Used for the confirmation a destructive action carries. The title, message
 * and confirm label all come from Korri, so Pico never paraphrases what
 * "forget" means; the cancel label is Pico's, because backing out is the
 * surface's affordance.
 *
 * The screen behind goes away entirely. PICO-8 has no translucency, and a
 * question is the only thing on screen while it is asked. The cursor starts
 * on Cancel, so a stray press backs out instead of destroying something.
 *
 * A message of several lines is a report to read before answering, such as
 * what a plugin asks for. Each line is then a stop for the cursor, and the
 * cursor starts on the first: the d-pad walks the report down to the answers,
 * and the card scrolls with it, because a focus-driven screen scrolls only by
 * moving focus. A line is a statement, so a stray press on it does nothing.
 *
 * `destructive` (the default) warns in red; a question that only asks, such
 * as turning a reviewed plugin on, is yellow and its confirm is not red.
 */
export function PicoModal({
  title,
  message,
  confirmLabel,
  destructive = true,
  onConfirm,
  onCancel,
}: {
  readonly title: string
  readonly message: string
  readonly confirmLabel: string
  readonly destructive?: boolean
  readonly onConfirm: () => void
  readonly onCancel: () => void
}) {
  const titleId = useId()
  const dialog = useRef<HTMLDivElement>(null)
  const lines = message.split("\n").map((line) => line.trim()).filter((line) => line !== "")
  const report = lines.length > 1
  useEffect(() => {
    const node = dialog.current
    const opener = document.activeElement
    if (!node || !(opener instanceof HTMLElement) || !node.closest(".pico-screen")?.contains(opener)) return
    const first = report ? node.querySelector<HTMLElement>(".pico-modal-line") : null
    ;(first ?? node.querySelector<HTMLButtonElement>("button:last-of-type"))?.focus()
    return () => {
      if (opener.isConnected && (document.activeElement === document.body || node.contains(document.activeElement))) {
        opener.focus()
      }
    }
  }, [])
  return (
    <div className="pico-modal-scrim">
      {/* data-block-exit keeps the host's directional focus inside the
          question, off the screen under the scrim. */}
      <div aria-labelledby={titleId} aria-modal className="pico-modal" data-block-exit ref={dialog} role="dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault()
            event.stopPropagation()
            onCancel()
          } else if (event.key === "Tab") {
            const buttons = dialog.current?.querySelectorAll<HTMLElement>(".pico-modal-line, button:not([disabled])")
            if (!buttons?.length) return
            const first = buttons[0]
            const last = buttons[buttons.length - 1]
            if (event.shiftKey && document.activeElement === first) {
              event.preventDefault()
              last?.focus()
            } else if (!event.shiftKey && document.activeElement === last) {
              event.preventDefault()
              first?.focus()
            }
          }
        }}>
        <PicoCard kicker={destructive ? "ARE YOU SURE?" : "READ THIS FIRST"} title={title} titleId={titleId} tone={destructive ? "warn" : "ask"}>
          {report ? (
            <ul className="pico-modal-report">
              {/* Lines may repeat; a line's place in the report is its identity. */}
              {lines.map((line, index) => (
                <li className="pico-modal-line" key={`${index}:${line}`} tabIndex={0}>{line}</li>
              ))}
            </ul>
          ) : (
            <p className="pico-modal-message">{message}</p>
          )}
          <div className="pico-modal-actions">
            <PicoRow danger={destructive} label={confirmLabel} onPress={onConfirm} />
            <PicoRow label="CANCEL" onPress={onCancel} />
          </div>
        </PicoCard>
      </div>
    </div>
  )
}
