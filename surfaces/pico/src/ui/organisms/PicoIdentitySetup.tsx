import "./PicoIdentitySetup.css"
import type { PicoIdentityView } from "../../pico-onboarding-view"
import type { IdentityOrigin } from "../../state/onboarding"
import { PicoBusyMeter } from "../atoms/PicoBusyMeter"
import { PicoRow } from "../atoms/PicoRow"
import { PicoChoiceRow } from "../molecules/PicoChoiceRow"
import { PicoSetupHeading } from "../molecules/PicoSetupHeading"

export interface PicoIdentityActions {
  readonly onChoose: (how: IdentityOrigin) => void
  readonly onEdit: (field: "backup" | "password" | "uri") => void
  readonly onSubmit: () => void
  readonly onBackUp: () => void
  readonly onContinue: () => void
}

function heading(view: PicoIdentityView): { readonly title: string; readonly message?: string } {
  switch (view._tag) {
    case "Choose":
      return view.offline
        ? { title: "Who are you?", message: "Your identity is the key that ties your Korri devices together. With no connection, only a new one can be made." }
        : { title: "Who are you?", message: "Your identity is the key that ties your Korri devices together." }
    case "Working":
      return { title: "One moment" }
    case "Ready":
      switch (view.origin) {
        case "created":
          return {
            title: "Identity created",
            message: view.canBackUp
              ? "Korri cannot recover a lost key. Back it up now, or later from Settings."
              : "Korri cannot recover a lost key. Back it up later from Settings.",
          }
        case "restored":
          return { title: "Identity restored", message: "This device uses the identity from your backup." }
        case "signer":
          return { title: "Signer connected", message: "Your signer keeps the key. Korri asks it each time it needs to sign." }
      }
      break
    case "Restore":
      return { title: "Restore from backup", message: "Type the encrypted backup and the password it was made with." }
    case "Signer":
      return { title: "Connect a signer", message: "A NIP-46 signer keeps your key on another device." }
    case "Approving":
      return { title: "Approve in your signer", message: "Your signer app shows a request from this device. Approve it there." }
  }
}

/**
 * Who this device belongs to: keep the identity Korri made for it, bring one
 * back from a backup, or hand the key to a NIP-46 signer. Drawn the way the
 * identity dialog draws identity work: solid ground, a title in the accent
 * over a floor rule, the facts in plain sentences, and plates under the
 * cursor. Unlike the dialog it types with Pico's keyboard (the page opens it
 * for a field), because a native field cannot be filled from a pad.
 *
 * With no connection, restoring and connecting a signer stay in the list,
 * disabled, saying why; a new identity still works. A refusal from Korri
 * shows in its words on a red plate, with the form kept.
 *
 * The backup itself is the real PicoIdentityDialog; the page opens it.
 */
export function PicoIdentitySetup({
  view,
  actions,
}: {
  readonly view: PicoIdentityView
  readonly actions: PicoIdentityActions
}) {
  const { title, message } = heading(view)
  const problem = view._tag === "Restore" || view._tag === "Signer" ? view.problem : undefined
  return (
    <section aria-label="Identity" className="pico-identity-setup">
      <PicoSetupHeading message={message} title={title} />
      {problem === undefined ? null : <p className="pico-identity-setup-problem">{problem}</p>}
      {view._tag === "Choose" ? (
        <ul className="pico-identity-setup-list">
          <PicoChoiceRow label="New identity" note="Korri made a key for you on this device." onPress={() => actions.onChoose("created")} />
          <PicoChoiceRow
            disabled={!view.restore.enabled}
            label="Restore from backup"
            note={view.restore.note}
            onPress={() => actions.onChoose("restored")}
          />
          <PicoChoiceRow
            disabled={!view.signer.enabled}
            label="Connect a signer"
            note={view.signer.note}
            onPress={() => actions.onChoose("signer")}
          />
        </ul>
      ) : null}
      {view._tag === "Working" ? (
        <div className="pico-identity-setup-busy">
          <PicoBusyMeter label={view.label} />
          <p className="pico-identity-setup-note">{view.label}</p>
        </div>
      ) : null}
      {view._tag === "Ready" ? (
        <div className="pico-identity-setup-actions">
          <PicoRow label="NEXT" onPress={actions.onContinue} />
          {view.canBackUp ? <PicoRow detail="▶" label="Back up now" onPress={actions.onBackUp} /> : null}
        </div>
      ) : null}
      {view._tag === "Restore" ? (
        <ul className="pico-identity-setup-list">
          <PicoChoiceRow detail={view.backup === "" ? "EMPTY" : view.backup} label="Backup" onPress={() => actions.onEdit("backup")} />
          <PicoChoiceRow detail={view.passwordLength === 0 ? "EMPTY" : "*".repeat(view.passwordLength)} label="Password" onPress={() => actions.onEdit("password")} />
          <li className="pico-identity-setup-submit">
            <PicoRow disabled={!view.ready} label="RESTORE" onPress={actions.onSubmit} />
          </li>
        </ul>
      ) : null}
      {view._tag === "Signer" ? (
        <ul className="pico-identity-setup-list">
          <PicoChoiceRow detail={view.uri === "" ? "EMPTY" : view.uri} label="Address" note="Your signer app shows a bunker:// address." onPress={() => actions.onEdit("uri")} />
          <li className="pico-identity-setup-submit">
            <PicoRow disabled={!view.ready} label="CONNECT" onPress={actions.onSubmit} />
          </li>
        </ul>
      ) : null}
      {view._tag === "Approving" ? (
        <div className="pico-identity-setup-busy">
          <PicoBusyMeter label="Waiting for your signer" />
          <p className="pico-identity-setup-note">{view.uri}</p>
        </div>
      ) : null}
    </section>
  )
}
