import "./PicoSettings.css"
import type { ReactNode } from "react"
import {
  type PicoConfirmation,
  type PicoSettingRowView,
  type PicoSettingsView,
  picoTextSettingFrom,
} from "../pico-settings-view"
import type { PicoTextDraft } from "../pico-text-draft"
import type { PicoWifiEntryView } from "../pico-wifi-view"
import { PicoBadge } from "../ui/atoms/PicoBadge"
import type { PicoTyping } from "../ui/molecules/PicoKeyboard"
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

/** The text setting being changed, what is typed, and whether clearing it is being asked. */
export interface PicoSettingsEditing {
  readonly settingId: string
  readonly clearing: boolean
  readonly draft: PicoTextDraft
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
 *
 * Joining a Wi-Fi network types in the same editor (`wifi`): a password with
 * SHOW, or a hidden network's name and then its password. CONNECT sends it.
 */
export function PicoSettings({
  settings,
  group,
  onGroup,
  asking,
  editing,
  wifi,
  onAsk,
  onConfirm,
  onCancel,
  onChange,
  onRun,
  onEdit,
  onCloseEditor,
  onAskClear,
  onCancelClear,
  onConfirmClear,
  onSaveText,
  typing,
  onSubmitWifi,
  onToggleShown,
  onDismissProblem,
  clockLabel,
  title = "SETTINGS",
  notice,
  lead,
  from,
  banner,
}: {
  readonly settings: PicoSettingsView
  /** Which settings group shows, kept by the owner. */
  readonly group: number
  readonly onGroup: (index: number) => void
  /** A destructive action awaiting the user's yes, when one is. */
  readonly asking?: { readonly actionId: string; readonly confirmation: PicoConfirmation }
  readonly editing?: PicoSettingsEditing
  /** A Wi-Fi network being joined, when one is. */
  readonly wifi?: PicoWifiEntryView
  readonly onAsk: (actionId: string, confirmation: PicoConfirmation) => void
  readonly onConfirm: () => void
  readonly onCancel: () => void
  readonly onChange: (settingId: string, value: string) => void
  readonly onRun: (actionId: string) => void
  readonly onEdit: (settingId: string) => void
  readonly onCloseEditor: () => void
  readonly onAskClear: () => void
  readonly onCancelClear: () => void
  readonly onConfirmClear: () => void
  /** Save what is typed in the editor. */
  readonly onSaveText: () => void
  /** The editor's keys. */
  readonly typing: PicoTyping
  /** CONNECT, or NEXT after a hidden network's name. */
  readonly onSubmitWifi: () => void
  /** SHOW on a Wi-Fi password. */
  readonly onToggleShown: () => void
  readonly onDismissProblem: () => void
  readonly clockLabel?: string
  /** The screen's name in the status bar. A game's launch settings use the
   * same screen under their own name. */
  readonly title?: string
  /** What Korri says about the whole screen (still reading, changed
   * elsewhere, failed). It takes the groups' place until Korri is ready:
   * values that changed elsewhere are not worth changing again. The spine
   * shown is kept, so the person returns to it. */
  readonly notice?: ReactNode
  /** A control on the floor row, before the hints, while nothing is typed. */
  readonly lead?: ReactNode
  /** Where Back leads from this screen, for the status bar. Absent: Korri's home. */
  readonly from?: string
  /** One line above the groups about all of them, with its own controls. */
  readonly banner?: ReactNode
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
      case "network":
        onRun(row.control.actionId)
        return
    }
  }

  /* A row Korri stopped publishing closes its editor. */
  const edited = editing === undefined ? undefined : picoTextSettingFrom(settings, editing.settingId)
  const clearLabel = edited?.row.control.clearLabel
  const typingOpen = edited !== undefined || wifi !== undefined
  const place = wifi?.place ?? edited?.row.label.toUpperCase()

  return (
    <PicoScreenShell
      clockLabel={clockLabel}
      hints={typingOpen ? EDITOR_HINTS : HINTS}
      label={place ?? title}
      readout={settings.buildLabel}
      {...(place === undefined ? (from === undefined ? {} : { place: from }) : { place: title })}
      {...(lead === undefined || typingOpen ? {} : { lead })}
    >
      <div className="pico-settings-list" hidden={typingOpen}>
        {notice === undefined && banner !== undefined ? <div className="pico-settings-banner">{banner}</div> : null}
        {notice ?? (
          <PicoSettingsPanel
            group={group}
            onActivate={activate}
            onGroup={onGroup}
            onDismissProblem={onDismissProblem}
            settings={settings}
          />
        )}
      </div>
      {edited === undefined || editing === undefined ? null : (
        <PicoTextEditor
          actions={[
            {
              label: "SAVE",
              detail: edited.row.state === "saving" ? <PicoBadge text="SAVING" tone="info" /> : undefined,
              disabled: editing.draft.text.trim() === "",
              onPress: onSaveText,
            },
            { label: "CANCEL", onPress: onCloseEditor },
            ...(edited.row.control.sensitive && clearLabel !== undefined
              ? [{ label: clearLabel.toUpperCase(), danger: true, alone: true, onPress: onAskClear }]
              : []),
          ]}
          busy={edited.row.state === "saving"}
          draft={editing.draft}
          formLabel={`Change ${edited.row.label}`}
          heading={{
            group: edited.group,
            ...(edited.row.description === undefined ? {} : { description: edited.row.description }),
          }}
          key={edited.row.id}
          label={edited.row.label}
          maxLength={edited.row.control.maxLength}
          now={edited.row.value}
          placeholder={edited.row.control.placeholder}
          problem={typeof edited.row.state === "object" && "problem" in edited.row.state
            ? { message: edited.row.state.problem, onDismiss: onDismissProblem }
            : undefined}
          secret={edited.row.control.sensitive}
          typing={typing}
        />
      )}
      {wifi === undefined ? null : (
        <PicoTextEditor
          actions={[
            { label: wifi.submitLabel, disabled: !wifi.ready, onPress: onSubmitWifi },
            { label: "CANCEL", onPress: onCloseEditor },
          ]}
          draft={wifi.draft}
          formLabel={wifi.fieldLabel}
          heading={{ group: "WI-FI", ...(wifi.note === undefined ? {} : { description: wifi.note }) }}
          key={`${wifi.place}:${wifi.prompt}`}
          label={wifi.fieldLabel}
          maxLength={wifi.maxLength}
          placeholder={wifi.placeholder}
          prompt={wifi.prompt}
          reveal={wifi.secret ? { shown: wifi.shown, onToggle: onToggleShown } : undefined}
          secret={wifi.secret}
          typing={typing}
        />
      )}
      {editing?.clearing === true && clearLabel !== undefined && edited !== undefined ? (
        <PicoModal
          confirmLabel={clearLabel.toUpperCase()}
          message="To use it again, you must type it again."
          onCancel={onCancelClear}
          onConfirm={onConfirmClear}
          title={`${clearLabel.toUpperCase()}?`}
        />
      ) : null}
      {asking === undefined ? null : (
        <PicoModal
          confirmLabel={asking.confirmation.confirmLabel}
          destructive={asking.confirmation.destructive !== false}
          message={asking.confirmation.message}
          onCancel={onCancel}
          onConfirm={onConfirm}
          title={asking.confirmation.title}
        />
      )}
    </PicoScreenShell>
  )
}
