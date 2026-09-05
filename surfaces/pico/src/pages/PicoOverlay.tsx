import type { PicoOverlayControlView, PicoOverlayView } from "../pico-overlay-view"
import { PicoModal } from "../ui/organisms/PicoModal"
import { PicoPauseMenu } from "../ui/organisms/PicoPauseMenu"
import { PicoGameOverlay } from "../ui/templates/PicoGameOverlay"

const HINTS = [
  { hintKey: "a", label: "SELECT" },
  { hintKey: "b", label: "RESUME" },
] as const

/**
 * The gameplay overlay: what Pico draws over a running game when Korri asks.
 *
 * Decides what a press means. A control that carries a value sends it; a bare
 * command sends its id; a destructive one asks first. The question is Pico's
 * because the treaty's gameplay controls carry no confirmation copy of their
 * own — so it is built from Korri's label, and says only that.
 */
export function PicoOverlay({
  overlay,
  asking,
  onAsk,
  onConfirm,
  onCancel,
  onInvoke,
  onRetry,
}: {
  readonly overlay: PicoOverlayView
  readonly asking?: PicoOverlayControlView
  readonly onAsk: (control: PicoOverlayControlView) => void
  readonly onConfirm: () => void
  readonly onCancel: () => void
  readonly onInvoke: (control: PicoOverlayControlView) => void
  readonly onRetry: () => void
}) {
  const activate = (control: PicoOverlayControlView) => {
    if (control.destructive && control.sends === undefined) onAsk(control)
    else onInvoke(control)
  }
  return (
    <PicoGameOverlay hints={HINTS} label={overlay.title}>
      <PicoPauseMenu onActivate={activate} onRetry={onRetry} overlay={overlay} />
      {asking === undefined ? null : (
        <PicoModal
          confirmLabel={asking.label.toUpperCase()}
          message={asking.description ?? "This cannot be undone."}
          onCancel={onCancel}
          onConfirm={onConfirm}
          title={`${asking.label.toUpperCase()}?`}
        />
      )}
    </PicoGameOverlay>
  )
}
