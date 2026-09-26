import { PICO_IDENTITY_BACKUP_ACTION } from "../../pico-settings-view"
import { PicoIdentityDialog } from "./PicoIdentityDialog"

export const name = "Identity Dialog"
export const note = "Backing up or switching the device's identity; the whole screen, nothing around it"

export default function PicoIdentityDialogPart() {
  return (
    <PicoIdentityDialog
      action={PICO_IDENTITY_BACKUP_ACTION}
      onClose={() => undefined}
      onDeleteRetired={() => undefined}
      onDismissStatus={() => undefined}
      onExport={() => undefined}
      onSwitchLocal={() => undefined}
      onSwitchNip46={() => undefined}
    />
  )
}
