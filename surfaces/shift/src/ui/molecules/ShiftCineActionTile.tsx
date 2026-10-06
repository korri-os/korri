import {
  KeyRound,
  type LucideIcon,
  Settings,
  Square,
  Wrench,
  X,
} from "lucide-react"
import {
  SHIFT_DESIGN_PARTS,
  shiftDesignPartAttrs,
} from "../../shift-design-parts"

/**
 * A trailing non-game rail entry for a host-declared action (grant storage
 * access or stop the running game). It is the Library tile's twin:
 * the same `.shift-cine-tile` skin, focus lift, and `data-cine-index` centering
 * math, with a motif and label instead of cover art.
 *
 * The host names the action; Shift only chooses a glyph for it and falls back
 * to a neutral one for ids it does not recognise — so a new host action appears
 * in the rail without a surface change.
 */
const ACTION_ICONS: Record<string, LucideIcon> = {
  "storage-access": KeyRound,
  "background-notice": Settings,
  "shift:settings": Settings,
  stop: Square,
  "cancel-launch": X,
}

/** Exact pending-launch cancellations carry the launch in their id. */
const iconFor = (actionId: string): LucideIcon =>
  ACTION_ICONS[actionId] ?? (actionId.startsWith("cancel-pending:") ? X : Wrench)

export interface ShiftCineActionTileProps {
  readonly index: number
  readonly actionId: string
  readonly label: string
  readonly disabled?: boolean
  readonly focused?: boolean
  readonly onFocus: () => void
  readonly onActivate: () => void
}

export function ShiftCineActionTile({
  index,
  actionId,
  label,
  disabled,
  focused,
  onFocus,
  onActivate,
}: ShiftCineActionTileProps) {
  const Icon = iconFor(actionId)

  return (
    <button
      type="button"
      data-cine-index={index}
      data-focused={focused || undefined}
      disabled={disabled}
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.cineLibraryTile, actionId)}
      className="shift-cine-tile shift-cine-tile-affordance"
      aria-label={label}
      onFocus={onFocus}
      onClick={onActivate}
    >
      <span className="shift-cine-tile-affordance-inner">
        <Icon className="shift-cine-tile-affordance-icon" aria-hidden />
        <span className="shift-cine-tile-affordance-label">{label}</span>
      </span>
    </button>
  )
}
