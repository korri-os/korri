import { backupText, identityFixture, retiredPublicKey } from "../../fixtures/named-states"
import {
  PICO_IDENTITY_BACKUP_ACTION,
  PICO_IDENTITY_SWITCH_LOCAL_ACTION,
  PICO_IDENTITY_SWITCH_NIP46_ACTION,
  picoIdentityRetiredDeleteAction,
  picoIdentityRetiredExportAction,
} from "../../pico-settings-view"
import { PicoIdentityDialog } from "./PicoIdentityDialog"

const callbacks = {
  onClose: () => undefined,
  onDeleteRetired: () => undefined,
  onDismissStatus: () => undefined,
  onExport: () => undefined,
  onSwitchLocal: () => undefined,
  onSwitchNip46: () => undefined,
}

// Intentionally null: a closed dialog must not leave a backdrop or placeholder.
export function Closed() {
  return <PicoIdentityDialog {...callbacks} action={null} />
}

export function SwitchFromBackup() {
  return <PicoIdentityDialog {...callbacks} action={PICO_IDENTITY_SWITCH_LOCAL_ACTION} />
}

export function SwitchToNip46() {
  return <PicoIdentityDialog {...callbacks} action={PICO_IDENTITY_SWITCH_NIP46_ACTION} />
}

export function Working() {
  return <PicoIdentityDialog {...callbacks} action={PICO_IDENTITY_BACKUP_ACTION} identity={identityFixture({ _tag: "Working", operation: "Encrypting backup" })} />
}

export function Problem() {
  return <PicoIdentityDialog {...callbacks} action={PICO_IDENTITY_BACKUP_ACTION} identity={identityFixture({ _tag: "Problem", message: "The backup could not be created. Try again with a new password." })} />
}

export function BackupReady() {
  return <PicoIdentityDialog {...callbacks} action={PICO_IDENTITY_BACKUP_ACTION} identity={identityFixture({ _tag: "BackupReady", encryptedSecret: backupText })} />
}

export function Switched() {
  return <PicoIdentityDialog {...callbacks} action={PICO_IDENTITY_SWITCH_LOCAL_ACTION} identity={identityFixture({ _tag: "Switched", ownerPublicKey: "preview-public-key" })} />
}

export function BackUpRetiredKey() {
  return <PicoIdentityDialog {...callbacks} action={picoIdentityRetiredExportAction(retiredPublicKey)} />
}

export function DeleteRetiredKey() {
  return <PicoIdentityDialog {...callbacks} action={picoIdentityRetiredDeleteAction(retiredPublicKey)} />
}

// Passwords, backup/URI input, disposition and confirmation are local form
// state. Use the real controls to reach enabled submit buttons; no initial-form
// props or scripted clicks are added just for the gallery.
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
