import { useEffect, useId, useRef } from "react"
import { PicoButton } from "../atoms/PicoButton"
import { PicoTitle } from "../atoms/PicoTitle"

/**
 * A question over the screen, in Korri's words.
 *
 * Used for the confirmation a destructive setting action carries. The title,
 * message and confirm label all come from Korri, so Pico never paraphrases what
 * "forget" means; the cancel label is Pico's, because backing out is the
 * surface's affordance.
 */
export function PicoModal({
  title,
  message,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  readonly title: string
  readonly message: string
  readonly confirmLabel: string
  readonly onConfirm: () => void
  readonly onCancel: () => void
}) {
  const titleId = useId()
  const dialog = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const node = dialog.current
    const opener = document.activeElement
    // Several independent surfaces may share a document. Only the active one
    // may take focus; an idle preview must not focus another surface's dialog.
    if (!node || !(opener instanceof HTMLElement) || !node.closest(".pico-screen")?.contains(opener)) return
    node.querySelector<HTMLButtonElement>("button:last-child")?.focus()
    return () => {
      if (opener.isConnected && (document.activeElement === document.body || node.contains(document.activeElement))) {
        opener.focus()
      }
    }
  }, [])
  return (
    <div className="pico-modal-scrim">
      <div aria-labelledby={titleId} aria-modal className="pico-modal" ref={dialog} role="dialog"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault()
            event.stopPropagation()
            onCancel()
          } else if (event.key === "Tab") {
            const buttons = dialog.current?.querySelectorAll<HTMLButtonElement>("button:not([disabled])")
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
        <div id={titleId}>
          <PicoTitle level={2} size="md" text={title} tone="warn" />
        </div>
        <p className="pico-modal-message">{message}</p>
        <div className="pico-modal-actions">
          <PicoButton label={confirmLabel} onPress={onConfirm} />
          <PicoButton label="CANCEL" onPress={onCancel} />
        </div>
      </div>
    </div>
  )
}
