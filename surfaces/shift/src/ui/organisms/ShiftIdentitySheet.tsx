import type {
  SurfaceIdentityDisposition,
  SurfaceIdentityManagement,
} from "@contracts/surface/korri-surface"
import * as QRCode from "qrcode"
import { useEffect, useMemo, useState } from "react"
import { ShiftSheetAction } from "../molecules/ShiftSheetAction"
import { ShiftSheetBody } from "./ShiftSheetBody"
import { ShiftSheetGroup } from "./ShiftSheetGroup"
import { ShiftSheetHeader } from "./ShiftSheetHeader"
import { ShiftSheetPanel } from "./ShiftSheetPanel"
import { ShiftSheetRoot } from "./ShiftSheetRoot"
import { ShiftSheetTitle } from "./ShiftSheetTitle"

export const IDENTITY_BACKUP_ACTION = "identity:backup"
export const IDENTITY_SWITCH_LOCAL_ACTION = "identity:switch:local"
export const IDENTITY_SWITCH_NIP46_ACTION = "identity:switch:nip46"
export const identityRetiredExportAction = (key: string) => `identity:retired:export:${key}`
export const identityRetiredDeleteAction = (key: string) => `identity:retired:delete:${key}`

interface ShiftIdentitySheetProps {
  readonly action: string | null
  readonly identity?: SurfaceIdentityManagement
  readonly onClose: () => void
  readonly onExport: (password: string, retiredPublicKey?: string) => void
  readonly onSwitchLocal: (
    encryptedSecret: string,
    password: string,
    disposition: SurfaceIdentityDisposition,
    trustLossConfirmed: boolean,
  ) => void
  readonly onSwitchNip46: (
    bunkerUri: string,
    disposition: SurfaceIdentityDisposition,
    trustLossConfirmed: boolean,
  ) => void
  readonly onDeleteRetired: (publicKey: string, backupConfirmed: boolean) => void
  readonly onDismissStatus: () => void
}

const retiredKey = (action: string | null, operation: "export" | "delete") => {
  const prefix = `identity:retired:${operation}:`
  return action?.startsWith(prefix) ? action.slice(prefix.length) : undefined
}

export function ShiftIdentitySheet({
  action,
  identity,
  onClose,
  onExport,
  onSwitchLocal,
  onSwitchNip46,
  onDeleteRetired,
  onDismissStatus,
}: ShiftIdentitySheetProps) {
  const [password, setPassword] = useState("")
  const [encryptedSecret, setEncryptedSecret] = useState("")
  const [bunkerUri, setBunkerUri] = useState("")
  const [disposition, setDisposition] = useState<SurfaceIdentityDisposition>("transfer")
  const [confirmed, setConfirmed] = useState(false)
  const [qrDataUrl, setQrDataUrl] = useState<string>()
  const exportKey = retiredKey(action, "export")
  const deleteKey = retiredKey(action, "delete")
  const status = identity?.status
  const working = status?._tag === "Working"
  const workingLabel = status?._tag === "Working" ? status.operation : undefined

  useEffect(() => {
    setPassword("")
    setEncryptedSecret("")
    setBunkerUri("")
    setDisposition("transfer")
    setConfirmed(false)
    setQrDataUrl(undefined)
    onDismissStatus()
  }, [action, onDismissStatus])

  useEffect(() => {
    if (status?._tag !== "BackupReady") {
      setQrDataUrl(undefined)
      return
    }
    let current = true
    void QRCode.toDataURL(status.encryptedSecret, {
      errorCorrectionLevel: "M",
      margin: 2,
      width: 512,
      color: { dark: "#101010", light: "#ffffff" },
    }).then(url => {
      if (current) setQrDataUrl(url)
    })
    return () => { current = false }
  }, [status])

  const title = useMemo(() => {
    if (action === IDENTITY_BACKUP_ACTION) return "Back up identity"
    if (action === IDENTITY_SWITCH_LOCAL_ACTION) return "Switch from backup"
    if (action === IDENTITY_SWITCH_NIP46_ACTION) return "Switch to NIP-46"
    if (exportKey) return "Export retired key"
    if (deleteKey) return "Delete retired key"
    return "Identity"
  }, [action, deleteKey, exportKey])

  const close = () => {
    onDismissStatus()
    onClose()
  }

  return (
    <ShiftSheetRoot open={action !== null} onClose={close} label={title}>
      <ShiftSheetPanel>
        <ShiftSheetHeader><ShiftSheetTitle>{title}</ShiftSheetTitle></ShiftSheetHeader>
        <ShiftSheetBody>
          {status?._tag === "Problem" ? (
            <ShiftSheetGroup title="Could not continue">
              <p className="shift-setting-problem">{status.message}</p>
              <ShiftSheetAction label="Try again" onSelect={onDismissStatus} />
            </ShiftSheetGroup>
          ) : null}

          {status?._tag === "BackupReady" ? (
            <ShiftSheetGroup title="Encrypted NIP-49 backup">
              <p className="shift-identity-warning">
                Korri cannot recover a lost key. If this backup is not exported before device loss, the identity is unrecoverable. Store the encrypted text and password outside the device.
              </p>
              {qrDataUrl ? (
                <img className="shift-identity-qr" src={qrDataUrl} alt="QR code for encrypted identity backup" />
              ) : null}
              <textarea className="shift-setting-input shift-identity-secret" readOnly value={status.encryptedSecret} />
              <ShiftSheetAction label="Done" onSelect={close} />
            </ShiftSheetGroup>
          ) : null}

          {status?._tag === "Switched" ? (
            <ShiftSheetGroup title="Identity switched">
              <p className="shift-identity-warning">Korri is restarting with the new identity.</p>
              <ShiftSheetAction label="Close" onSelect={close} />
            </ShiftSheetGroup>
          ) : null}

          {status?._tag !== "BackupReady" && status?._tag !== "Switched" ? (
            <>
              {(action === IDENTITY_BACKUP_ACTION || exportKey) ? (
                <ShiftSheetGroup title="Encryption password">
                  <input
                    className="shift-setting-input"
                    type="password"
                    value={password}
                    autoComplete="new-password"
                    onChange={event => setPassword(event.target.value)}
                    placeholder="Backup password"
                  />
                  <ShiftSheetAction
                    label={workingLabel ?? "Create encrypted backup"}
                    disabled={working || password.trim().length === 0}
                    onSelect={() => onExport(password, exportKey)}
                  />
                </ShiftSheetGroup>
              ) : null}

              {action === IDENTITY_SWITCH_LOCAL_ACTION ? (
                <ShiftSheetGroup title="NIP-49 backup">
                  <textarea
                    className="shift-setting-input shift-identity-secret"
                    value={encryptedSecret}
                    onChange={event => setEncryptedSecret(event.target.value)}
                    placeholder="ncryptsec…"
                  />
                  <input
                    className="shift-setting-input"
                    type="password"
                    value={password}
                    autoComplete="current-password"
                    onChange={event => setPassword(event.target.value)}
                    placeholder="Backup password"
                  />
                </ShiftSheetGroup>
              ) : null}

              {action === IDENTITY_SWITCH_NIP46_ACTION ? (
                <ShiftSheetGroup title="Remote signer">
                  <textarea
                    className="shift-setting-input shift-identity-secret"
                    value={bunkerUri}
                    onChange={event => setBunkerUri(event.target.value)}
                    placeholder="bunker://…"
                  />
                </ShiftSheetGroup>
              ) : null}

              {(action === IDENTITY_SWITCH_LOCAL_ACTION || action === IDENTITY_SWITCH_NIP46_ACTION) ? (
                <ShiftSheetGroup title="Local person records">
                  <select
                    className="shift-setting-input"
                    value={disposition}
                    onChange={event => setDisposition(event.target.value as SurfaceIdentityDisposition)}
                  >
                    <option value="transfer">Transfer to the new identity</option>
                    <option value="delete">Delete from this device</option>
                  </select>
                  <label className="shift-identity-confirmation">
                    <input
                      type="checkbox"
                      checked={confirmed}
                      onChange={event => setConfirmed(event.target.checked)}
                    />
                    <span>I understand that peer pairings and stream-client trust will be cleared.</span>
                  </label>
                  <p className="shift-identity-warning">
                    Save files and save states stay under the plugin account and are not moved.
                  </p>
                  <ShiftSheetAction
                    label={workingLabel ?? "Switch identity"}
                    tone="danger"
                    disabled={working || !confirmed || (
                      action === IDENTITY_SWITCH_LOCAL_ACTION
                        ? encryptedSecret.trim().length === 0 || password.trim().length === 0
                        : bunkerUri.trim().length === 0
                    )}
                    onSelect={() => {
                      if (action === IDENTITY_SWITCH_LOCAL_ACTION) {
                        onSwitchLocal(encryptedSecret, password, disposition, confirmed)
                      } else {
                        onSwitchNip46(bunkerUri, disposition, confirmed)
                      }
                    }}
                  />
                </ShiftSheetGroup>
              ) : null}

              {deleteKey ? (
                <ShiftSheetGroup title="Permanent deletion">
                  <p className="shift-identity-warning">
                    Delete this retired private key only after its encrypted backup is stored elsewhere.
                  </p>
                  <label className="shift-identity-confirmation">
                    <input
                      type="checkbox"
                      checked={confirmed}
                      onChange={event => setConfirmed(event.target.checked)}
                    />
                    <span>I have stored a recovery backup.</span>
                  </label>
                  <ShiftSheetAction
                    label={workingLabel ?? "Delete retired key"}
                    tone="danger"
                    disabled={working || !confirmed}
                    onSelect={() => onDeleteRetired(deleteKey, confirmed)}
                  />
                </ShiftSheetGroup>
              ) : null}
            </>
          ) : null}
        </ShiftSheetBody>
      </ShiftSheetPanel>
    </ShiftSheetRoot>
  )
}
