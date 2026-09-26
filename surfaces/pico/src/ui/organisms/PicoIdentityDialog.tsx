import type {
  SurfaceIdentityDisposition,
  SurfaceIdentityManagement,
} from "@contracts/surface/korri-surface"
import * as QRCode from "qrcode"
import { useEffect, useRef, useState } from "react"
import {
  PICO_IDENTITY_BACKUP_ACTION,
  PICO_IDENTITY_SWITCH_LOCAL_ACTION,
  PICO_IDENTITY_SWITCH_NIP46_ACTION,
} from "../../pico-settings-view"

interface PicoIdentityDialogProps {
  readonly action: string | null
  readonly identity?: SurfaceIdentityManagement
  readonly onClose: () => void
  readonly onExport: (password: string, retiredPublicKey?: string) => void
  readonly onSwitchLocal: (
    encryptedSecret: string,
    password: string,
    disposition: SurfaceIdentityDisposition,
    confirmed: boolean,
  ) => void
  readonly onSwitchNip46: (
    bunkerUri: string,
    disposition: SurfaceIdentityDisposition,
    confirmed: boolean,
  ) => void
  readonly onDeleteRetired: (publicKey: string, confirmed: boolean) => void
  readonly onDismissStatus: () => void
}

const retiredKey = (action: string | null, operation: "export" | "delete") => {
  const prefix = `identity:retired:${operation}:`
  return action?.startsWith(prefix) ? action.slice(prefix.length) : undefined
}

export function PicoIdentityDialog({
  action,
  identity,
  onClose,
  onExport,
  onSwitchLocal,
  onSwitchNip46,
  onDeleteRetired,
  onDismissStatus,
}: PicoIdentityDialogProps) {
  const [password, setPassword] = useState("")
  const [secret, setSecret] = useState("")
  const [bunkerUri, setBunkerUri] = useState("")
  const [disposition, setDisposition] = useState<SurfaceIdentityDisposition>("transfer")
  const [confirmed, setConfirmed] = useState(false)
  const [qr, setQr] = useState<string>()
  const dialogRef = useRef<HTMLDivElement>(null)
  const status = identity?.status
  const exportKey = retiredKey(action, "export")
  const deleteKey = retiredKey(action, "delete")
  const working = status?._tag === "Working"
  // Callers pass a new callback on every render. Only a new action resets the
  // dialog; a render alone must not clear input, dismiss status or move focus.
  const dismissStatus = useRef(onDismissStatus)
  dismissStatus.current = onDismissStatus

  useEffect(() => {
    setPassword("")
    setSecret("")
    setBunkerUri("")
    setDisposition("transfer")
    setConfirmed(false)
    setQr(undefined)
    dismissStatus.current()
    requestAnimationFrame(() => {
      dialogRef.current?.querySelector<HTMLElement>("button, input, textarea, select")?.focus()
    })
  }, [action])

  useEffect(() => {
    if (status?._tag !== "BackupReady") {
      setQr(undefined)
      return
    }
    let current = true
    void QRCode.toDataURL(status.encryptedSecret, { width: 384, margin: 2 })
      .then(value => { if (current) setQr(value) })
    return () => { current = false }
  }, [status])

  if (action === null) return null
  const close = () => { onDismissStatus(); onClose() }
  const switching = action === PICO_IDENTITY_SWITCH_LOCAL_ACTION || action === PICO_IDENTITY_SWITCH_NIP46_ACTION
  const title = action === PICO_IDENTITY_BACKUP_ACTION ? "BACK UP IDENTITY"
    : action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? "SWITCH FROM BACKUP"
    : action === PICO_IDENTITY_SWITCH_NIP46_ACTION ? "SWITCH TO NIP-46"
    : exportKey ? "BACK UP RETIRED KEY"
    : "DELETE RETIRED KEY"

  return (
    <div ref={dialogRef} className="pico-identity-dialog" role="dialog" aria-modal="true" aria-label={title}>
      <header><strong>{title}</strong><button type="button" onClick={close}>CLOSE</button></header>
      <div className="pico-identity-dialog-body">
        {status?._tag === "Problem" ? <p className="pico-identity-error">{status.message}</p> : null}
        {status?._tag === "BackupReady" ? (
          <>
            <p>Korri cannot recover a lost key. If this backup is not exported before device loss, the identity is unrecoverable. Store it outside the device.</p>
            {qr ? <img src={qr} alt="QR code for encrypted identity backup" /> : null}
            <textarea readOnly value={status.encryptedSecret} />
            <button type="button" onClick={close}>DONE</button>
          </>
        ) : status?._tag === "Switched" ? (
          <><p>Korri is restarting with the new identity.</p><button type="button" onClick={close}>CLOSE</button></>
        ) : (
          <>
            {(action === PICO_IDENTITY_BACKUP_ACTION || exportKey) ? (
              <>
                <label>Password<input type="password" value={password} onChange={event => setPassword(event.target.value)} /></label>
                <button type="button" disabled={working || !password.trim()} onClick={() => onExport(password, exportKey)}>
                  {working && status?._tag === "Working" ? status.operation : "CREATE BACKUP"}
                </button>
              </>
            ) : null}
            {action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? (
              <>
                <label>Encrypted backup<textarea value={secret} onChange={event => setSecret(event.target.value)} placeholder="ncryptsec…" /></label>
                <label>Password<input type="password" value={password} onChange={event => setPassword(event.target.value)} /></label>
              </>
            ) : null}
            {action === PICO_IDENTITY_SWITCH_NIP46_ACTION ? (
              <label>Bunker URI<textarea value={bunkerUri} onChange={event => setBunkerUri(event.target.value)} placeholder="bunker://…" /></label>
            ) : null}
            {switching ? (
              <>
                <label>Local records<select value={disposition} onChange={event => setDisposition(event.target.value as SurfaceIdentityDisposition)}><option value="transfer">Transfer</option><option value="delete">Delete</option></select></label>
                <label className="pico-identity-check"><input type="checkbox" checked={confirmed} onChange={event => setConfirmed(event.target.checked)} /><span>I understand that peer pairings and stream-client trust will be cleared.</span></label>
                <p>Save files and save states stay under the plugin account.</p>
                <button
                  type="button"
                  className="danger"
                  disabled={working || !confirmed || (action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? !secret.trim() || !password.trim() : !bunkerUri.trim())}
                  onClick={() => action === PICO_IDENTITY_SWITCH_LOCAL_ACTION
                    ? onSwitchLocal(secret, password, disposition, confirmed)
                    : onSwitchNip46(bunkerUri, disposition, confirmed)}
                >{working && status?._tag === "Working" ? status.operation : "SWITCH IDENTITY"}</button>
              </>
            ) : null}
            {deleteKey ? (
              <>
                <p>Delete this private key only after its encrypted backup is stored elsewhere.</p>
                <label className="pico-identity-check"><input type="checkbox" checked={confirmed} onChange={event => setConfirmed(event.target.checked)} /><span>I have stored a recovery backup.</span></label>
                <button type="button" className="danger" disabled={working || !confirmed} onClick={() => onDeleteRetired(deleteKey, confirmed)}>DELETE KEY</button>
              </>
            ) : null}
          </>
        )}
      </div>
    </div>
  )
}
