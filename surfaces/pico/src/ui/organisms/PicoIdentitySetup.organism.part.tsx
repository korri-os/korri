import { PicoIdentitySetup, type PicoIdentityActions } from "./PicoIdentitySetup"

export const name = "Identity Setup"
export const note = "Keep the key Korri made, restore one from a backup, or connect a signer"

const none: PicoIdentityActions = {
  onChoose: () => undefined, onEdit: () => undefined, onSubmit: () => undefined, onBackUp: () => undefined, onContinue: () => undefined,
}
const on = { enabled: true, note: "You have an ncryptsec backup and its password." }
const signer = { enabled: true, note: "NIP-46: the key stays in your signer app." }
const off = { enabled: false, note: "Needs a connection." }

export default function PicoIdentitySetupPart() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Choose", offline: false, restore: on, signer }} />
}

export function ChooseOffline() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Choose", offline: true, restore: off, signer: off }} />
}

export function Created() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Ready", origin: "created", canBackUp: true }} />
}

export function Restored() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Ready", origin: "restored", canBackUp: true }} />
}

export function Restore() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Restore", backup: "ncryptsec1qgg9947r…", passwordLength: 12, ready: true }} />
}

export function RestoreEmpty() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Restore", backup: "", passwordLength: 0, ready: false }} />
}

export function RestoreRefused() {
  return (
    <PicoIdentitySetup
      actions={none}
      view={{ _tag: "Restore", backup: "ncryptsec1qgg9947r…", passwordLength: 0, ready: false, problem: "That password does not open this backup." }}
    />
  )
}

export function Working() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Working", label: "Opening your backup" }} />
}

export function Signer() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Signer", uri: "bunker://7e1f0c3a9b?rel…", ready: true }} />
}

export function Approving() {
  return <PicoIdentitySetup actions={none} view={{ _tag: "Approving", uri: "bunker://7e1f0c3a9b?rel…" }} />
}
