/**
 * The SAVE DATA page's model: which save card the cursor is on, and how far
 * the player is through the identity flow.
 *
 *     card   the focused card in the band: SWITCH (BUNKER)
 *     flow   the step of the flow on top of the band, as a tagged union
 *
 * Korri's `SurfaceIdentityManagement` is not stored here. Korri owns it and
 * replaces it whole; `update` receives it beside this value, as navigation
 * receives the SurfaceModel. Its `status` is drawn straight from Korri: the
 * busy strip and the problem and switched dialogs are Korri's facts, so this
 * model never holds a copy that could go stale.
 *
 * Rules the treaty demands (the same Pico's identity dialog holds):
 *   - nothing is sent while Korri is Working;
 *   - nothing is sent from an empty entry;
 *   - a switch is sent only after the trust loss is confirmed: the flag is
 *     always true when a request carries it;
 *   - Korri's status is cleared only by the player (B, DISMISS or OK),
 *     never by a republished model.
 */
import type { SurfaceIdentityDisposition, SurfaceIdentityManagement } from "../../../../../../contracts/generated/surface"
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
export type IdentitySource = { readonly kind: "bunker"; readonly bunkerUri: string }

/** A two-button dialog's cursor: 0 is the left button, 1 the right. */
export type IdentityChoice = 0 | 1

export type IdentityFlow =
  | { readonly _tag: "idle" }
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

export interface IdentityModel {
  readonly card: number
  readonly flow: IdentityFlow
}

export const IDENTITY_START: IdentityModel = { card: 0, flow: { _tag: "idle" } }

/** The cards, in band order. */
export const IDENTITY_SWITCH_BUNKER_CARD = 0
export const IDENTITY_CARDS = 1

/** Every NIP-46 bunker URI starts with this; the bunker step opens with it typed. */
export const IDENTITY_BUNKER_PREFIX = "bunker://"

export type IdentityMessage =
  /** The cursor is on a save card. */
  | { readonly _tag: "FocusedSaveCard"; readonly index: number }
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
  /** NO or YES on the trust dialog. */
  | { readonly _tag: "Answered"; readonly yes: boolean }
  /** DISMISS on Korri's problem, or OK on its switched dialog. */
  | { readonly _tag: "DismissedStatus" }
  /** The host's back button. */
  | { readonly _tag: "PressedBack" }
  /** Korri published a new model: the band and the flow are settled against it. */
  | { readonly _tag: "KorriPublished" }

/** Cards in the band: one NOT AVAILABLE card when Korri cannot manage identity here. */
export function identityCardCount(identity: SurfaceIdentityManagement | undefined): number {
  return identity === undefined ? 1 : IDENTITY_CARDS
}

const draftOf = (text: string, layer: IdentityKeyboardLayer, focusedKey: string): IdentityDraft =>
  ({ text, caret: text.length, layer, focusedKey })

const IDLE: IdentityFlow = { _tag: "idle" }

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
      return flow._tag === "idle"
        ? stay(model.card === message.index ? model : { ...model, card: message.index })
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
      return done(model)

    case "FocusedChoice":
      switch (flow._tag) {
        case "disposition":
        case "trust":
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
  if (model.flow._tag !== "idle" || index !== IDENTITY_SWITCH_BUNKER_CARD) return stay(model)
  return stay({ card: index, flow: { _tag: "bunker", draft: draftOf(IDENTITY_BUNKER_PREFIX, "uri", "1") } })
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
  if (flow._tag !== "bunker") return stay(model)
  const draft = edit(flow.draft)
  return stay(draft === flow.draft ? model : { ...model, flow: { ...flow, draft } })
}

/** DONE: the bunker URI goes to the disposition step. An empty entry goes nowhere. */
function done(model: IdentityModel): ZoneStep<IdentityModel> {
  const { flow } = model
  if (flow._tag !== "bunker") return stay(model)
  const bunkerUri = flow.draft.text.trim()
  if (bunkerUri === "" || bunkerUri === IDENTITY_BUNKER_PREFIX) return stay(model)
  return stay({ ...model, flow: { _tag: "disposition", source: { kind: "bunker", bunkerUri }, focus: 0 } })
}

/** NO / YES on the trust dialog. NO abandons the flow; YES sends. */
function answered(
  model: IdentityModel,
  yes: boolean,
  status: SurfaceIdentityManagement["status"]["_tag"],
): ZoneStep<IdentityModel> {
  const { flow } = model
  if (flow._tag !== "trust") return stay(model)
  if (!yes) return stay({ ...model, flow: IDLE })
  if (status === "Working") return stay(model)
  return ask({ ...model, flow: IDLE }, {
    _tag: "SwitchIdentityToNip46",
    bunkerUri: flow.source.bunkerUri,
    disposition: flow.disposition,
    trustLossConfirmed: true,
  })
}

/** DISMISS or OK: Korri's finished status is cleared. */
function dismissed(model: IdentityModel, status: SurfaceIdentityManagement["status"]["_tag"]): ZoneStep<IdentityModel> {
  if (status === "Idle" || status === "Working") return stay(model)
  return ask(model, { _tag: "DismissIdentityStatus" })
}

/**
 * B. Korri's dialogs close first, as DISMISS does. Otherwise one step back:
 * the keyboard returns to the band, a dialog to the step that opened it.
 */
function back(model: IdentityModel, status: SurfaceIdentityManagement["status"]["_tag"]): ZoneStep<IdentityModel> {
  if (status === "Problem" || status === "Switched") return dismissed(model, status)
  const { flow } = model
  switch (flow._tag) {
    case "idle":
      return stay(model)
    case "bunker":
      return stay({ ...model, flow: IDLE })
    case "disposition":
      return stay({ ...model, flow: { _tag: "bunker", draft: draftOf(flow.source.bunkerUri, "uri", "done") } })
    case "trust":
      return stay({
        ...model,
        flow: { _tag: "disposition", source: flow.source, focus: flow.disposition === "transfer" ? 0 : 1 },
      })
  }
}

/**
 * Korri's facts win. Without identity management the page is one card and
 * no flow; the cursor never points past the last card.
 */
function settle(model: IdentityModel, identity: SurfaceIdentityManagement | undefined): IdentityModel {
  if (identity === undefined) {
    return model.card === 0 && model.flow._tag === "idle" ? model : IDENTITY_START
  }
  const card = Math.max(0, Math.min(model.card, identityCardCount(identity) - 1))
  return card === model.card ? model : { ...model, card }
}
