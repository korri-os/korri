import "../../pico-motion.css"
import "./PicoRow.css"
import { useId, type ReactNode } from "react"

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
 * With no `onPress` the row is a statement, not a control. A caller can make
 * the statement focusable so directional navigation can reveal its value.
 */
export function PicoRow({
  label,
  detail,
  danger = false,
  disabled = false,
  focusable = false,
  onPress,
}: {
  readonly label: string
  readonly detail?: ReactNode
  /** Marks a row whose action cannot be undone. */
  readonly danger?: boolean
  readonly disabled?: boolean
  /** Let native directional focus reach a read-only fact without an action. */
  readonly focusable?: boolean
  readonly onPress?: () => void
}) {
  const labelId = useId()
  const detailId = `${labelId}-detail`
  const body = (
    <>
      <span className="pico-row-label" id={labelId}>{label}</span>
      {detail === undefined ? null : <span className="pico-row-detail" id={detailId}>{detail}</span>}
    </>
  )
  if (onPress === undefined) {
    return (
      <div
        className="pico-row"
        data-danger={danger ? "true" : undefined}
        tabIndex={focusable ? 0 : undefined}
        role={focusable ? "group" : undefined}
        aria-labelledby={focusable ? `${labelId}${detail === undefined ? "" : ` ${detailId}`}` : undefined}
      >
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
