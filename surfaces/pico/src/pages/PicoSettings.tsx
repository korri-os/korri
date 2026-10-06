import "./PicoSettings.css"
import type {
  PicoConfirmation,
  PicoSettingRowView,
  PicoSettingsView,
  PicoTextSettingRowView,
} from "../pico-settings-view"
import { PicoModal } from "../ui/organisms/PicoModal"
import { PicoTextEditor } from "../ui/organisms/PicoTextEditor"
import { PicoSettingsPanel } from "./PicoSettingsPanel"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

const HINTS = [
  { hintKey: "a", label: "SELECT" },
  { hintKey: "b", label: "BACK" },
] as const

const EDITOR_HINTS = [
  { hintKey: "a", label: "SELECT" },
  { hintKey: "b", label: "CANCEL" },
] as const

/** The text setting being changed, and whether clearing it is being asked. */
export interface PicoSettingsEditing {
  readonly settingId: string
  readonly clearing: boolean
}

function findText(settings: PicoSettingsView, settingId: string) {
  for (const group of settings.groups) {
    for (const row of group.rows) {
      if (row.id === settingId && row.control.kind === "text") {
        return { group: group.title, row: row as PicoTextSettingRowView }
      }
    }
  }
  return undefined
}

/**
 * Device facts and settings, in the same shell as everything else.
 *
 * Decides what a press on a row means. A fact: nothing. A cycle: the next value,
 * sent unchanged. A plain action: run now. A destructive action with a
 * confirmation: ask first, in Korri's words. Text: open the editor.
 *
 * The editor takes the screen, and the status bar names the setting. The list
 * stays mounted underneath, hidden, so the group being looked at and the row
 * the cursor came from are still there when the editor closes. Clearing a
 * saved secret asks first: Korri publishes no confirmation for it, so the
 * question is Pico's, in Korri's label.
 */
export function PicoSettings({
  settings,
  group,
  onGroup,
  asking,
  editing,
  onAsk,
  onConfirm,
  onCancel,
  onChange,
  onRun,
  onEdit,
  onCloseEditor,
  onAskClear,
  onCancelClear,
  onDismissProblem,
  clockLabel,
}: {
  readonly settings: PicoSettingsView
  /** Which settings group shows, kept by the owner. */
  readonly group: number
  readonly onGroup: (index: number) => void
  /** A destructive action awaiting the user's yes, when one is. */
  readonly asking?: { readonly actionId: string; readonly confirmation: PicoConfirmation }
  readonly editing?: PicoSettingsEditing
  readonly onAsk: (actionId: string, confirmation: PicoConfirmation) => void
  readonly onConfirm: () => void
  readonly onCancel: () => void
  readonly onChange: (settingId: string, value: string) => void
  readonly onRun: (actionId: string) => void
  readonly onEdit: (settingId: string) => void
  readonly onCloseEditor: () => void
  readonly onAskClear: () => void
  readonly onCancelClear: () => void
  readonly onDismissProblem: () => void
  readonly clockLabel?: string
}) {
  const activate = (row: PicoSettingRowView) => {
    switch (row.control.kind) {
      case "fact":
        return
      case "cycle":
      case "step":
        onChange(row.id, row.control.next)
        return
      case "action":
        if (row.control.confirmation !== undefined) {
          onAsk(row.control.actionId, row.control.confirmation)
        } else {
          onRun(row.control.actionId)
        }
        return
      case "text":
        onEdit(row.id)
        return
    }
  }

  /* A row Korri stopped publishing closes its editor. */
  const edited = editing === undefined ? undefined : findText(settings, editing.settingId)
  const clearLabel = edited?.row.control.clearLabel

  return (
    <PicoScreenShell
      clockLabel={clockLabel}
      hints={edited === undefined ? HINTS : EDITOR_HINTS}
      label={edited === undefined ? "SETTINGS" : edited.row.label.toUpperCase()}
      readout={settings.buildLabel}
      {...(edited === undefined ? {} : { place: "SETTINGS" })}
    >
      <div className="pico-settings-list" hidden={edited !== undefined}>
        <PicoSettingsPanel
          group={group}
          onActivate={activate}
          onGroup={onGroup}
          onDismissProblem={onDismissProblem}
          settings={settings}
        />
      </div>
      {edited === undefined ? null : (
        <PicoTextEditor
          group={edited.group}
          key={edited.row.id}
          onAskClear={onAskClear}
          onClose={onCloseEditor}
          onDismissProblem={onDismissProblem}
          onSave={(value) => onChange(edited.row.id, value)}
          row={edited.row}
        />
      )}
      {editing?.clearing === true && clearLabel !== undefined && edited !== undefined ? (
        <PicoModal
          confirmLabel={clearLabel.toUpperCase()}
          message="To use it again, you must type it again."
          onCancel={onCancelClear}
          onConfirm={() => {
            onCancelClear()
            onChange(edited.row.id, "")
          }}
          title={`${clearLabel.toUpperCase()}?`}
        />
      ) : null}
      {asking === undefined ? null : (
        <PicoModal
          confirmLabel={asking.confirmation.confirmLabel}
          message={asking.confirmation.message}
          onCancel={onCancel}
          onConfirm={onConfirm}
          title={asking.confirmation.title}
        />
      )}
    </PicoScreenShell>
  )
}
