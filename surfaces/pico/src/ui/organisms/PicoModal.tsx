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
    if (!node || !(opener instanceof HTMLElement) || !node.closest(".pico-screen")?.contains(opener)) return
    node.querySelector<HTMLButtonElement>("button:last-of-type")?.focus()
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
        <PicoCard kicker="ARE YOU SURE?" title={title} titleId={titleId} tone="warn">
          <p className="pico-modal-message">{message}</p>
          <div className="pico-modal-actions">
            <PicoRow danger label={confirmLabel} onPress={onConfirm} />
            <PicoRow label="CANCEL" onPress={onCancel} />
          </div>
        </PicoCard>
      </div>
    </div>
  )
}
