import "./ZoneOptionRow.css"
import "../../zone-motion.css"
import type { SurfaceSettingInteraction } from "@contracts/surface/korri-surface"
import { type ReactNode, useEffect, useRef } from "react"
import { useZoneHorizontal } from "../../use-zone-horizontal"

/**
 * One OPTIONS row, as Mania's VIDEO page draws it: the label on a black
 * parallelogram, the value on a second one beside it, a hairline over both.
 *
 * What the value does comes from Korri's interaction, verbatim:
 *
 *  - `choice`: red triangles flank the value while the row is focused;
 *    left and right step it (`onStep`).
 *  - `text`: the value shows as given; A raises the keyboard (`onPress`).
 *  - `sensitiveText`: the value shows as given, because Korri's value for
 *    a secret is its status ("Configured", "Not set"), never the secret;
 *    the X keycap with Korri's `clearLabel` stands beside it while focused.
 *    Dots are only for what the person types, on the keyboard.
 *  - `text` and `sensitiveText` with no value: Korri's `placeholder`,
 *    dimmed, stands in the value's place.
 *  - `action`: the value is the command; a destructive one is red.
 *  - absent: the row is read-only and draws nothing it cannot do.
 *
 * Left and right reach a choice row from the portal as
 * `korri-semantic-direction` events (the row marks itself
 * `data-korri-horizontal-control` while it can step), never as keys.
 *
 * `valueNode` draws the value when a word is not enough (a ZoneSignal's
 * bars); `value` still names it for assistive tech.
 * `disabled` dims the row and keeps it focusable; Korri's `description`
 * stands under it whether or not it is focused, so the reason is on screen;
 * A does nothing.
 *
 * `saving` replaces the value with a blinking caret until Korri answers.
 * `problem` is Korri's message, red on its own parallelogram under the row.
 * `description` shows under an enabled row only while it is focused, so a
 * long list stays one row per setting.
 *
 * Focus is a prop. The row brings itself into view when it gains it, so a
 * scrolling group never leaves the cursor off screen.
 *
 * For now the parallelograms, the triangles and the keycap are drawn here;
 * they become ZonePanel, ZoneTriangle and ZoneKeycap after the merge.
 */
export function ZoneOptionRow({
  label,
  value,
  valueNode,
  interaction,
  description,
  focused = false,
  saving = false,
  disabled = false,
  problem,
  onPress,
  onStep,
  onFocus,
}: {
  readonly label: string
  readonly value?: string
  /** Drawn in place of `value`'s text, which still labels it. */
  readonly valueNode?: ReactNode
  readonly interaction?: SurfaceSettingInteraction
  readonly description?: string
  readonly focused?: boolean
  readonly saving?: boolean
  /** Dimmed and inert to A; the description shows why. */
  readonly disabled?: boolean
  readonly problem?: string
  /** A on the row: fire the action or raise the keyboard. */
  readonly onPress?: () => void
  /** Left or right on a choice row. */
  readonly onStep?: (direction: -1 | 1) => void
  readonly onFocus?: () => void
}) {
  const control = useRef<HTMLButtonElement | null>(null)
  useEffect(() => {
    if (focused) control.current?.scrollIntoView({ block: "nearest" })
  }, [focused])

  const kind = interaction?.kind ?? "readonly"
  const destructive = interaction?.kind === "action" && interaction.destructive === true
  const clearLabel = interaction?.kind === "sensitiveText" ? interaction.clearLabel : undefined
  const placeholder = (value === undefined || value === "")
    && (interaction?.kind === "text" || interaction?.kind === "sensitiveText") ? interaction.placeholder : undefined
  const steps = kind === "choice" && focused && !saving && !disabled
  const horizontal = useZoneHorizontal(control, steps && onStep !== undefined, kind, onStep)

  return (
    <div
      className="zone-option-row"
      data-destructive={destructive || undefined}
      data-disabled={disabled || undefined}
      data-focused={focused || undefined}
      data-interaction={kind}
      data-saving={saving || undefined}
    >
      <button
        aria-disabled={disabled || undefined}
        aria-label={valueNode !== undefined && value !== undefined ? `${label}: ${value}` : undefined}
        className="zone-option-row-control"
        data-korri-horizontal-control={horizontal["data-korri-horizontal-control"]}
        onClick={disabled ? undefined : onPress}
        onFocus={onFocus}
        ref={control}
        type="button"
      >
        <span className="zone-option-row-label">{label}</span>
        <span className="zone-option-row-value">
          {steps ? <span className="zone-option-row-step zone-option-row-step-back" /> : null}
          {saving
            ? <span aria-label="Saving" className="zone-option-row-caret" role="img" />
            : valueNode !== undefined
              ? <span className="zone-option-row-text">{valueNode}</span>
              : placeholder !== undefined
                ? <span className="zone-option-row-text" data-placeholder>{placeholder}</span>
                : <span className="zone-option-row-text">{value}</span>}
          {steps ? <span className="zone-option-row-step zone-option-row-step-forward" /> : null}
        </span>
      </button>
      {focused && clearLabel !== undefined && !saving ? (
        <span className="zone-option-row-clear">
          <span className="zone-option-row-keycap">X</span>
          {clearLabel}
        </span>
      ) : null}
      {(focused || disabled) && description !== undefined ? (
        <span className="zone-option-row-description">{description}</span>
      ) : null}
      {problem !== undefined ? (
        <span className="zone-option-row-problem" role="alert">{problem}</span>
      ) : null}
    </div>
  )
}
