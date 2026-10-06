import type { SurfaceIdentityDisposition } from "@contracts/surface/korri-surface"
import { useState } from "react"
import { backupText, identityFixture, retiredPublicKey } from "../../fixtures/named-states"
import {
  PICO_IDENTITY_BACKUP_ACTION,
  PICO_IDENTITY_SWITCH_LOCAL_ACTION,
  PICO_IDENTITY_SWITCH_NIP46_ACTION,
  picoIdentityRetiredDeleteAction,
  picoIdentityRetiredExportAction,
} from "../../pico-settings-view"
import { PicoIdentityDialog, type PicoIdentityFormView } from "./PicoIdentityDialog"

const empty: PicoIdentityFormView = { password: "", secret: "", bunkerUri: "", disposition: "transfer", confirmed: false }

const callbacks = {
  form: empty,
  onClose: () => undefined,
  onConfirmed: () => undefined,
  onDisposition: () => undefined,
  onEdit: () => undefined,
  onSubmit: () => undefined,
}

// Intentionally null: a closed dialog must not leave a backdrop or placeholder.
export function Closed() {
  return <PicoIdentityDialog {...callbacks} action={null} />
}

export function SwitchFromBackup() {
  return <Fillable action={PICO_IDENTITY_SWITCH_LOCAL_ACTION} />
}

export function SwitchToNip46() {
  return <Fillable action={PICO_IDENTITY_SWITCH_NIP46_ACTION} />
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
  return <Fillable action={picoIdentityRetiredExportAction(retiredPublicKey)} />
}

export function DeleteRetiredKey() {
  return <Fillable action={picoIdentityRetiredDeleteAction(retiredPublicKey)} />
}

// The form is a prop. The surface keeps it; in these states the part does, so
// the real controls reach an enabled submit button. FilledSwitch pins one.
function Fillable({ action }: { readonly action: string }) {
  const [form, setForm] = useState(empty)
  return <PicoIdentityDialog {...callbacks} action={action} form={form}
    onConfirmed={confirmed => setForm(current => ({ ...current, confirmed }))}
    onDisposition={(disposition: SurfaceIdentityDisposition) => setForm(current => ({ ...current, disposition }))}
    onEdit={(field, value) => setForm(current => ({ ...current, [field]: value }))} />
}

export function FilledSwitch() {
  return <PicoIdentityDialog {...callbacks} action={PICO_IDENTITY_SWITCH_NIP46_ACTION}
    form={{ ...empty, bunkerUri: "bunker://preview", confirmed: true }} />
}

export const name = "Identity Dialog"
export const note = "Backing up or switching the device's identity; the whole screen, nothing around it"

export default function PicoIdentityDialogPart() {
  return (
    <PicoIdentityDialog
      action={PICO_IDENTITY_BACKUP_ACTION}
      form={empty}
      onClose={() => undefined}
      onConfirmed={() => undefined}
      onDisposition={() => undefined}
      onEdit={() => undefined}
      onSubmit={() => undefined}
    />
  )
}
