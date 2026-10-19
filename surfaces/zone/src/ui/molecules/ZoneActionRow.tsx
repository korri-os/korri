import "./ZoneActionRow.css"
import { ZoneButton } from "../atoms/ZoneButton"

export interface ZoneAction {
  readonly id: string
  readonly label: string
  readonly disabled?: boolean
  /** Why it is disabled, verbatim from Korri. */
  readonly reason?: string
}

/**
 * The actions under the focused card, centred, with room between them for
 * the triangles of whichever is focused. `lit` is the default action; it
 * keeps its underline while the cursor is elsewhere. When the row is
 * narrower than its buttons, the gap gives first and then the row wraps
 * onto a second line; a label wider than the row truncates with an
 * ellipsis. No button is dropped.
 */
export function ZoneActionRow({
  actions,
  lit,
  focused,
  pressed,
  onPress,
  onFocus,
}: {
  readonly actions: readonly ZoneAction[]
  /** The default action's id. */
  readonly lit?: string
  /** The action the cursor is on, when the cursor is on the row at all. */
  readonly focused?: string
  /** The action mid-press. */
  readonly pressed?: string
  readonly onPress: (id: string) => void
  readonly onFocus?: (id: string) => void
}) {
  return (
    <div className="zone-action-row">
      {actions.map(action => (
        <ZoneButton
          disabled={action.disabled}
          focused={action.id === focused}
          key={action.id}
          label={action.label}
          lit={action.id === lit}
          onFocus={onFocus === undefined ? undefined : () => onFocus(action.id)}
          onPress={() => onPress(action.id)}
          pressed={action.id === pressed}
          reason={action.reason}
        />
      ))}
    </div>
  )
}
