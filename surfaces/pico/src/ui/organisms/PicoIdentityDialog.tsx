import "../../pico-motion.css"
import "./PicoIdentityDialog.css"
import type {
  SurfaceIdentityDisposition,
  SurfaceIdentityManagement,
} from "@contracts/surface/korri-surface"
import { useEffect, useRef } from "react"
import {
  PICO_IDENTITY_BACKUP_ACTION,
  PICO_IDENTITY_SWITCH_LOCAL_ACTION,
  PICO_IDENTITY_SWITCH_NIP46_ACTION,
  picoIdentityRetiredKey,
} from "../../pico-settings-view"

/** What the player has filled in. The owner keeps it; a new action starts it empty. */
export interface PicoIdentityFormView {
  readonly password: string
  readonly secret: string
  readonly bunkerUri: string
  readonly disposition: SurfaceIdentityDisposition
  readonly confirmed: boolean
}

interface PicoIdentityDialogProps {
  readonly action: string | null
  readonly identity?: SurfaceIdentityManagement
  readonly form: PicoIdentityFormView
  /** The backup Korri made, drawn as a QR code, once it is drawn. */
  readonly qr?: string
  readonly onEdit: (field: "password" | "secret" | "bunkerUri", value: string) => void
  readonly onDisposition: (disposition: SurfaceIdentityDisposition) => void
  readonly onConfirmed: (confirmed: boolean) => void
  /** Send what the form holds for this action. */
  readonly onSubmit: () => void
  readonly onClose: () => void
}

/**
 * Back up, switch or retire this device's person identity.
 *
 * The dialog holds no state. The form, the QR code and when Korri's status is
 * cleared are the owner's (src/state/update.ts): a new action starts an empty
 * form, and a re-render or a republished model changes neither.
 */
export function PicoIdentityDialog({
  action,
  identity,
  form,
  qr,
  onEdit,
  onDisposition,
  onConfirmed,
  onSubmit,
  onClose,
}: PicoIdentityDialogProps) {
  const { password, secret, bunkerUri, disposition, confirmed } = form
  const dialogRef = useRef<HTMLDivElement>(null)
  const status = identity?.status
  const exportKey = picoIdentityRetiredKey(action, "export")
  const deleteKey = picoIdentityRetiredKey(action, "delete")
  const working = status?._tag === "Working"

  // A new action puts the cursor on its first control. DOM mechanics only.
  useEffect(() => {
    requestAnimationFrame(() => {
      dialogRef.current?.querySelector<HTMLElement>("button, input, textarea, select")?.focus()
    })
  }, [action])

  if (action === null) return null
  const close = onClose
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
                <label>Password<input type="password" value={password} onChange={event => onEdit("password", event.target.value)} /></label>
                <button type="button" disabled={working || !password.trim()} onClick={onSubmit}>
                  {working && status?._tag === "Working" ? status.operation : "CREATE BACKUP"}
                </button>
              </>
            ) : null}
            {action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? (
              <>
                <label>Encrypted backup<textarea value={secret} onChange={event => onEdit("secret", event.target.value)} placeholder="ncryptsec…" /></label>
                <label>Password<input type="password" value={password} onChange={event => onEdit("password", event.target.value)} /></label>
              </>
            ) : null}
            {action === PICO_IDENTITY_SWITCH_NIP46_ACTION ? (
              <label>Bunker URI<textarea value={bunkerUri} onChange={event => onEdit("bunkerUri", event.target.value)} placeholder="bunker://…" /></label>
            ) : null}
            {switching ? (
              <>
                <label>Local records<select value={disposition} onChange={event => onDisposition(event.target.value as SurfaceIdentityDisposition)}><option value="transfer">Transfer</option><option value="delete">Delete</option></select></label>
                <label className="pico-identity-check"><input type="checkbox" checked={confirmed} onChange={event => onConfirmed(event.target.checked)} /><span>I understand that peer pairings and stream-client trust will be cleared.</span></label>
                <p>Save files and save states stay under the plugin account.</p>
                <button
                  type="button"
                  className="danger"
                  disabled={working || !confirmed || (action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? !secret.trim() || !password.trim() : !bunkerUri.trim())}
                  onClick={onSubmit}
                >{working && status?._tag === "Working" ? status.operation : "SWITCH IDENTITY"}</button>
              </>
            ) : null}
            {deleteKey ? (
              <>
                <p>Delete this private key only after its encrypted backup is stored elsewhere.</p>
                <label className="pico-identity-check"><input type="checkbox" checked={confirmed} onChange={event => onConfirmed(event.target.checked)} /><span>I have stored a recovery backup.</span></label>
                <button type="button" className="danger" disabled={working || !confirmed} onClick={onSubmit}>DELETE KEY</button>
              </>
            ) : null}
          </>
        )}
      </div>
    </div>
  )
}
