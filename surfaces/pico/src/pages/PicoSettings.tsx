import { useState } from "react"
import type { PicoConfirmation, PicoSettingRowView, PicoSettingsView } from "../pico-settings-view"
import { PicoNotice } from "../ui/molecules/PicoNotice"
import { PicoModal } from "../ui/organisms/PicoModal"
import { PicoSettingsPanel } from "../ui/organisms/PicoSettingsPanel"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

const HINTS = [
  { hintKey: "a", label: "SELECT" },
  { hintKey: "b", label: "BACK" },
] as const

/**
 * Device facts and settings, in the same shell as everything else.
 *
 * Decides what a press on a row means. A fact: nothing. A cycle: the next value,
 * sent unchanged. A plain action: run now. A destructive action with a
 * confirmation: ask first, in Korri's words. Text: say there is no keyboard
 * yet, rather than pretend.
 */
export function PicoSettings({
  settings,
  asking,
  onAsk,
  onConfirm,
  onCancel,
  onChange,
  onRun,
  onDismissProblem,
  clockLabel,
}: {
  readonly settings: PicoSettingsView
  /** A destructive action awaiting the user's yes, when one is. */
  readonly asking?: { readonly actionId: string; readonly confirmation: PicoConfirmation }
  readonly onAsk: (actionId: string, confirmation: PicoConfirmation) => void
  readonly onConfirm: () => void
  readonly onCancel: () => void
  readonly onChange: (settingId: string, value: string) => void
  readonly onRun: (actionId: string) => void
  readonly onDismissProblem: () => void
  readonly clockLabel?: string
}) {
  const [noKeyboard, setNoKeyboard] = useState(false)

  const activate = (row: PicoSettingRowView) => {
    setNoKeyboard(false)
    switch (row.control.kind) {
      case "fact":
        return
      case "cycle":
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
        setNoKeyboard(true)
        return
    }
  }

  return (
    <PicoScreenShell backdrop="none" clockLabel={clockLabel} hints={HINTS} label="PICO ▸ SETTINGS">
      <PicoSettingsPanel
        onActivate={activate}
        onDismissProblem={onDismissProblem}
        settings={settings}
      />
      {noKeyboard ? (
        <div className="pico-settings-toast">
          <PicoNotice
            actions={[{ label: "OK", onPress: () => setNoKeyboard(false) }]}
            kicker="NO KEYBOARD YET"
            message="Pico cannot type this yet. Change it from another surface for now."
            tone="info"
          />
        </div>
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
