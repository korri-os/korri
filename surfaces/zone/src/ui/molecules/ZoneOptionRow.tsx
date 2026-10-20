import "./ZoneOptionRow.css"
import "../../zone-motion.css"
import type { SurfaceSettingInteraction } from "@contracts/surface/korri-surface"
import { type ReactNode, useEffect, useRef } from "react"
import { zoneCutMode, zoneWholeLine } from "../../plan/options-row"
import { useZoneHorizontal } from "../../use-zone-horizontal"
import { useZoneOptionRow } from "../../use-zone-option-row"

/**
 * One OPTIONS row, as Mania's VIDEO and SECRETS pages draw it: the label on
 * a black panel, the value centred on a second black panel beside it, both
 * leaning at 45 degrees with stair edges. The focused row wears the pink
 * drop under its label; a choice also gets the red triangles inside its
 * value panel, pointing the ways left and right step it.
 *
 * Lettering: the label is Zone Caps (caps-1, white over lavender lower
 * rows) and wraps between words; a value, and a label Korri publishes as a
 * fact (`verbatimLabel`: a path, an SSID, a program), are Jersey 10 in
 * their own case (`verbatimValue`) on one line. A fact that does not fit is
 * cut, a path in the middle and anything else at the end, and the focused
 * row shows it whole on a line of its own under the panels. Zone's own
 * value words (GO, JOIN, OK) stay in caps. The row lays itself out against
 * its own width (useZoneOptionRow): at or under 224 art px it stacks the
 * label over the value.
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
 *  - `text` and `sensitiveText` with no value: Korri's `placeholder`, in
 *    the steel face, stands in the value's place.
 *  - `action`: the value is the command; a destructive one is in the
 *    danger ink.
 *  - absent: the row is read-only and draws nothing it cannot do.
 *
 * Left and right reach a choice row from the portal as
 * `korri-semantic-direction` events (the row marks itself
 * `data-korri-horizontal-control` while it can step), never as keys.
 *
 * `valueNode` draws the value when a word is not enough (a ZoneSignal's
 * bars); `value` still names it for assistive tech.
 * `disabled` draws the row in the steel face and keeps it focusable;
 * Korri's `description` stands under it whether or not it is focused, so
 * the reason is on screen; A does nothing.
 *
 * `saving`: a red caret blinks at the value panel's end until Korri
 * answers; the value stays. `problem` is Korri's message on a black panel
 * with a red bar, under the row. `description` is Korri's sentence, in
 * Jersey 10 as written, under an enabled row only while it is focused, so
 * a long list stays one row per setting; `descriptionCaps` marks it as
 * Zone's own word instead (CONNECTING), set in caps.
 *
 * Focus is a prop. The row brings itself into view when it gains it, so a
 * scrolling group never leaves the cursor off screen.
 */
export function ZoneOptionRow({
  label,
  value,
  valueNode,
  interaction,
  description,
  descriptionCaps = false,
  focused = false,
  saving = false,
  disabled = false,
  problem,
  verbatimLabel = false,
  verbatimValue = false,
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
  /** The description is Zone's own word, not Korri's sentence: set it in caps. */
  readonly descriptionCaps?: boolean
  readonly focused?: boolean
  readonly saving?: boolean
  /** The steel face and inert to A; the description shows why. */
  readonly disabled?: boolean
  readonly problem?: string
  /** The label is a fact Korri publishes (a path, an SSID, a program): keep its case. */
  readonly verbatimLabel?: boolean
  /** The value is a fact Korri publishes: keep its case. */
  readonly verbatimValue?: boolean
  /** A on the row: fire the action or raise the keyboard. */
  readonly onPress?: () => void
  /** Left or right on a choice row. */
  readonly onStep?: (direction: -1 | 1) => void
  readonly onFocus?: () => void
}) {
  const row = useRef<HTMLDivElement | null>(null)
  const control = useRef<HTMLButtonElement | null>(null)
  // The whole row, so its notes (the whole fact, the description) come into view with it.
  useEffect(() => {
    if (focused) row.current?.scrollIntoView({ block: "nearest" })
  }, [focused])

  const kind = interaction?.kind ?? "readonly"
  const destructive = interaction?.kind === "action" && interaction.destructive === true
  const clearLabel = interaction?.kind === "sensitiveText" ? interaction.clearLabel : undefined
  const placeholder = (value === undefined || value === "")
    && (interaction?.kind === "text" || interaction?.kind === "sensitiveText") ? interaction.placeholder : undefined
  const steps = kind === "choice" && focused && !saving && !disabled
  const horizontal = useZoneHorizontal(control, steps && onStep !== undefined, kind, onStep)
  const shown = placeholder ?? value
  const cutValue = valueNode === undefined && shown !== undefined && shown !== ""
  useZoneOptionRow(row, [label, shown, focused, saving, disabled, problem, description, valueNode === undefined].join("\u0000"))

  return (
    <div
      className="zone-option-row"
      data-destructive={destructive || undefined}
      data-disabled={disabled || undefined}
      data-focused={focused || undefined}
      data-interaction={kind}
      data-saving={saving || undefined}
      ref={row}
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
        <span className="zone-option-row-label" data-case={verbatimLabel ? "verbatim" : undefined} data-option-clip="slash">
          {verbatimLabel ? (
            <span className="zone-option-row-fit" data-cut-for="label" data-cut-mode={zoneCutMode(label)}>
              <span className="zone-option-row-whole" data-whole-words="">{label}</span>
            </span>
          ) : (
            <span className="zone-option-row-words" data-cut-for="label" data-cut-mode="end" data-whole-words="">{label}</span>
          )}
        </span>
        <span
          className="zone-option-row-value"
          data-case={verbatimValue ? "verbatim" : undefined}
          data-drawn={valueNode === undefined ? undefined : ""}
          data-option-clip="slash"
        >
          {steps ? <i aria-hidden="true" className="zone-option-row-step" data-way="back" /> : null}
          {valueNode !== undefined ? (
            <span className="zone-option-row-text">{valueNode}</span>
          ) : (
            <span
              className="zone-option-row-fit"
              data-cut-for={cutValue ? "value" : undefined}
              data-cut-mode={cutValue ? zoneCutMode(shown) : undefined}
            >
              <span
                className="zone-option-row-text zone-option-row-whole"
                data-placeholder={placeholder === undefined ? undefined : ""}
                data-whole-words=""
              >
                {shown}
              </span>
            </span>
          )}
          {saving ? <span aria-label="Saving" className="zone-option-row-caret" role="img" /> : null}
          {steps ? <i aria-hidden="true" className="zone-option-row-step" data-way="forward" /> : null}
        </span>
      </button>
      {focused && clearLabel !== undefined && !saving ? (
        <span className="zone-option-row-clear">
          <span className="zone-option-row-keycap">X</span>
          {clearLabel}
        </span>
      ) : null}
      {focused ? (
        <span
          aria-hidden="true"
          className="zone-option-row-whole-line"
          data-face={verbatimLabel ? undefined : "caps"}
          data-for="label"
          data-option-clip="slash"
          data-whole={verbatimLabel ? zoneWholeLine(label) : label}
        />
      ) : null}
      {focused && cutValue ? (
        <span aria-hidden="true" className="zone-option-row-whole-line" data-for="value" data-option-clip="slash" data-whole={zoneWholeLine(shown)} />
      ) : null}
      {(focused || disabled) && description !== undefined ? (
        <span className="zone-option-row-description" data-case={descriptionCaps ? undefined : "sentence"} data-option-clip="slash">
          {description}
        </span>
      ) : null}
      {problem !== undefined ? (
        <span className="zone-option-row-problem" data-option-clip="flush" role="alert">{problem}</span>
      ) : null}
    </div>
  )
}
