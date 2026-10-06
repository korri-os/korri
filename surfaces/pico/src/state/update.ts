/**
 * The one function that changes Pico's catalog navigation.
 *
 * Pure: (navigation, message, Korri's model) -> (navigation, requests). Every
 * navigation rule lives here, and a test can drive each one with plain values.
 */
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { isPicoFont } from "../pico-font-preference"
import { picoScreenViewFromModel } from "../pico-screen-view"
import { picoSessionReturnFromModel, picoSessionReturnOnPlay } from "../pico-session-return"
import {
  PICO_FONT_SETTING,
  PICO_IDENTITY_BACKUP_ACTION,
  PICO_IDENTITY_SWITCH_LOCAL_ACTION,
  PICO_IDENTITY_SWITCH_NIP46_ACTION,
  picoIdentityRetiredKey,
  picoSettingsViewFromModel,
  picoTextSettingFrom,
  type PicoTextSettingRowView,
} from "../pico-settings-view"
import { picoDraftBackspace, picoDraftType, picoTextDraft, type PicoTextDraft } from "../pico-text-draft"
import { isHostButton, type PicoMessage } from "./messages"
import {
  type Detail,
  type Home,
  IDENTITY_FORM,
  type IdentityForm,
  type PicoNavigation,
  SETTINGS,
  type Settings,
  type SettingsQuestion,
  type TextEditor,
  topLayer,
} from "./navigation"
import { ask, type PicoRequest, type PicoStep, stay } from "./requests"
import { attractShowing, canAttract, identityAction, runnerOpen } from "./shown"

export function update(nav: PicoNavigation, message: PicoMessage, korri: SurfaceModel): PicoStep<PicoNavigation> {
  const result = step(nav, message, korri)
  const settled = settleIdle(settleEditor(settleSession(result.model, korri), korri), korri)
  const identity = settleIdentity(nav, settled, korri)
  return { model: identity.model, requests: [...result.requests, ...identity.requests] }
}

function step(nav: PicoNavigation, message: PicoMessage, korri: SurfaceModel): PicoStep<PicoNavigation> {
  // The runner picker covers everything Pico draws. Back cancels it; the other
  // host buttons wait for it to close.
  if (runnerOpen(korri) && isHostButton(message)) {
    return message._tag === "PressedBack" ? ask(nav, { _tag: "RunAction", actionId: "runner:cancel" }) : stay(nav)
  }

  // A press while attract shows wakes the screen and does nothing else.
  // Picking a device up must not start a game, cycle a layout or open
  // Settings: the first press is how the player gets the screen back.
  if (isHostButton(message) || message._tag === "PressedScreen" || message._tag === "MovedFocus") {
    const waking = message._tag !== "MovedFocus" && attractShowing(nav, korri)
    const activity = nav.idle.activity + 1
    nav = { ...nav, idle: { _tag: message._tag === "MovedFocus" ? nav.idle._tag : "Awake", activity } }
    if (waking || !isHostButton(message)) return stay(nav)
  }

  // Input acts on what is shown. While Korri shows a launch, a running game
  // or a failure, the host buttons do not change the screens hidden below it.
  // Back acknowledges a failure.
  if (isHostButton(message) && korri.status._tag !== "Browsing") {
    return message._tag === "PressedBack" && korri.status._tag === "Problem"
      ? ask(nav, { _tag: "DismissProblem" })
      : stay(nav)
  }

  switch (message._tag) {
    case "PressedBack":
      return stay(back(nav))

    case "PressedSystem":
      // Settings toggles. Closing it drops its question, identity dialog included.
      return stay(nav.settings !== undefined
        ? { ...nav, settings: undefined }
        : { ...nav, settings: SETTINGS, home: closeMenu(nav.home) })

    case "PressedOptions": {
      // Find opens from home and closes from Find. Over any other screen the
      // toggle would change only what is hidden below it.
      const top = topLayer(nav)
      if (top === "Find") return stay({ ...nav, find: false })
      if (top === "Home") return stay({ ...nav, find: true, home: closeMenu(nav.home) })
      return stay(nav)
    }

    case "PressedMenu":
      // The layout belongs to home. Cycling it under another screen changes nothing the player sees.
      return stay(topLayer(nav) === "Home" ? cycleMode(nav) : nav)

    case "KorriPublished":
      // settleSession and settleIdle, after every message, do the work.
      return stay(nav)

    case "IdleElapsed":
      // A timer that fired late cannot start attract over a screen that no longer allows it.
      return stay(nav.idle._tag === "Awake" && canAttract(nav, korri)
        ? { ...nav, idle: { _tag: "Attracting", activity: nav.idle.activity } }
        : nav)

    case "ChoseRunnerAction":
      return ask(nav, { _tag: "RunAction", actionId: message.actionId })

    // Home
    case "ToggledHomeMenu":
      return stay({ ...nav, home: { ...nav.home, menu: nav.home.menu === "Open" ? "Closed" : "Open" } })
    case "ChoseMenuFind":
      return stay({ ...nav, find: true, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } })
    case "ChoseMenuSettings":
      return stay({ ...nav, settings: SETTINGS, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } })
    case "ChoseMenuView":
      return stay(cycleMode(nav))
    case "AimedMenu":
      return stay(nav.home.aim === message.label ? nav : { ...nav, home: { ...nav.home, aim: message.label } })
    case "ReturnedFocus":
      return stay({ ...nav, home: { ...nav.home, focusOnReturn: "None" } })
    case "SelectedCart":
      return stay({ ...nav, home: { ...nav.home, selectedGameId: message.gameId } })
    case "OpenedGame": {
      const detail: Detail = { gameId: message.gameId, question: { _tag: "None" } }
      // From the shelf, Back should put the cursor on this cart again.
      return stay(topLayer(nav) === "Home"
        ? { ...nav, detail, home: { ...closeMenu(nav.home), focusOnReturn: nav.home.mode === "shelf" ? "Cart" : "None" } }
        : { ...nav, detail })
    }
    case "PressedRetry":
      return ask(nav, picoScreenViewFromModel(korri)._tag === "Problem" ? { _tag: "Retry" } : { _tag: "Reload" })
    case "PressedDismiss":
      return ask(nav, { _tag: "DismissProblem" })
    case "PressedStatusAction":
      // Only what Korri publishes now, enabled, is authority. A press that
      // lands after the work ended or the action went inert sends nothing.
      return korri.status._tag === "Busy" &&
        (korri.status.actions ?? []).some(action => action.id === message.actionId && action.enabled)
        ? ask(nav, { _tag: "RunAction", actionId: message.actionId })
        : stay(nav)

    // Find
    case "TypedCharacter":
      return stay({ ...nav, search: { ...nav.search, query: nav.search.query + message.character } })
    case "PressedBackspace":
      return stay({ ...nav, search: { ...nav.search, query: nav.search.query.slice(0, -1) } })
    case "ClearedQuery":
      return stay({ ...nav, search: { ...nav.search, query: "" } })
    case "ChoseSection":
      return stay({ ...nav, search: { ...nav.search, section: message.section } })
    case "ChoseOrder":
      return stay({ ...nav, search: { ...nav.search, order: message.order } })

    // A game's own screen
    case "PressedPlay": {
      const detail = nav.detail
      if (detail === undefined) return stay(nav)
      const view = picoScreenViewFromModel(korri)
      const game = view._tag === "Shelf" ? view.games.find(candidate => candidate.id === detail.gameId) : undefined
      if (game === undefined) return stay(nav)
      // Korri offers more than one place to play: ask, never pick for the player.
      if (game.locations !== undefined && game.locations.length > 0) {
        return stay({ ...nav, detail: { ...detail, question: { _tag: "ChoosingLocation", game } } })
      }
      return ask(
        { ...nav, session: picoSessionReturnOnPlay(nav.session, game.id, korri.catalog) },
        { _tag: "LaunchGame", gameId: game.id },
      )
    }
    case "ChoseLocation": {
      const detail = nav.detail
      if (detail?.question._tag !== "ChoosingLocation") return stay(nav)
      const gameId = detail.question.game.id
      return ask(
        {
          ...nav,
          detail: { ...detail, question: { _tag: "None" } },
          session: picoSessionReturnOnPlay(nav.session, gameId, korri.catalog),
        },
        { _tag: "LaunchGame", gameId, locationId: message.locationId },
      )
    }
    case "PressedGameAction": {
      const detail = nav.detail
      if (detail === undefined) return stay(nav)
      return message.action.destructive === true
        ? stay({ ...nav, detail: { ...detail, question: { _tag: "ConfirmingAction", action: message.action } } })
        : ask(nav, { _tag: "RunGameAction", gameId: detail.gameId, actionId: message.action.id })
    }
    case "ConfirmedGameAction": {
      const detail = nav.detail
      if (detail?.question._tag !== "ConfirmingAction") return stay(nav)
      return ask(
        { ...nav, detail: { ...detail, question: { _tag: "None" } } },
        { _tag: "RunGameAction", gameId: detail.gameId, actionId: detail.question.action.id },
      )
    }
    case "CancelledGameAction":
      return stay(nav.detail === undefined ? nav : { ...nav, detail: { ...nav.detail, question: { _tag: "None" } } })

    // Settings
    case "ChoseSettingsGroup":
      return stay(nav.settings === undefined ? nav : { ...nav, settings: { ...nav.settings, group: message.group } })
    case "PressedSettingAction":
      return message.actionId.startsWith("identity:")
        ? stay(withQuestion(nav, { _tag: "Identity", actionId: message.actionId, form: IDENTITY_FORM, qr: undefined }))
        : ask(nav, { _tag: "RunAction", actionId: message.actionId })
    case "AskedSettingConfirmation":
      return stay(withQuestion(nav, { _tag: "ConfirmingAction", actionId: message.actionId, confirmation: message.confirmation }))
    case "ConfirmedSettingAction": {
      const question = nav.settings?.question
      if (question?._tag !== "ConfirmingAction") return stay(nav)
      return ask(withQuestion(nav, { _tag: "None" }), { _tag: "RunAction", actionId: question.actionId })
    }
    case "CancelledSettingAction":
    case "ClosedEditor":
    case "ClosedIdentity":
      return stay(withQuestion(nav, { _tag: "None" }))
    case "ChangedSetting":
      // Pico's own row: Pico keeps it, and Korri is never asked.
      if (message.settingId === PICO_FONT_SETTING) {
        return isPicoFont(message.value) ? ask(nav, { _tag: "ChooseFont", font: message.value }) : stay(nav)
      }
      return ask(nav, { _tag: "ChangeSetting", settingId: message.settingId, value: message.value })
    case "DismissedSettingsProblem":
      return ask(nav, { _tag: "DismissSettingsProblem" })

    // The text editor. Text starts from the value Korri published; a secret
    // starts empty. Typing is the editor's alone: a republished model keeps it.
    case "OpenedEditor": {
      const row = textRow(korri, message.settingId)
      if (row === undefined) return stay(nav)
      const draft = picoTextDraft(row.control.sensitive ? "" : (row.value ?? ""))
      return stay(withQuestion(nav, { _tag: "EditingText", settingId: row.id, draft, sawSaving: false }))
    }
    case "AskedClear": {
      const editor = openEditor(nav)
      return editor?._tag === "EditingText" && !saving(korri, editor)
        ? stay(withQuestion(nav, { ...editor, _tag: "ConfirmingClear" }))
        : stay(nav)
    }
    case "CancelledClear": {
      const editor = openEditor(nav)
      return editor?._tag === "ConfirmingClear" ? stay(withQuestion(nav, { ...editor, _tag: "EditingText" })) : stay(nav)
    }
    case "ConfirmedClear": {
      const editor = openEditor(nav)
      if (editor?._tag !== "ConfirmingClear") return stay(nav)
      return ask(withQuestion(nav, { ...editor, _tag: "EditingText" }), { _tag: "ChangeSetting", settingId: editor.settingId, value: "" })
    }
    case "TypedText":
      return stay(editDraft(nav, korri, (draft, row) => picoDraftType(draft, message.character, row.control.maxLength)))
    case "PressedTextBackspace":
      return stay(editDraft(nav, korri, draft => picoDraftBackspace(draft)))
    case "ClearedText":
      return stay(editDraft(nav, korri, draft => ({ ...draft, text: "" })))
    case "ToggledCapitals":
      return stay(editDraft(nav, korri, draft => ({ ...draft, capitals: !draft.capitals })))
    case "ToggledSymbols":
      return stay(editDraft(nav, korri, draft => ({ ...draft, symbols: !draft.symbols })))
    case "SavedText": {
      // Save sends exactly what was typed, never an empty value: emptying a
      // secret is the clear action's job.
      const editor = openEditor(nav)
      if (editor?._tag !== "EditingText" || saving(korri, editor) || editor.draft.text.trim() === "") return stay(nav)
      return ask(nav, { _tag: "ChangeSetting", settingId: editor.settingId, value: editor.draft.text })
    }

    // The identity dialog
    case "EditedIdentity":
      return stay(editIdentity(nav, form => ({ ...form, [message.field]: message.value })))
    case "ChoseIdentityDisposition":
      return stay(editIdentity(nav, form => ({ ...form, disposition: message.disposition })))
    case "CheckedIdentityConfirmation":
      return stay(editIdentity(nav, form => ({ ...form, confirmed: message.confirmed })))
    case "SubmittedIdentity": {
      const question = nav.settings?.question
      if (question?._tag !== "Identity" || korri.identityManagement?.status._tag === "Working") return stay(nav)
      const request = identityRequest(question.actionId, question.form)
      return request === undefined ? stay(nav) : ask(nav, request)
    }
    case "RenderedQr": {
      const question = nav.settings?.question
      return question?._tag === "Identity" && question.qr?.text === message.text
        ? stay(withQuestion(nav, { ...question, qr: { text: message.text, dataUrl: message.dataUrl } }))
        : stay(nav)
    }
    case "Waited":
      return stay(nav)
  }
}

/**
 * Back while browsing withdraws the top layer's question, else closes the top
 * layer. Only the top layer: it never touches what the player cannot see.
 * With nothing left to close, leaving the surface is the host's decision.
 */
function back(nav: PicoNavigation): PicoNavigation {
  switch (topLayer(nav)) {
    case "Settings":
      return nav.settings === undefined ? nav : backInSettings(nav, nav.settings)
    case "Detail":
      if (nav.detail === undefined) return nav
      return nav.detail.question._tag !== "None"
        ? { ...nav, detail: { ...nav.detail, question: { _tag: "None" } } }
        : { ...nav, detail: undefined }
    case "Find":
      return { ...nav, find: false }
    case "Home":
      return nav.home.menu === "Open" ? { ...nav, home: closeMenu(nav.home) } : nav
  }
}

/** The clearing question, then the editor, then any other question, then Settings. */
function backInSettings(nav: PicoNavigation, settings: Settings): PicoNavigation {
  const question = settings.question
  switch (question._tag) {
    case "ConfirmingClear":
      return withQuestion(nav, { ...question, _tag: "EditingText" })
    case "ConfirmingAction":
    case "EditingText":
    case "Identity":
      return withQuestion(nav, { _tag: "None" })
    case "None":
      return { ...nav, settings: undefined }
  }
}

/**
 * When a game played from its own screen ends, the player returns to the
 * shelf, with every screen and question closed (pico-session-return.ts says
 * what counts as an end).
 */
function settleSession(nav: PicoNavigation, korri: SurfaceModel): PicoNavigation {
  const session = picoSessionReturnFromModel(nav.session, nav.detail?.gameId, korri)
  // Keep the same value when nothing changed, so React has nothing to redraw.
  if (session === nav.session || (session._tag === "Idle" && nav.session._tag === "Idle")) return nav
  if (session._tag !== "ReturnToLibrary") return { ...nav, session }
  return {
    ...nav,
    home: { ...nav.home, mode: "shelf", menu: "Closed", focusOnReturn: "None" },
    find: false,
    detail: undefined,
    settings: undefined,
    session: { _tag: "Idle" },
  }
}

/**
 * Korri reports a save. The editor closes when the row it saw saving is idle
 * again; a refusal keeps it open with the typing kept, so a second try does
 * not start over.
 */
function settleEditor(nav: PicoNavigation, korri: SurfaceModel): PicoNavigation {
  const editor = openEditor(nav)
  if (editor === undefined) return nav
  const state = textRow(korri, editor.settingId)?.state
  if (state === "saving") return editor.sawSaving ? nav : withQuestion(nav, { ...editor, sawSaving: true })
  if (!editor.sawSaving) return nav
  return withQuestion(nav, state === "idle" ? { _tag: "None" } : { ...editor, sawSaving: false })
}

/**
 * The identity dialog. Opening, changing or closing it clears the status Korri
 * shows for the last identity operation, so an old result never greets a new
 * one. A backup Korri made is drawn as a QR code.
 */
function settleIdentity(before: PicoNavigation, nav: PicoNavigation, korri: SurfaceModel): PicoStep<PicoNavigation> {
  const requests: PicoRequest[] = []
  if (identityAction(before) !== identityAction(nav)) requests.push({ _tag: "DismissIdentityStatus" })
  const question = nav.settings?.question
  if (question?._tag !== "Identity") return { model: nav, requests }
  const status = korri.identityManagement?.status
  const text = status?._tag === "BackupReady" ? status.encryptedSecret : undefined
  if (text === question.qr?.text) return { model: nav, requests }
  if (text === undefined) return { model: withQuestion(nav, { ...question, qr: undefined }), requests }
  requests.push({ _tag: "RenderQr", text })
  return { model: withQuestion(nav, { ...question, qr: { text, dataUrl: undefined } }), requests }
}

/** What a submitted identity form asks Korri to do, when the form is complete. */
function identityRequest(actionId: string, form: IdentityForm): PicoRequest | undefined {
  const exportKey = picoIdentityRetiredKey(actionId, "export")
  const deleteKey = picoIdentityRetiredKey(actionId, "delete")
  if (actionId === PICO_IDENTITY_BACKUP_ACTION || exportKey !== undefined) {
    return form.password.trim() === "" ? undefined
      : { _tag: "ExportIdentityBackup", password: form.password, retiredPublicKey: exportKey }
  }
  if (actionId === PICO_IDENTITY_SWITCH_LOCAL_ACTION) {
    return !form.confirmed || form.secret.trim() === "" || form.password.trim() === "" ? undefined : {
      _tag: "SwitchIdentityFromBackup",
      encryptedSecret: form.secret,
      password: form.password,
      disposition: form.disposition,
      trustLossConfirmed: form.confirmed,
    }
  }
  if (actionId === PICO_IDENTITY_SWITCH_NIP46_ACTION) {
    return !form.confirmed || form.bunkerUri.trim() === "" ? undefined : {
      _tag: "SwitchIdentityToNip46",
      bunkerUri: form.bunkerUri,
      disposition: form.disposition,
      trustLossConfirmed: form.confirmed,
    }
  }
  if (deleteKey !== undefined) {
    return form.confirmed ? { _tag: "DeleteRetiredIdentity", publicKey: deleteKey, backupConfirmed: form.confirmed } : undefined
  }
  return undefined
}

const textRow = (korri: SurfaceModel, settingId: string): PicoTextSettingRowView | undefined =>
  picoTextSettingFrom(picoSettingsViewFromModel(korri), settingId)?.row

type OpenEditor = Extract<SettingsQuestion, { readonly _tag: "EditingText" | "ConfirmingClear" }>

const openEditor = (nav: PicoNavigation): OpenEditor | undefined => {
  const question = nav.settings?.question
  return question?._tag === "EditingText" || question?._tag === "ConfirmingClear" ? question : undefined
}

const saving = (korri: SurfaceModel, editor: TextEditor): boolean =>
  textRow(korri, editor.settingId)?.state === "saving"

/** Change what is typed. Keys do nothing while Korri saves, or behind the clear question. */
function editDraft(
  nav: PicoNavigation,
  korri: SurfaceModel,
  change: (draft: PicoTextDraft, row: PicoTextSettingRowView) => PicoTextDraft,
): PicoNavigation {
  const editor = openEditor(nav)
  if (editor?._tag !== "EditingText") return nav
  const row = textRow(korri, editor.settingId)
  if (row === undefined || row.state === "saving") return nav
  return withQuestion(nav, { ...editor, draft: change(editor.draft, row) })
}

function editIdentity(nav: PicoNavigation, change: (form: IdentityForm) => IdentityForm): PicoNavigation {
  const question = nav.settings?.question
  return question?._tag === "Identity" ? withQuestion(nav, { ...question, form: change(question.form) }) : nav
}

/** Attract leaves as soon as the screen no longer allows it. */
function settleIdle(nav: PicoNavigation, korri: SurfaceModel): PicoNavigation {
  return nav.idle._tag === "Attracting" && !canAttract(nav, korri)
    ? { ...nav, idle: { _tag: "Awake", activity: nav.idle.activity } }
    : nav
}

const closeMenu = (home: Home): Home => (home.menu === "Closed" ? home : { ...home, menu: "Closed" })
const withQuestion = (nav: PicoNavigation, question: SettingsQuestion): PicoNavigation =>
  nav.settings === undefined ? nav : { ...nav, settings: { ...nav.settings, question } }
const cycleMode = (nav: PicoNavigation): PicoNavigation => ({
  ...nav,
  home: { ...nav.home, mode: nav.home.mode === "shelf" ? "grid" : nav.home.mode === "grid" ? "hero" : "shelf" },
})
