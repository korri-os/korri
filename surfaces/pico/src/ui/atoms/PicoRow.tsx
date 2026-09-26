import type { ReactNode } from "react"

/**
 * One line of a menu: a label on the left and, when there is one, its current
 * value on the right.
 *
 * Every list Pico shows — a pause menu, a setting group, the choices in a
 * question — is a stack of these, so they all move the cursor the same way.
 * Focus is the cursor: the focused row lifts onto a light plate with a hard
 * shadow. The plate colours come from the surrounding card through
 * `--pico-row-*`, so a row inside a red warning and a row on the bare ground
 * are the same component.
 *
 * With no `onPress` the row is a statement, not a control, and renders as
 * plain text that takes no focus.
 */
export function PicoRow({
  label,
  detail,
  danger = false,
  disabled = false,
  onPress,
}: {
  readonly label: string
  readonly detail?: ReactNode
  /** Marks a row whose action cannot be undone. */
  readonly danger?: boolean
  readonly disabled?: boolean
  readonly onPress?: () => void
}) {
  const body = (
    <>
      <span className="pico-row-label">{label}</span>
      {detail === undefined ? null : <span className="pico-row-detail">{detail}</span>}
    </>
  )
  if (onPress === undefined) {
    return (
      <div className="pico-row" data-danger={danger ? "true" : undefined}>
        {body}
      </div>
    )
  }
  return (
    <button
      className="pico-row"
      data-danger={danger ? "true" : undefined}
      disabled={disabled}
      onClick={onPress}
      type="button"
    >
      {body}
    </button>
  )
}
