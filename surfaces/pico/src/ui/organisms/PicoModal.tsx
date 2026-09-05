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
  return (
    <div className="pico-modal-scrim">
      <div aria-labelledby="pico-modal-title" aria-modal className="pico-modal" role="dialog">
        <div id="pico-modal-title">
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
