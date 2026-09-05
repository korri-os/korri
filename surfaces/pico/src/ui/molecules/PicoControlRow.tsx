import type { PicoOverlayControlView } from "../../pico-overlay-view"

/**
 * One gameplay control as a menu row: Korri's label left, its state right.
 *
 * A disabled control is drawn dimmed with Korri's reason beneath it rather than
 * hidden. Hiding it would make the menu change shape between opens, and the
 * reason is the useful part — "No save yet" tells the user what to do.
 */
export function PicoControlRow({
  control,
  onActivate,
}: {
  readonly control: PicoOverlayControlView
  readonly onActivate: () => void
}) {
  return (
    <li className="pico-control-row-item">
      <button
        className="pico-control-row"
        data-destructive={control.destructive ? "true" : undefined}
        disabled={!control.enabled}
        onClick={onActivate}
        type="button"
      >
        <span className="pico-control-row-label">{control.label}</span>
        {control.stateLabel === undefined ? null : (
          <span className="pico-control-row-state">‹ {control.stateLabel} ›</span>
        )}
      </button>
      {!control.enabled && control.disabledReason !== undefined ? (
        <p className="pico-control-row-reason">{control.disabledReason}</p>
      ) : control.description === undefined ? null : (
        <p className="pico-control-row-reason">{control.description}</p>
      )}
    </li>
  )
}
