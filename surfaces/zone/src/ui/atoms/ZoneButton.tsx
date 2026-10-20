import "./ZoneButton.css"
import { ZoneCaps } from "./ZoneCaps"
import { ZonePanel } from "./ZonePanel"
import { ZoneTriangle } from "./ZoneTriangle"
import { ZoneUnderline } from "./ZoneUnderline"

/**
 * An action: a black parallelogram with its verb in white caps. `lit` is
 * the row's default action and wears the pink underline; `focused` adds the
 * two red triangles; `pressed` sinks the panel into its underline for the
 * press time. A disabled button dims, keeps no underline, stays focusable
 * (the triangles still find it) and says why on a line under itself, in
 * Korri's words, so the cursor can reach it and read it.
 *
 * `focused` is drawn from the prop. The host moves DOM focus; `onFocus`
 * lets the page follow it.
 */
export function ZoneButton({
  label,
  lit = false,
  focused = false,
  pressed = false,
  disabled = false,
  reason,
  onPress,
  onFocus,
}: {
  readonly label: string
  readonly lit?: boolean
  readonly focused?: boolean
  readonly pressed?: boolean
  readonly disabled?: boolean
  /** Why it is disabled, verbatim from Korri. Shown only when disabled. */
  readonly reason?: string
  readonly onPress: () => void
  readonly onFocus?: () => void
}) {
  const showReason = disabled && reason !== undefined
  return (
    <button
      className="zone-button"
      aria-disabled={disabled ? "true" : undefined}
      data-disabled={disabled ? "on" : "off"}
      data-focused={focused ? "on" : "off"}
      data-lit={lit ? "on" : "off"}
      data-pressed={pressed ? "on" : "off"}
      onClick={disabled ? undefined : onPress}
      onFocus={onFocus}
      type="button"
    >
      <span className="zone-button-face">
        {focused ? (
          <span className="zone-button-cursor-left">
            <ZoneTriangle direction="right" size="inline" />
          </span>
        ) : null}
        <span className="zone-button-body">
          <ZonePanel edge={false}>
            <ZoneCaps>{label}</ZoneCaps>
            {(lit || focused) && !disabled ? <ZoneUnderline /> : null}
          </ZonePanel>
        </span>
        {focused ? (
          <span className="zone-button-cursor-right">
            <ZoneTriangle direction="left" size="inline" />
          </span>
        ) : null}
      </span>
      {showReason ? <span className="zone-button-reason">{reason}</span> : null}
    </button>
  )
}
