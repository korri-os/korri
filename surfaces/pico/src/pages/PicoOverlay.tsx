import type { PicoOverlayControlView, PicoOverlayView, PicoRanging } from "../pico-overlay-view"
import { PicoModal } from "../ui/organisms/PicoModal"
import { PicoPauseMenu } from "../ui/organisms/PicoPauseMenu"
import { PicoGameOverlay } from "../ui/templates/PicoGameOverlay"

const HINTS = [
  { hintKey: "a", label: "SELECT" },
  { hintKey: "b", label: "RESUME" },
] as const

/* Named only when a range can be adjusted: left and right otherwise move the
 * cursor, which needs no hint. */
const ADJUST_HINT = { hintKey: "lr", label: "ADJUST" } as const

/**
 * The gameplay overlay: what Pico draws over a running game when Korri asks.
 *
 * Decides what a press means. A control that carries a value sends it; a bare
 * command sends its id; a destructive one asks first. A range is not pressed:
 * left and right adjust it, and its settled value is sent. The question is Pico's
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
  ranging,
  onRetry,
}: {
  readonly overlay: PicoOverlayView
  readonly asking?: PicoOverlayControlView
  readonly onAsk: (control: PicoOverlayControlView) => void
  readonly onConfirm: () => void
  readonly onCancel: () => void
  readonly onInvoke: (control: PicoOverlayControlView) => void
  readonly ranging: PicoRanging
  readonly onRetry: () => void
}) {
  const activate = (control: PicoOverlayControlView) => {
    if (control.destructive && control.sends === undefined) onAsk(control)
    else onInvoke(control)
  }
  const adjustable = [...overlay.controls, ...overlay.groups.flatMap((group) => group.controls)]
    .some((control) => control.range !== undefined && control.enabled)
  return (
    <PicoGameOverlay hints={adjustable ? [ADJUST_HINT, ...HINTS] : HINTS} label={overlay.title}>
      <PicoPauseMenu onActivate={activate} onRetry={onRetry} overlay={overlay} ranging={ranging} />
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
