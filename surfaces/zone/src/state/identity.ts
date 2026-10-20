/**
 * The SAVE DATA page's model: which save card the cursor is on, and how far
 * the player is through one identity flow.
 *
 *     card   the focused card in the band: BACK UP, SWITCH (FROM BACKUP),
 *            SWITCH (BUNKER), then one card per retired key
 *     flow   the step of the flow on top of the band, as a tagged union
 *
 * Korri's `SurfaceIdentityManagement` is not stored here. Korri owns it and
 * replaces it whole; `update` receives it beside this value, as navigation
 * receives the SurfaceModel. Its `status` is drawn straight from Korri: the
 * busy strip, the problem and switched dialogs and the backup's QR are all
 * Korri's facts, so this model never holds a copy that could go stale.
 *
 * Rules the treaty demands (the same Pico's identity dialog holds):
 *   - nothing is sent while Korri is Working;
 *   - nothing is sent from an empty entry;
 *   - a deletion is sent only after the backup is confirmed, and a switch
 *     only after the trust loss is confirmed: both flags are always true
 *     when a request carries them;
 *   - Korri's status is cleared only by the player (B, DISMISS, OK, or a
 *     new flow over a finished backup), never by a republished model.
 */
import type { SurfaceIdentityDisposition, SurfaceIdentityManagement } from "@contracts/surface/korri-surface"
import { ask, stay, type ZoneStep } from "./requests"

/** The keyboard's layers, as ZoneKeyboard names them. */
export type IdentityKeyboardLayer = "letters" | "shift" | "symbols" | "uri"

/** What is typed so far in one keyboard step. `focusedKey` is a key id as ZoneKeyboard names it. */
export interface IdentityDraft {
  readonly text: string
  readonly caret: number
  readonly layer: IdentityKeyboardLayer
  readonly focusedKey: string
}

/** Where the new identity comes from, carried through the disposition and trust steps. */
export type IdentitySource =
  | { readonly kind: "backup"; readonly encryptedSecret: string; readonly password: string }
  | { readonly kind: "bunker"; readonly bunkerUri: string }

/** What a password is typed for: a new backup of this key, or opening a backup to switch to. */
export type IdentityPasswordFor =
  | { readonly kind: "export" }
  | { readonly kind: "switch"; readonly encryptedSecret: string }

/** A two-button dialog's cursor: 0 is the left button, 1 the right. */
export type IdentityChoice = 0 | 1

export type IdentityFlow =
  | { readonly _tag: "idle" }
  /** The keyboard, secret: a password for a new backup or for the backup being opened. */
  | { readonly _tag: "password"; readonly for: IdentityPasswordFor; readonly draft: IdentityDraft }
  /** The keyboard: the encrypted secret of a backup to switch to. */
  | { readonly _tag: "secret"; readonly draft: IdentityDraft }
  /** The keyboard on its URI layer: a NIP-46 bunker URI. */
  | { readonly _tag: "bunker"; readonly draft: IdentityDraft }
  /** TRANSFER / DELETE: what becomes of this device's local records. */
  | { readonly _tag: "disposition"; readonly source: IdentitySource; readonly focus: IdentityChoice }
  /** NO / YES: other devices will stop trusting this key. */
  | {
      readonly _tag: "trust"
      readonly source: IdentitySource
      readonly disposition: SurfaceIdentityDisposition
      readonly focus: IdentityChoice
    }
  /** NO / YES: delete this retired key? */
  | { readonly _tag: "confirmDelete"; readonly publicKey: string; readonly focus: IdentityChoice }
  /** NO / YES: is it backed up? The only way to the deletion. */
  | { readonly _tag: "confirmBackedUp"; readonly publicKey: string; readonly focus: IdentityChoice }
  /**
   * A backup was asked for: Korri's Working, then its BackupReady, is drawn;
   * B closes it. `onSlab` is where the cursor rests: on the secret's slab,
   * which leaves the BACK UP card unmarked so its QR code can fill the well,
   * or back on the band.
   */
  | { readonly _tag: "showSecret"; readonly onSlab: boolean }

export interface IdentityModel {
  readonly card: number
  readonly flow: IdentityFlow
}

export const IDENTITY_START: IdentityModel = { card: 0, flow: { _tag: "idle" } }

/** The fixed cards, in band order. Retired keys follow them. */
export const IDENTITY_BACKUP_CARD = 0
export const IDENTITY_SWITCH_BACKUP_CARD = 1
export const IDENTITY_SWITCH_BUNKER_CARD = 2
export const IDENTITY_FIXED_CARDS = 3

/** Every NIP-46 bunker URI starts with this; the bunker step opens with it typed. */
export const IDENTITY_BUNKER_PREFIX = "bunker://"

export type IdentityMessage =
  /** The cursor is on a save card. */
  | { readonly _tag: "FocusedSaveCard"; readonly index: number }
  /** The cursor is on the backup's slab. */
  | { readonly _tag: "FocusedSecret" }
  /** A on a save card. */
  | { readonly _tag: "ChoseSaveCard"; readonly index: number }
  /** The cursor is on a key. */
  | { readonly _tag: "FocusedKey"; readonly key: string }
  | { readonly _tag: "TypedChar"; readonly char: string }
  | { readonly _tag: "TypedSpace" }
  | { readonly _tag: "DeletedChar" }
  | { readonly _tag: "SwitchedLayer"; readonly layer: IdentityKeyboardLayer }
  /** DONE on the keyboard. */
  | { readonly _tag: "PressedDone" }
  /** The cursor is on a dialog button. */
  | { readonly _tag: "FocusedChoice"; readonly index: IdentityChoice }
  /** TRANSFER or DELETE. */
  | { readonly _tag: "ChoseDisposition"; readonly disposition: SurfaceIdentityDisposition }
  /** NO or YES on a trust or delete dialog. */
  | { readonly _tag: "Answered"; readonly yes: boolean }
  /** DISMISS on Korri's problem, or OK on its switched dialog. */
  | { readonly _tag: "DismissedStatus" }
  /** The host's back button. */
  | { readonly _tag: "PressedBack" }
  /** Korri published a new model: the band and the flow are settled against it. */
  | { readonly _tag: "KorriPublished" }

/** Cards in the band: one NOT AVAILABLE card when Korri cannot manage identity here. */
export function identityCardCount(identity: SurfaceIdentityManagement | undefined): number {
  return identity === undefined ? 1 : IDENTITY_FIXED_CARDS + identity.retiredPublicKeys.length
}

/** The retired key a card stands for, if it stands for one. */
export function retiredKeyAt(identity: SurfaceIdentityManagement | undefined, index: number): string | undefined {
  if (identity === undefined || index < IDENTITY_FIXED_CARDS) return undefined
  return identity.retiredPublicKeys[index - IDENTITY_FIXED_CARDS]
}

const draftOf = (text: string, layer: IdentityKeyboardLayer, focusedKey: string): IdentityDraft =>
  ({ text, caret: text.length, layer, focusedKey })

const IDLE: IdentityFlow = { _tag: "idle" }

/** A backup secret is bech32: whitespace in it is a typing slip, never part of it. */
const cleanSecret = (text: string): string => text.replace(/\s+/g, "")

export function update(
  model: IdentityModel,
  message: IdentityMessage,
  identity: SurfaceIdentityManagement | undefined,
): ZoneStep<IdentityModel> {
  const result = step(model, message, identity)
  return { model: settle(result.model, identity), requests: result.requests }
}

function step(
  model: IdentityModel,
  message: IdentityMessage,
  identity: SurfaceIdentityManagement | undefined,
): ZoneStep<IdentityModel> {
  const status = identity?.status._tag ?? "Idle"
  const { flow } = model
  switch (message._tag) {
    case "KorriPublished":
      return stay(model)

    case "FocusedSaveCard":
      if (flow._tag === "showSecret") {
        return stay(model.card === message.index && !flow.onSlab
          ? model
          : { card: message.index, flow: { _tag: "showSecret", onSlab: false } })
      }
      return flow._tag === "idle"
        ? stay(model.card === message.index ? model : { ...model, card: message.index })
        : stay(model)

    case "FocusedSecret":
      // Korri's backup can be on screen with no flow behind it (the page
      // was opened over it): the cursor on its slab is the same fact.
      return (flow._tag === "showSecret" && !flow.onSlab) || (flow._tag === "idle" && status === "BackupReady")
        ? stay({ ...model, flow: { _tag: "showSecret", onSlab: true } })
        : stay(model)

    case "ChoseSaveCard":
      return chose(model, message.index, identity)

    case "FocusedKey":
      return editDraft(model, draft => draft.focusedKey === message.key ? draft : { ...draft, focusedKey: message.key })

    case "TypedChar":
      return editDraft(model, draft => insert(draft, message.char, message.char))

    case "TypedSpace":
      return editDraft(model, draft => insert(draft, " ", "space"))

    case "DeletedChar":
      return editDraft(model, draft => draft.caret === 0 ? { ...draft, focusedKey: "delete" } : {
        ...draft,
        text: draft.text.slice(0, draft.caret - 1) + draft.text.slice(draft.caret),
        caret: draft.caret - 1,
        focusedKey: "delete",
      })

    case "SwitchedLayer":
      return editDraft(model, draft => ({ ...draft, layer: message.layer, focusedKey: `layer:${message.layer}` }))

    case "PressedDone":
      return done(model, status)

    case "FocusedChoice":
      switch (flow._tag) {
        case "disposition":
        case "trust":
        case "confirmDelete":
        case "confirmBackedUp":
          return stay(flow.focus === message.index ? model : { ...model, flow: { ...flow, focus: message.index } })
        default:
          return stay(model)
      }

    case "ChoseDisposition":
      return flow._tag === "disposition"
        ? stay({ ...model, flow: { _tag: "trust", source: flow.source, disposition: message.disposition, focus: 0 } })
        : stay(model)

    case "Answered":
      return answered(model, message.yes, status)

    case "DismissedStatus":
      return dismissed(model, status)

    case "PressedBack":
      return back(model, status)
  }
}

/** A on a card starts its flow. Nothing starts under Korri's work or its dialogs. */
function chose(
  model: IdentityModel,
  index: number,
  identity: SurfaceIdentityManagement | undefined,
): ZoneStep<IdentityModel> {
  if (identity === undefined) return stay(model)
  const status = identity.status._tag
  if (status === "Working" || status === "Problem" || status === "Switched") return stay(model)
  if (model.flow._tag !== "idle" && model.flow._tag !== "showSecret") return stay(model)

  let flow: IdentityFlow
  if (index === IDENTITY_BACKUP_CARD) {
    flow = { _tag: "password", for: { kind: "export" }, draft: draftOf("", "letters", "q") }
  } else if (index === IDENTITY_SWITCH_BACKUP_CARD) {
    flow = { _tag: "secret", draft: draftOf("", "letters", "q") }
  } else if (index === IDENTITY_SWITCH_BUNKER_CARD) {
    flow = { _tag: "bunker", draft: draftOf(IDENTITY_BUNKER_PREFIX, "uri", "1") }
  } else {
    const publicKey = retiredKeyAt(identity, index)
    if (publicKey === undefined) return stay(model)
    flow = { _tag: "confirmDelete", publicKey, focus: 0 }
  }
  const next: IdentityModel = { card: index, flow }
  // A finished backup is still on screen: starting something else closes it.
  return status === "BackupReady" ? ask(next, { _tag: "DismissIdentityStatus" }) : stay(next)
}

function insert(draft: IdentityDraft, char: string, focusedKey: string): IdentityDraft {
  return {
    ...draft,
    text: draft.text.slice(0, draft.caret) + char + draft.text.slice(draft.caret),
    caret: draft.caret + char.length,
    focusedKey,
  }
}

function editDraft(model: IdentityModel, edit: (draft: IdentityDraft) => IdentityDraft): ZoneStep<IdentityModel> {
  const { flow } = model
  switch (flow._tag) {
    case "password":
    case "secret":
    case "bunker": {
      const draft = edit(flow.draft)
      return stay(draft === flow.draft ? model : { ...model, flow: { ...flow, draft } })
    }
    default:
      return stay(model)
  }
}

/** DONE: the keyboard step hands its text to the next step. An empty entry goes nowhere. */
function done(model: IdentityModel, status: SurfaceIdentityManagement["status"]["_tag"]): ZoneStep<IdentityModel> {
  const { flow } = model
  switch (flow._tag) {
    case "password": {
      const password = flow.draft.text
      if (password.trim() === "") return stay(model)
      if (flow.for.kind === "export") {
        if (status === "Working") return stay(model)
        return ask({ ...model, flow: { _tag: "showSecret", onSlab: false } }, { _tag: "ExportIdentityBackup", password })
      }
      return stay({
        ...model,
        flow: { _tag: "disposition", source: { kind: "backup", encryptedSecret: flow.for.encryptedSecret, password }, focus: 0 },
      })
    }
    case "secret": {
      const encryptedSecret = cleanSecret(flow.draft.text)
      if (encryptedSecret === "") return stay(model)
      return stay({
        ...model,
        flow: { _tag: "password", for: { kind: "switch", encryptedSecret }, draft: draftOf("", "letters", "q") },
      })
    }
    case "bunker": {
      const bunkerUri = flow.draft.text.trim()
      if (bunkerUri === "" || bunkerUri === IDENTITY_BUNKER_PREFIX) return stay(model)
      return stay({ ...model, flow: { _tag: "disposition", source: { kind: "bunker", bunkerUri }, focus: 0 } })
    }
    default:
      return stay(model)
  }
}

/** NO / YES. NO always abandons the flow; YES takes the next step or sends. */
function answered(
  model: IdentityModel,
  yes: boolean,
  status: SurfaceIdentityManagement["status"]["_tag"],
): ZoneStep<IdentityModel> {
  const { flow } = model
  switch (flow._tag) {
    case "trust": {
      if (!yes) return stay({ ...model, flow: IDLE })
      if (status === "Working") return stay(model)
      const idle = { ...model, flow: IDLE }
      return flow.source.kind === "backup"
        ? ask(idle, {
            _tag: "SwitchIdentityFromBackup",
            encryptedSecret: flow.source.encryptedSecret,
            password: flow.source.password,
            disposition: flow.disposition,
            trustLossConfirmed: true,
          })
        : ask(idle, {
            _tag: "SwitchIdentityToNip46",
            bunkerUri: flow.source.bunkerUri,
            disposition: flow.disposition,
            trustLossConfirmed: true,
          })
    }
    case "confirmDelete":
      return stay({ ...model, flow: yes ? { _tag: "confirmBackedUp", publicKey: flow.publicKey, focus: 0 } : IDLE })
    case "confirmBackedUp": {
      if (!yes) return stay({ ...model, flow: IDLE })
      if (status === "Working") return stay(model)
      return ask({ ...model, flow: IDLE }, { _tag: "DeleteRetiredIdentity", publicKey: flow.publicKey, backupConfirmed: true })
    }
    default:
      return stay(model)
  }
}

/** DISMISS or OK: Korri's finished status is cleared. A flow waiting on it ends. */
function dismissed(model: IdentityModel, status: SurfaceIdentityManagement["status"]["_tag"]): ZoneStep<IdentityModel> {
  if (status === "Idle" || status === "Working") return stay(model)
  const next = model.flow._tag === "showSecret" ? { ...model, flow: IDLE } : model
  return ask(next, { _tag: "DismissIdentityStatus" })
}

/**
 * B. Korri's dialogs and a finished backup close first, as DISMISS does.
 * Otherwise one step back: a keyboard step returns to the one before it
 * with its text kept, a dialog to the step that opened it.
 */
function back(model: IdentityModel, status: SurfaceIdentityManagement["status"]["_tag"]): ZoneStep<IdentityModel> {
  if (status === "Problem" || status === "Switched" || status === "BackupReady") return dismissed(model, status)
  const { flow } = model
  switch (flow._tag) {
    case "idle":
      return stay(model)
    case "showSecret":
    case "secret":
    case "bunker":
    case "confirmDelete":
      return stay({ ...model, flow: IDLE })
    case "password":
      return stay({
        ...model,
        flow: flow.for.kind === "export"
          ? IDLE
          : { _tag: "secret", draft: draftOf(flow.for.encryptedSecret, "letters", "done") },
      })
    case "disposition":
      return stay({
        ...model,
        flow: flow.source.kind === "backup"
          ? {
              _tag: "password",
              for: { kind: "switch", encryptedSecret: flow.source.encryptedSecret },
              draft: draftOf(flow.source.password, "letters", "done"),
            }
          : { _tag: "bunker", draft: draftOf(flow.source.bunkerUri, "uri", "done") },
      })
    case "trust":
      return stay({
        ...model,
        flow: { _tag: "disposition", source: flow.source, focus: flow.disposition === "transfer" ? 0 : 1 },
      })
    case "confirmBackedUp":
      return stay({ ...model, flow: { _tag: "confirmDelete", publicKey: flow.publicKey, focus: 0 } })
  }
}

/**
 * Korri's facts win. Without identity management the page is one card and
 * no flow; the cursor never points past the last card; a deletion whose key
 * Korri no longer lists ends.
 */
function settle(model: IdentityModel, identity: SurfaceIdentityManagement | undefined): IdentityModel {
  if (identity === undefined) {
    return model.card === 0 && model.flow._tag === "idle" ? model : IDENTITY_START
  }
  const last = identityCardCount(identity) - 1
  const card = Math.max(0, Math.min(model.card, last))
  const { flow } = model
  const gone = (flow._tag === "confirmDelete" || flow._tag === "confirmBackedUp")
    && !identity.retiredPublicKeys.includes(flow.publicKey)
  if (card === model.card && !gone) return model
  return { card, flow: gone ? IDLE : flow }
}
