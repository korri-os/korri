/** Shift settings Row — one fact, optionally actionable. */
import { ChevronRight, LoaderCircle } from "lucide-react"
import type { ReactNode } from "react"
import {
  SHIFT_DESIGN_PARTS,
  shiftDesignPartAttrs,
} from "../../shift-design-parts"

export interface ShiftSettingRowProps {
  readonly label: string
  readonly index: number
  readonly value?: string
  readonly description?: string
  /** Where a layered value in force comes from, in Korri's words. */
  readonly source?: string
  /** Korri says the value stays on this device. */
  readonly deviceOnly?: boolean
  readonly focused?: boolean
  readonly saving?: boolean
  readonly onFocus?: () => void
  /** Present makes this a real button. Absent keeps it a readable focus anchor. */
  readonly onSelect?: () => void
}

function content(
  label: string,
  value: string | undefined,
  description: string | undefined,
  actionable: boolean,
  saving: boolean,
  source: string | undefined,
  deviceOnly: boolean,
): ReactNode {
  return (
    <>
      <span className="shift-setting-row-text">
        <span className="shift-setting-row-label">{label}</span>
        {source !== undefined || deviceOnly ? (
          <span className="shift-setting-row-source">
            {deviceOnly ? <span className="shift-setting-row-mark">Device-only</span> : null}
            {source === undefined ? null : <span>{source}</span>}
          </span>
        ) : null}
        {description ? (
          <span className="shift-setting-row-description">{description}</span>
        ) : null}
      </span>
      <span className="shift-setting-row-tail">
        {value === undefined ? null : (
          <span className="shift-setting-row-value">{value}</span>
        )}
        {saving ? (
          <LoaderCircle className="shift-setting-row-spinner" aria-hidden />
        ) : actionable ? (
          <ChevronRight className="shift-setting-row-chevron" aria-hidden />
        ) : null}
      </span>
    </>
  )
}

export function ShiftSettingRow({
  label,
  index,
  value,
  description,
  source,
  deviceOnly = false,
  focused = false,
  saving = false,
  onFocus,
  onSelect,
}: ShiftSettingRowProps) {
  const common = {
    className: "shift-setting-row",
    "data-setting-index": index,
    "data-focused": focused || undefined,
    "data-actionable": Boolean(onSelect) || undefined,
    "data-saving": saving || undefined,
    "aria-label": value === undefined ? label : `${label}: ${value}`,
    onFocus,
    ...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.settingRow, label),
  }
  const body = content(label, value, description, Boolean(onSelect), saving, source, deviceOnly)

  return onSelect ? (
    <button type="button" onClick={onSelect} {...common}>
      {body}
    </button>
  ) : (
    <div tabIndex={0} role="group" {...common}>
      {body}
    </div>
  )
}
